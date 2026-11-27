import os
import shutil
import signal
import subprocess
import tempfile
import threading
import time

import config
import runtime
from diagnostics import parse_diagnostics

SOURCE_NAME = "main.fi"
SYSTEM_DIRS = ("/usr", "/etc/alternatives", "/etc/ld.so.cache")
SYMLINKS = (("usr/bin", "/bin"), ("usr/lib", "/lib"), ("usr/lib64", "/lib64"), ("usr/sbin", "/sbin"))


def bwrap_prefix(work_dir, extra):
    args = ["bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--clearenv"]
    for path in SYSTEM_DIRS:
        if os.path.exists(path):
            args += ["--ro-bind", path, path]
    for target, link in SYMLINKS:
        args += ["--symlink", target, link]
    args += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--bind", work_dir, "/work", "--chdir", "/work"]
    args += ["--setenv", "PATH", "/usr/bin", "--setenv", "HOME", "/work"]
    return args + extra


def limited(command, address_space, cpu_seconds):
    return [
        "prlimit",
        f"--as={address_space}",
        f"--cpu={int(cpu_seconds)}",
        f"--fsize={config.RUN_FILE_SIZE if address_space == config.RUN_ADDRESS_SPACE else 64 << 20}",
        f"--nproc={config.MAX_PROCESSES}",
        "--core=0",
    ] + command


def drop_privileges():
    os.setgid(config.SANDBOX_UID)
    os.setgroups([])
    os.setuid(config.SANDBOX_UID)


class Capture:
    def __init__(self, limit):
        self.limit = limit
        self.data = bytearray()
        self.truncated = False

    def pump(self, stream, on_overflow):
        while True:
            chunk = stream.read1(8192)
            if not chunk:
                return
            room = self.limit - len(self.data)
            if len(chunk) > room:
                self.data += chunk[:max(room, 0)]
                self.truncated = True
                on_overflow()
                return
            self.data += chunk

    def text(self):
        return self.data.decode("utf-8", errors="replace")


def kill_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run_confined(command, timeout_s, output_limit, stdin_path="/dev/null"):
    started = time.monotonic()
    with open(stdin_path, "rb") as stdin:
        process = subprocess.Popen(
            command, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            preexec_fn=lambda: (os.setsid(), drop_privileges()),
        )
    out, err = Capture(output_limit), Capture(output_limit)
    overflow = threading.Event()

    def on_overflow():
        overflow.set()
        kill_group(process)

    pumps = [
        threading.Thread(target=out.pump, args=(process.stdout, on_overflow), daemon=True),
        threading.Thread(target=err.pump, args=(process.stderr, on_overflow), daemon=True),
    ]
    for pump in pumps:
        pump.start()
    timed_out = False
    try:
        process.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        timed_out = True
    kill_group(process)
    process.wait()
    for pump in pumps:
        pump.join(timeout=2)
    return {
        "stdout": out.text(), "stderr": err.text(), "code": process.returncode,
        "timed_out": timed_out, "truncated": out.truncated or err.truncated,
        "ms": round((time.monotonic() - started) * 1000),
    }


def compile_command(work_dir):
    inner = limited(
        ["/firn/bin/firnc", f"/work/{SOURCE_NAME}", "-o", "/work/program"],
        config.COMPILE_ADDRESS_SPACE, config.COMPILE_TIMEOUT_S,
    )
    extra = [
        "--ro-bind", f"{runtime.RUNTIME_DIR}/lib", "/firn/lib",
        "--ro-bind", f"{runtime.RUNTIME_DIR}/bin/firnc", "/firn/bin/firnc",
        "--setenv", "FIRNLIB", "/firn/lib",
    ] + inner
    return bwrap_prefix(work_dir, extra)


def check_command(work_dir):
    inner = limited(
        ["/usr/bin/sh", "-c", 'exec /firn/bin/firnc "$@" > /dev/null', "check", f"/work/{SOURCE_NAME}", "--emit=fir-raw"],
        config.COMPILE_ADDRESS_SPACE, config.CHECK_TIMEOUT_S,
    )
    extra = [
        "--ro-bind", f"{runtime.RUNTIME_DIR}/lib", "/firn/lib",
        "--ro-bind", f"{runtime.RUNTIME_DIR}/bin/firnc", "/firn/bin/firnc",
        "--setenv", "FIRNLIB", "/firn/lib",
    ] + inner
    return bwrap_prefix(work_dir, extra)


def run_command(work_dir):
    inner = limited(["/work/program"], config.RUN_ADDRESS_SPACE, config.RUN_TIMEOUT_S + 2)
    return bwrap_prefix(work_dir, inner)


def validate_source(source):
    if not isinstance(source, str) or not source.strip():
        return "empty source"
    if len(source.encode("utf-8")) > config.MAX_SOURCE_BYTES:
        return "source too large"
    if "\0" in source:
        return "invalid character in source"
    return None


def stage_source(source):
    work_dir = tempfile.mkdtemp(prefix="firn-playground-")
    os.chmod(work_dir, 0o777)
    path = os.path.join(work_dir, SOURCE_NAME)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    os.chmod(path, 0o644)
    return work_dir


def execute(source):
    problem = validate_source(source)
    if problem:
        return {"ok": False, "stage": "input", "diagnostics": [], "compiler_output": problem}
    work_dir = stage_source(source)
    try:
        return compile_and_run(work_dir)
    finally:
        shutil.rmtree(work_dir, ignore_errors=True)


def check(source):
    problem = validate_source(source)
    if problem:
        return {"ok": False, "diagnostics": [], "message": problem}
    work_dir = stage_source(source)
    try:
        built = run_confined(check_command(work_dir), config.CHECK_TIMEOUT_S, config.MAX_OUTPUT_BYTES)
        log = built["stderr"] + built["stdout"]
        if built["timed_out"]:
            return {"ok": False, "diagnostics": [], "message": "compiler timed out"}
        return {"ok": built["code"] == 0, "diagnostics": parse_diagnostics(log, SOURCE_NAME), "compile_ms": built["ms"]}
    finally:
        shutil.rmtree(work_dir, ignore_errors=True)


def compile_and_run(work_dir):
    built = run_confined(compile_command(work_dir), config.COMPILE_TIMEOUT_S, config.MAX_OUTPUT_BYTES)
    log = built["stderr"] + built["stdout"]
    if built["timed_out"]:
        return {"ok": False, "stage": "compile", "diagnostics": [], "compiler_output": "compiler timed out", "compile_ms": built["ms"]}
    program = os.path.join(work_dir, "program")
    if built["code"] != 0 or not os.path.exists(program):
        return {
            "ok": False, "stage": "compile", "compiler_output": log, "compile_ms": built["ms"],
            "diagnostics": parse_diagnostics(log, SOURCE_NAME),
        }
    os.chmod(program, 0o755)
    ran = run_confined(run_command(work_dir), config.RUN_TIMEOUT_S, config.MAX_OUTPUT_BYTES)
    return {
        "ok": ran["code"] == 0 and not ran["timed_out"] and not ran["truncated"],
        "stage": "run", "diagnostics": parse_diagnostics(log, SOURCE_NAME), "compiler_output": log,
        "stdout": ran["stdout"], "stderr": ran["stderr"], "exit_code": ran["code"],
        "timed_out": ran["timed_out"], "truncated": ran["truncated"],
        "compile_ms": built["ms"], "run_ms": ran["ms"],
    }

import os
import shutil
import subprocess
import tempfile

import callgrind
import covdata as profiles

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))

CALLGRIND = [
    "valgrind", "-q", "--tool=callgrind", "--dump-instr=yes", "--dump-line=no",
    "--compress-strings=no", "--compress-pos=no", "--trace-children=yes",
]


class RunError(Exception):
    pass


def find_compiler():
    explicit = os.environ.get("FIRNC")
    candidates = [explicit] if explicit else [os.path.join(ROOT, "compiler", "target", "release", "firnc")]
    for path in candidates:
        if path and os.access(path, os.X_OK):
            return path
    raise RunError("firnc not found: set FIRNC or build compiler/target/release/firnc")


def build(source, exe, opt, test_mode):
    command = [find_compiler(), "--no-cache"]
    command.append("--no-opt" if opt == "dev" else f"--opt-level={opt}")
    if test_mode:
        command += ["--test", "--no-run"]
    command += [source, "-o", exe]
    env = dict(os.environ)
    env.setdefault("FIRNLIB", os.path.join(ROOT, "lib"))
    result = subprocess.run(command, capture_output=True, text=True, env=env)
    if result.returncode != 0:
        raise RunError(f"{source}: build failed\n{result.stderr.strip()[:2000]}")


def run_one(source, opt="dev", test_mode=False, args=(), timeout=900):
    if shutil.which("valgrind") is None:
        raise RunError("valgrind not found")
    work = tempfile.mkdtemp(prefix="firncov.")
    try:
        exe = os.path.join(work, "prog")
        build(source, exe, opt, test_mode)
        out = os.path.join(work, "cg.%p")
        try:
            result = subprocess.run(CALLGRIND + [f"--callgrind-out-file={out}", exe, *args],
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
            status = result.returncode
        except subprocess.TimeoutExpired:
            status = "timeout"
        costs = callgrind.read_all(os.path.join(work, "cg.*"))
        if not costs:
            raise RunError(f"{source}: callgrind produced no data (status {status})")
        return profiles.build(exe, costs, source, status)
    finally:
        shutil.rmtree(work, ignore_errors=True)

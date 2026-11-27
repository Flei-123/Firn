import codecs
import json
import os
import secrets
import shutil
import subprocess
import threading
import time

import config
import sandbox

TERMINAL_EVENTS = ("exit", "failed")
POLL_S = 0.1


class Session:
    def __init__(self, source, client):
        self.id = secrets.token_urlsafe(16)
        self.source = source
        self.client = client
        self.events = []
        self.cond = threading.Condition()
        self.lock = threading.Lock()
        self.process = None
        self.work_dir = None
        self.reason = None
        self.finished = False
        self.finished_at = None
        self.listeners = 0
        self.detached_since = time.monotonic()
        self.last_activity = time.monotonic()
        self.stdin_bytes = 0
        self.stdin_closed = False
        self.output_bytes = 0

    def emit(self, name, payload):
        with self.cond:
            self.events.append((name, json.dumps(payload)))
            self.cond.notify_all()

    def wait_events(self, index, timeout):
        with self.cond:
            self.cond.wait_for(lambda: len(self.events) > index, timeout)
            return self.events[index:]

    def attach(self):
        with self.lock:
            self.listeners += 1

    def detach(self):
        with self.lock:
            self.listeners -= 1
            self.detached_since = time.monotonic()

    def terminate(self, reason):
        with self.lock:
            if self.reason is None:
                self.reason = reason
            if self.process is not None:
                sandbox.kill_group(self.process)

    def send_input(self, text, eof):
        with self.lock:
            if self.process is None:
                return "not_ready"
            if self.finished or self.stdin_closed:
                return "closed"
            if eof:
                self.close_stdin_locked()
                return "ok"
            data = text.encode("utf-8")
            if self.stdin_bytes + len(data) > config.MAX_STDIN_BYTES:
                return "limit"
            try:
                self.process.stdin.write(data)
            except OSError:
                self.close_stdin_locked()
                return "closed"
            self.stdin_bytes += len(data)
            self.last_activity = time.monotonic()
            return "ok"

    def close_stdin_locked(self):
        self.stdin_closed = True
        try:
            self.process.stdin.close()
        except OSError:
            pass

    def take_output(self, size):
        with self.lock:
            room = max(0, config.MAX_OUTPUT_BYTES - self.output_bytes)
            taken = min(size, room)
            self.output_bytes += taken
            self.last_activity = time.monotonic()
            return taken

    def pump(self, stream, name):
        decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
        while True:
            chunk = stream.read(8192)
            if not chunk:
                break
            taken = self.take_output(len(chunk))
            text = decoder.decode(chunk[:taken])
            if text:
                self.emit(name, text)
            if taken < len(chunk):
                self.terminate("output_limit")
                break
        tail = decoder.decode(b"", True)
        if tail:
            self.emit(name, tail)

    def start_program(self):
        command = sandbox.run_command(self.work_dir, config.SESSION_CPU_S)
        with self.lock:
            if self.reason is not None:
                return False
            self.process = subprocess.Popen(
                command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
                preexec_fn=lambda: (os.setsid(), sandbox.drop_privileges()),
            )
        return True

    def abandoned(self, now):
        return self.listeners == 0 and now - self.detached_since > config.SESSION_ABANDON_S

    def enforce_limits(self, started):
        now = time.monotonic()
        if now - started > config.SESSION_TOTAL_TIMEOUT_S:
            self.terminate("timeout")
        elif now - self.last_activity > config.SESSION_IDLE_TIMEOUT_S:
            self.terminate("idle")
        elif self.abandoned(now):
            self.terminate("disconnected")

    def supervise(self, compile_ms):
        started = time.monotonic()
        pumps = [
            threading.Thread(target=self.pump, args=(self.process.stdout, "out"), daemon=True),
            threading.Thread(target=self.pump, args=(self.process.stderr, "err"), daemon=True),
        ]
        for pump in pumps:
            pump.start()
        while self.process.poll() is None:
            self.enforce_limits(started)
            try:
                self.process.wait(timeout=POLL_S)
            except subprocess.TimeoutExpired:
                pass
        sandbox.kill_group(self.process)
        for pump in pumps:
            pump.join(timeout=2)
        return self.exit_payload(compile_ms, started)

    def exit_payload(self, compile_ms, started):
        code = self.process.returncode
        reason = self.reason or ("signal" if code < 0 else "exit")
        return {
            "code": None if self.reason else code, "reason": reason,
            "compile_ms": compile_ms, "run_ms": round((time.monotonic() - started) * 1000),
        }

    def limits(self):
        return {
            "total_s": round(config.SESSION_TOTAL_TIMEOUT_S), "idle_s": round(config.SESSION_IDLE_TIMEOUT_S),
            "output_kib": config.MAX_OUTPUT_BYTES // 1024,
        }

    def run(self):
        try:
            terminal = self.execute()
        except Exception as failure:
            terminal = ("failed", {"ok": False, "stage": "internal", "diagnostics": [], "compiler_output": f"internal error: {failure}"})
        self.cleanup()
        self.emit(*terminal)

    def execute(self):
        self.work_dir = sandbox.stage_source(self.source)
        self.emit("status", {"stage": "compiling"})
        built, _, failure = sandbox.compile_program(self.work_dir)
        if failure:
            return "failed", failure
        if self.abandoned(time.monotonic()) or not self.start_program():
            return "exit", {"code": None, "reason": self.reason or "disconnected", "compile_ms": built["ms"], "run_ms": 0}
        self.emit("started", {"compile_ms": built["ms"], "limits": self.limits()})
        return "exit", self.supervise(built["ms"])

    def cleanup(self):
        with self.lock:
            if self.process is not None and not self.stdin_closed:
                self.close_stdin_locked()
            self.finished = True
            self.finished_at = time.monotonic()
        if self.work_dir:
            shutil.rmtree(self.work_dir, ignore_errors=True)

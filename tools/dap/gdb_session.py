import queue
import subprocess
import threading

from mi_parser import parse_record

COMMAND_TIMEOUT_SECONDS = 30


class GdbError(RuntimeError):
    pass


class GdbSession:
    def __init__(self, gdb_path="gdb"):
        self.process = subprocess.Popen(
            [gdb_path, "--interpreter=mi3", "--quiet", "--nx"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self.events = queue.Queue()
        self.pending = {}
        self.lock = threading.Lock()
        self.next_token = 1
        threading.Thread(target=self.read_loop, daemon=True).start()

    def read_loop(self):
        for line in self.process.stdout:
            record = parse_record(line.rstrip("\n"))
            if record is None:
                continue
            if record["kind"] == "^" and record["token"] in self.pending:
                self.pending.pop(record["token"]).put(record)
            elif record["kind"] == "*":
                self.events.put(record)
        self.events.put({"kind": "*", "class": "gdb-exit", "data": {}})

    def command(self, text):
        with self.lock:
            token = str(self.next_token)
            self.next_token += 1
            reply = queue.Queue()
            self.pending[token] = reply
            self.process.stdin.write("%s%s\n" % (token, text))
            self.process.stdin.flush()
        try:
            record = reply.get(timeout=COMMAND_TIMEOUT_SECONDS)
        except queue.Empty:
            raise GdbError("timeout: " + text)
        if record["class"] == "error":
            raise GdbError(record["data"].get("msg", "gdb error"))
        return record["data"]

    def close(self):
        if self.process.poll() is None:
            try:
                self.process.stdin.write("-gdb-exit\n")
                self.process.stdin.flush()
                self.process.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                self.process.kill()

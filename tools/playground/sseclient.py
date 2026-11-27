import json
import socket
import threading
import time


class Stream:
    def __init__(self, port, session_id, last=0):
        self.cond = threading.Condition()
        self.events = []
        self.closed = False
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=60)
        request = f"GET /api/session/{session_id}/events?last={last} HTTP/1.0\r\nHost: test\r\n\r\n"
        self.socket.sendall(request.encode())
        self.reader = self.socket.makefile("rb")
        self.status = int(self.reader.readline().split()[1])
        while self.reader.readline().strip():
            pass
        threading.Thread(target=self.read_loop, daemon=True).start()

    def read_loop(self):
        name, data = None, None
        try:
            for raw in self.reader:
                line = raw.decode("utf-8").rstrip("\n")
                if line.startswith("event: "):
                    name = line[7:]
                elif line.startswith("data: "):
                    data = json.loads(line[6:])
                elif line == "" and name:
                    with self.cond:
                        self.events.append((name, data))
                        self.cond.notify_all()
                    name, data = None, None
        except OSError:
            pass
        with self.cond:
            self.closed = True
            self.cond.notify_all()

    def text(self, kind="out"):
        with self.cond:
            return "".join(data for name, data in self.events if name == kind)

    def wait_for(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        with self.cond:
            while not predicate():
                left = deadline - time.monotonic()
                if left <= 0:
                    return False
                self.cond.wait(left)
            return True

    def wait_text(self, needle, timeout=10, kind="out"):
        return self.wait_for(lambda: needle in "".join(d for n, d in self.events if n == kind), timeout)

    def wait_terminal(self, timeout=15):
        if not self.wait_for(lambda: any(n in ("exit", "failed") for n, _ in self.events), timeout):
            return None
        with self.cond:
            return next((n, d) for n, d in self.events if n in ("exit", "failed"))

    def names(self):
        with self.cond:
            return [name for name, _ in self.events]

    def close(self):
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()

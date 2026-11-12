import json
import sys
import threading


class Channel:
    def __init__(self, reader=None, writer=None):
        self.reader = reader or sys.stdin.buffer
        self.writer = writer or sys.stdout.buffer
        self.write_lock = threading.Lock()
        self.seq = 0

    def read_message(self):
        length = None
        while True:
            line = self.reader.readline()
            if not line:
                return None
            line = line.strip()
            if not line:
                break
            name, _, value = line.decode("ascii").partition(":")
            if name.lower() == "content-length":
                length = int(value)
        if length is None:
            return None
        return json.loads(self.reader.read(length).decode("utf-8"))

    def send(self, message):
        with self.write_lock:
            self.seq += 1
            message["seq"] = self.seq
            body = json.dumps(message).encode("utf-8")
            self.writer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
            self.writer.flush()

    def respond(self, request, body=None, error=None):
        message = {
            "type": "response",
            "request_seq": request["seq"],
            "command": request["command"],
            "success": error is None,
        }
        if error is not None:
            message["message"] = error
        if body is not None:
            message["body"] = body
        self.send(message)

    def event(self, name, body=None):
        message = {"type": "event", "event": name}
        if body is not None:
            message["body"] = body
        self.send(message)

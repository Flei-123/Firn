"""Harness only: a minimal SMTP sink on 127.0.0.1:$VSPY_PORT; the received message goes to $VSPY_MAIL when set."""
import os, socketserver


class H(socketserver.StreamRequestHandler):
    def say(self, text):
        try:
            self.wfile.write(text.encode() + b"\r\n")
        except OSError:
            pass

    def handle(self):
        self.say("220 sink ESMTP")
        while True:
            line = self.rfile.readline().decode(errors="replace").strip()
            cmd = line.upper()
            if not line or cmd.startswith("QUIT"):
                return self.say("221 bye")
            if cmd.startswith("DATA"):
                self.say("354 go on")
                lines = []
                while True:
                    row = self.rfile.readline()
                    if row == b".\r\n":
                        break
                    lines.append(row[1:] if row.startswith(b"..") else row)
                if os.environ.get("VSPY_MAIL"):
                    open(os.environ["VSPY_MAIL"], "wb").write(b"".join(lines))
                self.say("250 queued")
            elif cmd.startswith("EHLO"):
                self.say("250 sink")
            else:
                self.say("250 ok")


class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


S(("127.0.0.1", int(os.environ["VSPY_PORT"])), H).serve_forever()

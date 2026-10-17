"""Harness only: serves pkg.zip and pkg.tar.gz built in memory on 127.0.0.1:$VSPY_PORT."""
import http.server, io, os, tarfile, zipfile

FILES = {
    "README.md": b"# demo package\n",
    "bin/tool.sh": b"#!/bin/sh\necho tool\n",
    "data/blob.bin": bytes(range(256)) * 20,
    "docs/guide.txt": b"read me twice\n" * 50,
}


def build_zip():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in FILES.items():
            z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), data, zipfile.ZIP_DEFLATED)
    return buf.getvalue()


def build_tgz():
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as t:
        for name, data in FILES.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            t.addfile(info, io.BytesIO(data))
    return buf.getvalue()


BLOBS = {"/pkg.zip": build_zip(), "/pkg.tar.gz": build_tgz()}


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        body = BLOBS.get(self.path)
        self.send_response(200 if body is not None else 404)
        body = body if body is not None else b"no"
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", int(os.environ["VSPY_PORT"])), H).serve_forever()

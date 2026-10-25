import argparse, base64, hashlib, os, shutil, sys, time, urllib.parse
from email.utils import formatdate
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from xml.sax.saxutils import escape

ARGS = None
NONCES = {}


def h(alg, data):
    d = hashlib.sha256 if alg.upper().startswith("SHA-256") else hashlib.md5
    return d(data.encode()).hexdigest()


def parse_auth(value):
    out = {}
    rest = value.split(" ", 1)[1] if " " in value else ""
    i = 0
    while i < len(rest):
        while i < len(rest) and rest[i] in ", ":
            i += 1
        j = rest.find("=", i)
        if j < 0:
            break
        key = rest[i:j].strip().lower()
        i = j + 1
        if i < len(rest) and rest[i] == '"':
            i += 1
            val = ""
            while i < len(rest) and rest[i] != '"':
                if rest[i] == "\\":
                    i += 1
                val += rest[i]
                i += 1
            i += 1
        else:
            k = rest.find(",", i)
            k = len(rest) if k < 0 else k
            val = rest[i:k].strip()
            i = k
        out[key] = val
    return out


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def fs_path(self, urlpath):
        p = urllib.parse.unquote(urlpath.split("?")[0])
        pre = ARGS.prefix
        if not (p == pre or p.startswith(pre + "/")):
            return None
        rel = p[len(pre):].strip("/")
        full = os.path.normpath(os.path.join(ARGS.root, rel))
        if not (full == ARGS.root or full.startswith(ARGS.root + os.sep)):
            return None
        return full

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        data = b""
        while len(data) < n:
            chunk = self.rfile.read(n - len(data))
            if not chunk:
                break
            data += chunk
        return data

    def reply(self, code, data=b"", headers=None):
        self.send_response(code)
        self.send_header("Content-Length", str(len(data)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def challenge(self, stale=False):
        mode = ARGS.auth
        if mode == "basic":
            self.reply(401, b"", {"WWW-Authenticate": 'Basic realm="fake", charset="UTF-8"'})
            return
        nonce = os.urandom(12).hex()
        NONCES[nonce] = 0
        alg = "SHA-256" if mode == "digest-sha256" else "MD5"
        qop = "" if mode == "digest-legacy" else ', qop="auth"'
        st = ", stale=true" if stale else ""
        self.reply(401, b"", {"WWW-Authenticate": 'Digest realm="fake", nonce="%s", opaque="op%s", algorithm=%s%s%s' % (nonce, nonce[:4], alg, qop, st)})

    def authorized(self):
        mode = ARGS.auth
        if mode == "none":
            return True
        v = self.headers.get("Authorization")
        if not v:
            return False
        if mode == "basic":
            if not v.startswith("Basic "):
                return False
            return base64.b64decode(v[6:]).decode() == ARGS.user + ":" + ARGS.password
        if not v.startswith("Digest "):
            return False
        a = parse_auth(v)
        alg = "SHA-256" if mode == "digest-sha256" else "MD5"
        if a.get("username") != ARGS.user or a.get("nonce") not in NONCES:
            return "stale"
        if a.get("uri") != self.path:
            return False
        ha1 = h(alg, "%s:%s:%s" % (ARGS.user, "fake", ARGS.password))
        ha2 = h(alg, "%s:%s" % (self.command, a["uri"]))
        if "qop" in a:
            nc = int(a["nc"], 16)
            if nc <= NONCES[a["nonce"]]:
                return False
            NONCES[a["nonce"]] = nc
            want = h(alg, "%s:%s:%s:%s:%s:%s" % (ha1, a["nonce"], a["nc"], a["cnonce"], a["qop"], ha2))
        else:
            want = h(alg, "%s:%s:%s" % (ha1, a["nonce"], ha2))
        return want == a.get("response")

    def dispatch(self):
        ARGS.count += 1
        ok = self.authorized()
        if ok == "stale":
            self.body()
            return self.challenge(True)
        if not ok:
            self.body()
            return self.challenge()
        fn = getattr(self, "do_" + self.command + "_", None)
        if self.path.startswith(ARGS.prefix + "/evil"):
            self.body()
            return self.reply(302, b"", {"Location": "http://127.0.0.1:1/steal"})
        full = self.fs_path(self.path)
        if full is None:
            self.body()
            return self.reply(404)
        fn(full)

    def href(self, full):
        rel = os.path.relpath(full, ARGS.root)
        p = ARGS.prefix + ("" if rel == "." else "/" + rel.replace(os.sep, "/"))
        if os.path.isdir(full):
            p += "/"
        return urllib.parse.quote(p)

    def entry(self, full, d):
        st = os.stat(full)
        isdir = os.path.isdir(full)
        res = "<%s:collection/>" % d if isdir else ""
        props = "<%s:resourcetype>%s</%s:resourcetype>" % (d, res, d)
        if not isdir:
            props += "<%s:getcontentlength>%d</%s:getcontentlength>" % (d, st.st_size, d)
            props += "<%s:getcontenttype>application/octet-stream</%s:getcontenttype>" % (d, d)
        props += "<%s:getlastmodified>%s</%s:getlastmodified>" % (d, formatdate(st.st_mtime, usegmt=True), d)
        props += '<%s:getetag>"%x-%x"</%s:getetag>' % (d, int(st.st_mtime), st.st_size, d)
        props += "<%s:displayname>%s</%s:displayname>" % (d, escape(os.path.basename(full)), d)
        return "<%s:response><%s:href>%s</%s:href><%s:propstat><%s:prop>%s</%s:prop><%s:status>HTTP/1.1 200 OK</%s:status></%s:propstat></%s:response>" % (d, d, escape(self.href(full)), d, d, d, props, d, d, d, d, d)

    def do_OPTIONS_(self, full):
        self.reply(200, b"", {"DAV": "1,2", "Allow": "OPTIONS,GET,PUT,PROPFIND,MKCOL,DELETE,MOVE,COPY"})

    def do_PROPFIND_(self, full):
        self.body()
        if not os.path.exists(full):
            return self.reply(404)
        if ARGS.redirect_dirs and os.path.isdir(full) and not self.path.split("?")[0].endswith("/"):
            return self.reply(301, b"", {"Location": self.path + "/"})
        depth = self.headers.get("Depth", "infinity")
        items = [full]
        if os.path.isdir(full) and depth != "0":
            names = sorted(os.listdir(full))
            for n in names:
                items.append(os.path.join(full, n))
                if depth == "infinity" and os.path.isdir(os.path.join(full, n)):
                    for r, ds, fs in os.walk(os.path.join(full, n)):
                        for x in sorted(ds + fs):
                            items.append(os.path.join(r, x))
        d = "D" if ARGS.ns == "prefix" else "d"
        if ARGS.ns == "default":
            xml = '<?xml version="1.0"?><multistatus xmlns="DAV:">' + "".join(self.entry(i, "x").replace("<x:", "<").replace("</x:", "</") for i in items) + "</multistatus>"
            xml = xml.replace("<response>", "<response>")
        else:
            xml = '<?xml version="1.0"?><%s:multistatus xmlns:%s="DAV:">%s</%s:multistatus>' % (d, d, "".join(self.entry(i, d) for i in items), d)
        self.reply(207, xml.encode(), {"Content-Type": "application/xml; charset=utf-8"})

    def do_GET_(self, full):
        if not os.path.isfile(full):
            return self.reply(404)
        with open(full, "rb") as f:
            data = f.read()
        rng = self.headers.get("Range")
        if rng and rng.startswith("bytes="):
            a, b = rng[6:].split("-")
            a = int(a)
            b = min(int(b), len(data) - 1) if b else len(data) - 1
            return self.reply(206, data[a:b + 1], {"Content-Range": "bytes %d-%d/%d" % (a, b, len(data))})
        self.reply(200, data, {"Content-Type": "application/octet-stream"})

    do_HEAD_ = do_GET_

    def do_PUT_(self, full):
        data = self.body()
        if os.path.isdir(full) or not os.path.isdir(os.path.dirname(full)):
            return self.reply(409)
        existed = os.path.exists(full)
        with open(full, "wb") as f:
            f.write(data)
        self.reply(204 if existed else 201)

    def do_MKCOL_(self, full):
        self.body()
        if os.path.exists(full):
            return self.reply(405)
        if not os.path.isdir(os.path.dirname(full)):
            return self.reply(409)
        os.mkdir(full)
        self.reply(201)

    def do_DELETE_(self, full):
        if not os.path.exists(full):
            return self.reply(404)
        shutil.rmtree(full) if os.path.isdir(full) else os.remove(full)
        self.reply(204)

    def transfer(self, full, move):
        self.body()
        dest = self.headers.get("Destination", "")
        u = urllib.parse.urlparse(dest)
        if u.netloc and u.netloc != self.headers.get("Host"):
            return self.reply(502)
        dfull = self.fs_path(u.path)
        if dfull is None or not os.path.exists(full):
            return self.reply(404 if not os.path.exists(full) else 400)
        if not os.path.isdir(os.path.dirname(dfull)):
            return self.reply(409)
        existed = os.path.exists(dfull)
        if existed and self.headers.get("Overwrite", "T") == "F":
            return self.reply(412)
        if existed:
            shutil.rmtree(dfull) if os.path.isdir(dfull) else os.remove(dfull)
        if move:
            shutil.move(full, dfull)
        elif os.path.isdir(full):
            shutil.copytree(full, dfull)
        else:
            shutil.copy(full, dfull)
        self.reply(204 if existed else 201)

    def do_MOVE_(self, full):
        self.transfer(full, True)

    def do_COPY_(self, full):
        self.transfer(full, False)


for m in ("OPTIONS", "PROPFIND", "GET", "HEAD", "PUT", "MKCOL", "DELETE", "MOVE", "COPY"):
    setattr(Handler, "do_" + m, Handler.dispatch)


def main():
    global ARGS
    p = argparse.ArgumentParser()
    p.add_argument("--root", required=True)
    p.add_argument("--port-file", required=True)
    p.add_argument("--auth", default="none", choices=["none", "basic", "digest", "digest-sha256", "digest-legacy"])
    p.add_argument("--user", default="alice")
    p.add_argument("--password", default="s3cret \"q\"")
    p.add_argument("--prefix", default="/dav")
    p.add_argument("--ns", default="prefix", choices=["prefix", "alt", "default"])
    p.add_argument("--redirect-dirs", action="store_true")
    ARGS = p.parse_args()
    ARGS.root = os.path.realpath(ARGS.root)
    ARGS.count = 0
    srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    with open(ARGS.port_file, "w") as f:
        f.write(str(srv.server_address[1]))
    srv.serve_forever()


main()

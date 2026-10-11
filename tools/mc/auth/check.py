# tools/mc/auth/check.py <xauth-binary> -- cross-check of mc_auth (Firn) against python:
#  * server hash vs hashlib + signed big-endian two's complement hex (random + edge cases)
#  * offline UUID vs md5 (version 3 / variant bits)
#  * session join against a LOCAL MOCK session server (plain http on loopback):
#    exact request (method, path, content type, JSON fields), 204 / 403 / 401 / 429 / 500,
#    refused connection, timeout, bad arguments and a non-loopback http URL (nothing sent)
# env MC_TAMPER=1 corrupts the expectations: the run must then FAIL (counter-check).
import hashlib, json, os, random, subprocess, sys, tempfile, threading, time, uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

X = sys.argv[1]
TAMPER = os.environ.get("MC_TAMPER") == "1"
fails = passes = 0

def ok(c, m):
    global fails, passes
    if c: passes += 1
    else:
        fails += 1; print("  FAIL  " + m)

def sh(d):  # java BigInteger.toString(16) of the digest read as signed
    v = int.from_bytes(d, "big", signed=True)
    return format(v, "x") if v >= 0 else "-" + format(-v, "x")

def run(*a):
    return subprocess.run([X, *a], capture_output=True, text=True, timeout=60)

tmp = tempfile.mkdtemp()
try:
    rnd = random.Random(7)
    neg = pos = lead0 = 0
    for i in range(400):
        sid = "".join(rnd.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(rnd.choice([0, 0, 5, 20])))
        sec = bytes(rnd.getrandbits(8) for _ in range(16))
        der = bytes(rnd.getrandbits(8) for _ in range(rnd.choice([0, 1, 162, 294])))
        open(tmp + "/s", "wb").write(sec); open(tmp + "/d", "wb").write(der)
        want = sh(hashlib.sha1(sid.encode() + sec + der).digest())
        if TAMPER: want = want[:-1] + ("0" if want[-1] != "0" else "1")
        r = run("hash", sid, tmp + "/s", tmp + "/d") if sid else run("hash", "", tmp + "/s", tmp + "/d")
        got = r.stdout.strip()
        ok(got == want, f"hash case {i}: got {got} want {want}")
        neg += want.startswith("-"); pos += not want.startswith("-"); lead0 += len(want.lstrip("-")) < 40
    ok(neg > 100 and pos > 100 and lead0 > 5, f"hash edge coverage neg={neg} pos={pos} short={lead0}")
    # the three well known strings (python value wins where the task text differed)
    open(tmp + "/e", "wb").write(b"")
    for name, want in (("Notch", "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"),
                       ("jeb_", "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"),
                       ("simon", "88e16a1019277b15d58faf0541e11910eb756f6")):
        ok(sh(hashlib.sha1(name.encode()).digest()) == want, f"python vector {name}")
        ok(run("hash", name, tmp + "/e", tmp + "/e").stdout.strip() == want, f"firn vector {name}")
    # zero digest edge is covered by unit test 2675 (digest_to_signed_hex)
    def offline(n):
        d = bytearray(hashlib.md5(b"OfflinePlayer:" + n.encode()).digest())
        d[6] = (d[6] & 15) | 0x30; d[8] = (d[8] & 63) | 0x80
        return str(uuid.UUID(bytes=bytes(d)))
    names = ["Notch", "jeb_", "a", "Player_1234567890"[:16]] + ["".join(rnd.choice("abcXYZ_019") for _ in range(rnd.randint(1, 16))) for _ in range(200)]
    for n in names:
        want = offline(n)
        if TAMPER: want = want.replace("-3", "-4", 1)
        ok(run("uuid", n).stdout.strip() == want, f"offline uuid {n}")
    ok(offline("Notch") == "b50ad385-829d-3141-a216-7e7d7539ba7f", "Notch known uuid")

    # ---- mock session server
    log = []
    mode = {"m": "ok"}
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a): pass
        def do_POST(self):
            n = int(self.headers.get("Content-Length", "0")); body = self.rfile.read(n)
            log.append((self.path, self.headers.get("Content-Type"), self.headers.get("Accept"), body))
            m = mode["m"]
            if m == "slow": time.sleep(3)
            code = {"ok": 204, "slow": 204, "forbidden": 403, "unauth": 401, "rate": 429, "boom": 500}[m]
            self.send_response(code)
            if code in (403, 401):
                b = json.dumps({"error": "ForbiddenOperationException", "errorMessage": "Invalid session"}).encode()
                self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
            else:
                self.send_header("Content-Length", "0"); self.end_headers()
    srv = HTTPServer(("127.0.0.1", 0), H); port = srv.server_address[1]
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}"
    pid = "069a79f444e94726a5befca90e38aaf5"
    dashed = "069a79f4-44e9-4726-a5be-fca90e38aaf5"
    tok = 'tok."en\\x'  # characters that need JSON escaping
    for m, want_code in (("ok", 0), ("forbidden", 2), ("unauth", 7), ("rate", 3), ("boom", 4)):
        mode["m"] = m; log.clear()
        r = run("join", base + "/", tok, dashed if m == "ok" else pid, "-1f2e3d")  # trailing slash base, dashed uuid
        lines = r.stdout.split("\n")
        ok(lines[0].startswith(f"code={want_code} "), f"join {m}: {r.stdout!r} {r.stderr!r}")
        ok(len(log) == 1, f"join {m}: exactly one request")
        if log:
            path, ct, acc, body = log[0]
            j = json.loads(body)
            ok(path == "/session/minecraft/join", f"join {m}: path {path}")
            ok(ct == "application/json", f"join {m}: content type {ct}")
            ok(j == {"accessToken": tok, "selectedProfile": pid, "serverId": "-1f2e3d" if not TAMPER else "x"}, f"join {m}: body {j}")
        if m == "forbidden":
            ok(lines[2] == "ForbiddenOperationException|Invalid session", f"join forbidden error fields {lines[2]!r}")
    mode["m"] = "ok"; log.clear()
    ok(run("join", "http://127.0.0.1:1", tok, pid, "1a").stdout.startswith("code=1 "), "join refused connection -> net error")
    mode["m"] = "ok"; log.clear()
    n0 = len(log)
    for args, want in (((base, "", pid, "1a"), 5), ((base, tok, "xyz", "1a"), 5), ((base, tok, pid, "ZZ"), 5),
                       ((base, tok, pid, "-"), 5), ((base, tok, pid, "1" * 42), 5), (("http://example.com", tok, pid, "1a"), 6)):
        r = run("join", *args)
        ok(r.stdout.startswith(f"code={want} "), f"join args {args}: {r.stdout!r}")
    ok(len(log) == n0, "bad arguments / non-loopback http URL: nothing was sent")
    srv.shutdown()
finally:
    import shutil; shutil.rmtree(tmp, ignore_errors=True)
print(f"  mc-auth python cross-check: {passes} passed, {fails} failed")
sys.exit(1 if fails else 0)

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import base64, gzip, http.client, os, random, subprocess, sys, time
import requests

BIN, WORK = sys.argv[1], sys.argv[2]
os.makedirs(WORK, exist_ok=True)
ok = total = 0
PORT = 20100 + random.randrange(0, 700)
STATIC = os.path.join(WORK, "static")
os.makedirs(os.path.join(STATIC, "sub"), exist_ok=True)
BLOB = bytes(random.Random(7).randrange(256) for _ in range(5000))
open(os.path.join(STATIC, "a.txt"), "w").write("static hello 0123456789\n")
open(os.path.join(STATIC, "blob.bin"), "wb").write(BLOB)
open(os.path.join(STATIC, "sub", "index.html"), "w").write("<p>index</p>" * 40)


def check(name, cond, detail=""):
    global ok, total
    total += 1
    if cond:
        ok += 1
    else:
        print("  FAIL", name, str(detail)[:300])


class Srv:
    def __init__(self, *args):
        global PORT
        PORT += 1
        self.port = PORT
        self.p = subprocess.Popen([BIN, str(PORT), STATIC] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.p.stdout.readline()
        time.sleep(0.1)
        self.base = "http://127.0.0.1:%d" % PORT

    def stop(self):
        self.p.kill()
        self.p.wait()


def raw(port, method, path, headers=None):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    c.request(method, path, headers=headers or {})
    r = c.getresponse()
    body = r.read()
    out = (r.status, dict((k.lower(), v) for k, v in r.getheaders()), body)
    c.close()
    return out


s = Srv()
B = s.base
g = requests.get

r = g(B + "/hello/a%20b%2Fc")
check("param decode", r.status_code == 200 and r.text == "hello a b/c", r.text)
r = g(B + "/hello/x/")
check("trailing slash", r.text == "hello x", r.text)
r = g(B + "/users/42")
check("json response", r.json() == {"id": "42"} and r.headers["Content-Type"] == "application/json", r.text)
r = g(B + "/nope")
check("404", r.status_code == 404 and r.text == "not found\n")
r = g(B + "/nope", headers={"Accept": "application/json"})
check("404 json", r.status_code == 404 and r.json() == {"error": "not found"})
r = requests.post(B + "/hello/x")
check("405", r.status_code == 405 and r.headers["Allow"] == "GET", r.headers)
r = requests.options(B + "/hello/x")
check("OPTIONS allow", r.status_code == 204 and r.headers["Allow"] == "GET", r.status_code)
st, h, body = raw(s.port, "HEAD", "/hello/zed")
check("HEAD", st == 200 and body == b"" and h["content-length"] == "9", (st, h))
for m in ["PUT", "PATCH", "DELETE", "POST"]:
    r = requests.request(m, B + "/echo/a/b%20c", data="payload")
    check("any " + m, r.text == m + " /echo/a/b%20c payload", r.text)

r = requests.post(B + "/users", json={"name": "Zoë \"q\" \\ \n", "age": 31})
check("json body", r.status_code == 201 and r.json() == {"name": "Zoë \"q\" \\ \n", "age": 31}, r.text)
r = requests.post(B + "/users", data="{broken")
check("bad json 400", r.status_code == 400 and r.json() == {"error": "bad json"}, r.text)
r = requests.post(B + "/users", json={"name": "n"})
check("missing int default", r.json()["age"] == -1, r.text)

r = g(B + "/q", params={"a": "x&y z", "b": "é%"})
check("query decode", r.text == "x&y z|é%", r.text)
r = g(B + "/q?a=1&a=2&b")
check("query first wins + empty", r.text == "1|", r.text)
r = requests.post(B + "/form", data={"x": "a b&c", "y": "ü=1"})
check("form decode", r.text == "a b&c|ü=1", r.text)

ss = requests.Session()
r = ss.get(B + "/cookies")
check("cookie set", "seen" in ss.cookies and ss.cookies["seen"] == "yes%20please", ss.cookies)
r = ss.get(B + "/cookies")
check("cookie read", r.text == "old=yes please", r.text)
check("cookie flags", "HttpOnly" in r.headers["Set-Cookie"] and "Max-Age=3600" in r.headers["Set-Cookie"], r.headers)

ss = requests.Session()
vals = [ss.get(B + "/session").text for _ in range(4)]
check("session counts", vals == ["1", "2", "3", "4"], vals)
sid = ss.cookies["wf_sid"]
check("session cookie signed", "." in sid and len(sid.split(".")[1]) == 64)
bad = requests.Session()
bad.cookies.set("wf_sid", sid[:-4] + "0000")
check("forged session refused", bad.get(B + "/session").text == "1")
other = requests.Session()
other.cookies.set("wf_sid", "a" * 32 + "." + "b" * 64)
check("unknown session refused", other.get(B + "/session").text == "1")
ss.get(B + "/logout")
check("logout resets", ss.get(B + "/session").text == "1")

st, h, body = raw(s.port, "GET", "/big", {"Accept-Encoding": "gzip"})
check("gzip", h.get("content-encoding") == "gzip" and gzip.decompress(body) == b"the quick brown fox jumps over the lazy dog\n" * 400 and len(body) < 400, (h, len(body)))
st, h, body = raw(s.port, "GET", "/big", {"Accept-Encoding": "zstd, gzip"})
check("zstd", h.get("content-encoding") == "zstd" and body[:4] == b"\x28\xb5\x2f\xfd", h)
try:
    import zstandard
    check("zstd roundtrip", zstandard.ZstdDecompressor().decompress(body, max_output_size=100000) == b"the quick brown fox jumps over the lazy dog\n" * 400)
except ImportError:
    pass
st, h, body = raw(s.port, "GET", "/big", {"Accept-Encoding": "identity"})
check("identity", "content-encoding" not in h and len(body) == 17600, h)
check("requests auto gzip", requests.get(B + "/big").text.count("fox") == 400)
st, h, body = raw(s.port, "GET", "/hello/x", {"Accept-Encoding": "gzip"})
check("small not compressed", "content-encoding" not in h)

r = g(B + "/static/a.txt")
etag = r.headers["ETag"]
check("static 200", r.status_code == 200 and r.text == "static hello 0123456789\n" and r.headers["Accept-Ranges"] == "bytes" and r.headers["Content-Type"].startswith("text/plain"), r.headers)
r = g(B + "/static/a.txt", headers={"If-None-Match": etag})
check("etag 304", r.status_code == 304 and r.content == b"", r.status_code)
r = g(B + "/static/a.txt", headers={"If-None-Match": '"other"'})
check("etag mismatch 200", r.status_code == 200)
r = g(B + "/static/blob.bin", headers={"Range": "bytes=100-199"})
check("range 206", r.status_code == 206 and r.content == BLOB[100:200] and r.headers["Content-Range"] == "bytes 100-199/5000", r.headers)
r = g(B + "/static/blob.bin", headers={"Range": "bytes=4990-"})
check("range open end", r.status_code == 206 and r.content == BLOB[4990:], r.headers)
r = g(B + "/static/blob.bin", headers={"Range": "bytes=-10"})
check("range suffix", r.status_code == 206 and r.content == BLOB[-10:])
r = g(B + "/static/blob.bin", headers={"Range": "bytes=0-99999"})
check("range clamped", r.status_code == 206 and r.content == BLOB and r.headers["Content-Range"] == "bytes 0-4999/5000")
r = g(B + "/static/blob.bin", headers={"Range": "bytes=5000-5001"})
check("range 416", r.status_code == 416 and r.headers["Content-Range"] == "bytes */5000", r.headers)
r = g(B + "/static/blob.bin", headers={"Range": "bytes=9-3"})
check("range reversed 416", r.status_code == 416)
r = g(B + "/static/blob.bin")
check("binary exact", r.content == BLOB and r.headers["Content-Type"] == "application/octet-stream")
r = g(B + "/static/sub/")
check("dir index", r.status_code == 200 and r.text.startswith("<p>index</p>") and r.headers["Content-Type"].startswith("text/html"))
for evil in ["/static/../a.txt", "/static/%2e%2e/%2e%2e/etc/hostname", "/static/sub/../../x", "/static/..%2f..%2fetc%2fhostname", "/static/a.txt%00", "/static/..\\x"]:
    st, h, body = raw(s.port, "GET", evil)
    check("traversal " + evil, st == 404, (st, body[:40]))
st, h, body = raw(s.port, "GET", "//etc/hostname")
check("abs path not static", st == 404)
st, h, body = raw(s.port, "GET", "/static//etc/hostname")
check("double slash stays in root", st == 404, (st, body[:40]))
st, h, body = raw(s.port, "POST", "/static/a.txt")
check("static POST 404", st == 404)
st, h, body = raw(s.port, "HEAD", "/static/a.txt")
check("static HEAD", st == 200 and body == b"" and h["content-length"] == "24")

r = g(B + "/api/secret")
check("auth 401", r.status_code == 401 and "Basic" in r.headers["WWW-Authenticate"])
r = g(B + "/api/secret", auth=("admin", "pw"))
check("basic ok", r.status_code == 200 and r.json() == {"secret": 42})
r = g(B + "/api/secret", auth=("admin", "nope"))
check("basic wrong", r.status_code == 401)
r = g(B + "/api/secret", headers={"Authorization": "Bearer tok"})
check("bearer ok", r.status_code == 200)
r = g(B + "/api/secret", headers={"Authorization": "Bearer other"})
check("bearer wrong", r.status_code == 401)
r = g(B + "/api/secret", headers={"Authorization": "Basic !!!notbase64"})
check("garbage basic", r.status_code == 401)
r = g(B + "/api/v1/secret", auth=("admin", "pw"))
check("nested group mw", r.status_code == 200 and r.headers.get("X-Group") == "v1", r.headers)
r = g(B + "/api/v1/secret")
check("nested group auth", r.status_code == 401 and "X-Group" not in r.headers)
r = g(B + "/hello/x")
check("outside group", "X-Group" not in r.headers)

O = {"Origin": "http://example.test"}
r = g(B + "/hello/x", headers=O)
check("cors allowed", r.headers.get("Access-Control-Allow-Origin") == "http://example.test" and r.headers.get("Vary") == "Origin", r.headers)
r = g(B + "/hello/x", headers={"Origin": "http://evil.test"})
check("cors denied no header", r.status_code == 200 and "Access-Control-Allow-Origin" not in r.headers)
r = requests.options(B + "/users", headers=dict(O, **{"Access-Control-Request-Method": "POST"}))
check("preflight", r.status_code == 204 and "POST" in r.headers["Access-Control-Allow-Methods"] and r.headers["Access-Control-Max-Age"] == "600" and "Authorization" in r.headers["Access-Control-Allow-Headers"], r.headers)
r = requests.options(B + "/users", headers={"Origin": "http://evil.test", "Access-Control-Request-Method": "POST"})
check("preflight denied", r.status_code == 403)
r = g(B + "/hello/x")
check("no origin no cors", "Access-Control-Allow-Origin" not in r.headers)

r = g(B + "/jump", allow_redirects=False)
check("redirect", r.status_code == 302 and r.headers["Location"] == "/hello/redirected")
r = g(B + "/jump")
check("redirect followed", r.text == "hello redirected")
r = g(B + "/boom")
check("status_error", r.status_code == 422 and r.text == "teapot\n")

r = g(B + "/", params={"who": "<Bob & \"Al\">", "admin": "1"})
check("template escape/if/each", r.text == "<h1>Hello &lt;Bob &amp; &quot;Al&quot;&gt;</h1><p>admin</p><ul><li>one &amp; two</li><li>&lt;three&gt;</li></ul>", r.text)
r = g(B + "/")
check("template else", "<p>guest</p>" in r.text)

c = http.client.HTTPConnection("127.0.0.1", s.port, timeout=5)
for i in range(30):
    c.request("GET", "/hello/k%d" % i)
    rr = c.getresponse()
    if rr.read() != ("hello k%d" % i).encode():
        check("keepalive", False, i)
        break
else:
    check("keepalive 30 requests", True)
c.close()
c = http.client.HTTPConnection("127.0.0.1", s.port, timeout=5)
c.request("POST", "/echo/x", body="chunked-body", headers={"Transfer-Encoding": "chunked"}, encode_chunked=True)
check("chunked request body", c.getresponse().read() == b"POST /echo/x chunked-body")
c.close()
r = g(B + "/hello/x")
check("still alive", r.status_code == 200)
r = requests.get(B + "/stop")
s.p.wait(timeout=5)
check("stop exits", s.p.returncode == 0, s.p.returncode)
s.stop()

s2 = Srv("rate")
codes = [requests.get(s2.base + "/hello/x") for _ in range(7)]
check("rate limit 5 ok", [x.status_code for x in codes] == [200] * 5 + [429] * 2, [x.status_code for x in codes])
check("rate headers", codes[0].headers["X-RateLimit-Limit"] == "5" and codes[0].headers["X-RateLimit-Remaining"] == "4" and codes[4].headers["X-RateLimit-Remaining"] == "0", codes[0].headers)
check("rate retry-after", 0 < int(codes[5].headers["Retry-After"]) <= 60, codes[5].headers)
s2.stop()

print("webfw: %d/%d checks" % (ok, total))
sys.exit(0 if ok == total else 1)

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/tls/server_check.py -- lib/tls/server.fi against clients this
# repository did not write: `openssl s_client`, curl, and Python's `ssl`
# (OpenSSL). Every refusal is a counter-check: the handshake MUST fail, and
# with the right alert.
#
#   python3 tools/tls/server_check.py <server_main binary> <workdir>
import os, socket, ssl, subprocess, sys, threading, time, random, datetime

BIN, WORK = sys.argv[1], sys.argv[2]
ok = 0
total = 0
refusals = 0
PORT = 18400 + random.randrange(0, 500)


def check(name, cond, detail=""):
    global ok, total
    total += 1
    if cond:
        ok += 1
    else:
        print("  FAIL", name, detail)


def sh(cmd, inp=None, timeout=20):
    r = subprocess.run(cmd, input=inp, capture_output=True, timeout=timeout)
    return r.returncode, r.stdout.decode("latin1"), r.stderr.decode("latin1")


def certs():
    os.makedirs(WORK, exist_ok=True)
    k = os.path.join(WORK, "ec.key")
    c = os.path.join(WORK, "ec.crt")
    sh(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", k])
    sh(["openssl", "req", "-new", "-x509", "-key", k, "-out", c, "-days", "30", "-subj",
        "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"])
    sh(["openssl", "pkcs8", "-topk8", "-nocrypt", "-in", k, "-out", os.path.join(WORK, "ec8.key")])
    # a two-certificate chain: CA -> leaf
    ca_k = os.path.join(WORK, "ca.key")
    ca_c = os.path.join(WORK, "ca.crt")
    sh(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", ca_k])
    sh(["openssl", "req", "-new", "-x509", "-key", ca_k, "-out", ca_c, "-days", "30", "-subj",
        "/CN=Firn Test CA", "-addext", "basicConstraints=critical,CA:TRUE",
        "-addext", "keyUsage=critical,keyCertSign"])
    lk = os.path.join(WORK, "leaf.key")
    lcsr = os.path.join(WORK, "leaf.csr")
    lc = os.path.join(WORK, "leaf.crt")
    sh(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", lk])
    sh(["openssl", "req", "-new", "-key", lk, "-out", lcsr, "-subj", "/CN=localhost"])
    ext = os.path.join(WORK, "leaf.ext")
    open(ext, "w").write("subjectAltName=DNS:localhost,IP:127.0.0.1\n")
    sh(["openssl", "x509", "-req", "-in", lcsr, "-CA", ca_c, "-CAkey", ca_k, "-CAcreateserial",
        "-out", lc, "-days", "30", "-extfile", ext])
    chain = os.path.join(WORK, "chain.crt")
    open(chain, "w").write(open(lc).read() + open(ca_c).read())
    # an RSA certificate (must be refused at load time) and a wrong key
    rk = os.path.join(WORK, "rsa.key")
    rc = os.path.join(WORK, "rsa.crt")
    sh(["openssl", "req", "-new", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", rk,
        "-out", rc, "-days", "30", "-subj", "/CN=localhost"])
    return dict(key=k, crt=c, key8=os.path.join(WORK, "ec8.key"), ca=ca_c, chain=chain,
                leafkey=lk, rsakey=rk, rsacrt=rc)


class Server:
    def __init__(self, crt, key, n=0):
        global PORT
        PORT += 1
        self.port = PORT
        self.p = subprocess.Popen([BIN, str(PORT), crt, key, str(n)],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        line = self.p.stderr.readline().decode()
        self.ready = line.startswith("ready")
        self.first = line

    def lines(self):
        try:
            self.p.kill()
        except Exception:
            pass
        return self.p.stderr.read().decode()

    def stop(self):
        self.p.kill()
        self.p.wait()


def s_client(port, *extra, inp=b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"):
    return sh(["openssl", "s_client", "-connect", "127.0.0.1:%d" % port, "-servername",
               "localhost", "-ign_eof"] + list(extra), inp=inp)


C = certs()

# 0. loading: the RSA certificate and a key that does not belong are refused
for name, crt, key in [("RSA certificate refused", C["rsacrt"], C["rsakey"]),
                       ("foreign key refused", C["crt"], C["leafkey"])]:
    r = subprocess.run([BIN, "1", crt, key, "1"], capture_output=True, timeout=10)
    check(name, r.returncode == 4 and b"config refused" in r.stderr)
    refusals += 1

srv = Server(C["crt"], C["key"])
check("server starts", srv.ready, srv.first)

# 1. both suites, openssl s_client
for suite in ["TLS_AES_128_GCM_SHA256", "TLS_CHACHA20_POLY1305_SHA256"]:
    rc, out, err = s_client(srv.port, "-tls1_3", "-ciphersuites", suite)
    check("s_client " + suite, ("Cipher is " + suite) in out and "hello from firn tls" in out,
          out[-400:])

# 2. HelloRetryRequest: first share P-256, X25519 only in the groups
rc, out, err = s_client(srv.port, "-tls1_3", "-groups", "P-256:X25519", "-msg")
check("HelloRetryRequest -> X25519", "hello from firn tls" in out and "X25519" in out, out[-300:])
check("HelloRetryRequest was sent", out.count("ServerHello") >= 2, "")

# 3. curl with ALPN h2,http/1.1 -> http/1.1, and the chain verified against a CA
srv2 = Server(C["chain"], C["leafkey"])
rc, out, err = sh(["curl", "-sS", "--http2", "--cacert", C["ca"], "https://localhost:%d/" % srv2.port,
                   "--resolve", "localhost:%d:127.0.0.1" % srv2.port, "-w", " %{http_version}"])
check("curl, verified chain CA->leaf, ALPN picks http/1.1", out.startswith("hello from firn tls") and out.endswith(" 1.1"),
      out + err)

# 4. Python ssl: verified chain + hostname, 1 MiB echo, both suites
for suite in ["TLS_AES_128_GCM_SHA256", "TLS_CHACHA20_POLY1305_SHA256"]:
    ctx = ssl.create_default_context(cafile=C["ca"])
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    # Python cannot restrict TLS 1.3 suites; OpenSSL's order decides. Use
    # openssl for the suite and Python for the volume.
    s = ctx.wrap_socket(socket.create_connection(("127.0.0.1", srv2.port)), server_hostname="localhost")
    data = os.urandom(1 << 20)
    got = bytearray()

    def rd():
        while len(got) < len(data):
            b = s.recv(65536)
            if not b:
                break
            got.extend(b)
    th = threading.Thread(target=rd)
    th.start()
    s.sendall(data)
    th.join(20)
    check("python ssl 1 MiB echo (%s)" % s.cipher()[0], bytes(got) == data, "%d" % len(got))
    s.close()
    break

# 5. PKCS #8 key
srv3 = Server(C["crt"], C["key8"])
rc, out, err = s_client(srv3.port, "-tls1_3")
check("PKCS#8 key", "hello from firn tls" in out)
srv3.stop()

# 6. KeyUpdate: 'K' (interactive s_client) asks for an update in both
#    directions; data has to flow before and after it.
p = subprocess.Popen(["openssl", "s_client", "-connect", "127.0.0.1:%d" % srv.port, "-tls1_3"],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
for line in [b"abc\n", b"K\n", b"xyz\n"]:
    time.sleep(0.6)
    p.stdin.write(line)
    p.stdin.flush()
time.sleep(1.0)
p.stdin.write(b"QUIT\n")
p.stdin.flush()
time.sleep(0.8)
try:
    p.stdin.close()
except Exception:
    pass
out = p.stdout.read().decode("latin1"); p.wait(10)
after = out.split("KEYUPDATE")[-1]
check("KeyUpdate (update_requested), data before and after",
      "KEYUPDATE" in out and "abc" in out and "xyz" in after, out[-600:])

# 7. refusals, each with the right alert
def refused(name, alert, *extra):
    global refusals
    rc, out, err = s_client(srv.port, *extra)
    txt = out + err
    good = "hello from firn tls" not in txt and alert in txt
    check("REFUSED " + name, good, txt[-300:])
    refusals += 1


refused("TLS 1.2 only", "protocol version", "-tls1_2")
refused("AES-256-GCM only", "handshake failure", "-tls1_3", "-ciphersuites", "TLS_AES_256_GCM_SHA384")
refused("P-256 only (no X25519 at all)", "handshake failure", "-tls1_3", "-groups", "P-256")
refused("RSA-PSS signatures only", "handshake failure", "-tls1_3", "-sigalgs", "rsa_pss_rsae_sha256")
refused("ALPN h2 only", "no application protocol", "-tls1_3", "-alpn", "h2")

# plaintext HTTP to the TLS port: an alert, never a response
s = socket.create_connection(("127.0.0.1", srv.port))
s.sendall(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
s.settimeout(5)
resp = b""
try:
    while True:
        b = s.recv(4096)
        if not b:
            break
        resp += b
except Exception:
    pass
check("REFUSED plaintext HTTP -> alert record", resp[:1] == b"\x15" and b"HTTP" not in resp, repr(resp[:20]))
refusals += 1

# 8. a man in the middle that flips ONE bit of the client's first
#    application record: the server must answer bad_record_mac (20)
def mitm(listen_port, target, flip_after_ms):
    ls = socket.socket()
    ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    ls.bind(("127.0.0.1", listen_port))
    ls.listen(1)
    res = {}

    def run():
        c, _ = ls.accept()
        u = socket.create_connection(("127.0.0.1", target))
        state = {"n": 0, "flipped": False}
        buf = bytearray()

        def up():
            nonlocal buf
            while True:
                d = c.recv(65536)
                if not d:
                    u.shutdown(socket.SHUT_WR)
                    return
                buf += d
                out = bytearray()
                while len(buf) >= 5:
                    ln = int.from_bytes(buf[3:5], "big")
                    if len(buf) < 5 + ln:
                        break
                    rec = bytearray(buf[:5 + ln])
                    buf = buf[5 + ln:]
                    if rec[0] == 23:
                        state["n"] += 1
                        # record 1 = client Finished, record 2 = first data
                        if state["n"] == 2 and not state["flipped"]:
                            rec[5 + ln // 2] ^= 0x04
                            state["flipped"] = True
                    out += rec
                u.sendall(out)

        def down():
            data = bytearray()
            while True:
                d = u.recv(65536)
                if not d:
                    break
                data += d
                c.sendall(d)
            res["down"] = bytes(data)
            c.close()
        t1 = threading.Thread(target=up, daemon=True)
        t2 = threading.Thread(target=down, daemon=True)
        t1.start()
        t2.start()
        t2.join(15)
    th = threading.Thread(target=run, daemon=True)
    th.start()
    return th, res


PORT += 1
mp = PORT
th, res = mitm(mp, srv.port, 0)
time.sleep(0.2)
rc, out, err = s_client(mp, "-tls1_3")
th.join(15)
txt = out + err
check("REFUSED one flipped bit in an application record -> bad record mac",
      "hello from firn tls" not in txt and "bad record mac" in txt, txt[-300:])
refusals += 1

log = srv.lines() + srv2.lines()
# the server's own view of the alerts it sent
check("server log names alert 20 (bad_record_mac)", "alert_out 20 " in log, log[-500:])
check("server log names alert 70 (protocol_version)", "alert_out 70 " in log)
check("server log names alert 120 (no_application_protocol)", "alert_out 120 " in log)

print("   %d / %d TLS server cases, of them %d refusals" % (ok, total, refusals))
print(("TLS SERVER OK: %d / %d, refusals %d" if ok == total else "TLS SERVER FAILED: %d / %d, refusals %d")
      % (ok, total, refusals))
sys.exit(0 if ok == total else 1)

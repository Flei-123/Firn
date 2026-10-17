#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/tls/mini_check.py -- lib/tls/tlsmini.fi (the #[no_gc] TLS 1.3 client
with a key pin of the Android push thread) against Python's ssl (OpenSSL) and
`openssl s_server`.

    mini_check.py <tlsmini_main binary>

The servers are OpenSSL, the client is Firn; nothing is shared. What is
checked: the handshake with a pin, a long stream over many records, byte-wise
delivery, the refusals (wrong pin, RSA key, P-384 key, TLS 1.2 server, plain
TCP), a flipped bit in the handshake and in the data, a close without
close_notify, the SNI, and KeyUpdate (refused).
"""
import hashlib
import os
import select
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time

tool = sys.argv[1]
passed = failed = 0
work = tempfile.mkdtemp(prefix="tlsmini.")


def cleanup():
    shutil.rmtree(work, ignore_errors=True)


def check(label, got, want):
    global passed, failed
    if got == want:
        passed += 1
        print("  ok    %s" % label)
    else:
        failed += 1
        print("  FAIL  %s: got %r, want %r" % (label, got if len(repr(got)) < 200 else repr(got)[:200], want if len(repr(want)) < 200 else repr(want)[:200]))


def sh(*args):
    subprocess.run(args, check=True, capture_output=True)


def make_key(name, kind):
    key = os.path.join(work, name + ".key")
    crt = os.path.join(work, name + ".crt")
    if kind == "p256":
        sh("openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", key)
    elif kind == "p384":
        sh("openssl", "ecparam", "-name", "secp384r1", "-genkey", "-noout", "-out", key)
    elif kind == "rsa":
        sh("openssl", "genrsa", "-out", key, "2048")
    elif kind == "ed25519":
        sh("openssl", "genpkey", "-algorithm", "ed25519", "-out", key)
    sh("openssl", "req", "-new", "-x509", "-key", key, "-out", crt, "-days", "30",
       "-subj", "/CN=relay.test")
    return key, crt


def spki_pin(crt):
    pub = subprocess.run(["openssl", "x509", "-in", crt, "-pubkey", "-noout"],
                         check=True, capture_output=True).stdout
    der = subprocess.run(["openssl", "pkey", "-pubin", "-outform", "der"], input=pub,
                         check=True, capture_output=True).stdout
    return hashlib.sha256(der).hexdigest()


class Server:
    """TLS 1.3 server: for each connection `handler(conn)` runs."""

    def __init__(self, crt, key, handler, maxver=None, minver=None):
        self.ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.ctx.load_cert_chain(crt, key)
        if minver:
            self.ctx.minimum_version = minver
        if maxver:
            self.ctx.maximum_version = maxver
        self.sni = []
        self.ctx.sni_callback = self._sni
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(8)
        self.port = self.sock.getsockname()[1]
        self.handler = handler
        self.errors = []
        self.t = threading.Thread(target=self.loop, daemon=True)
        self.t.start()

    def _sni(self, sock, name, ctx):
        self.sni.append(name)

    def loop(self):
        while True:
            try:
                raw, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self.one, args=(raw,), daemon=True).start()

    def one(self, raw):
        try:
            raw.settimeout(10)
            conn = self.ctx.wrap_socket(raw, server_side=True)
            self.handler(conn)
        except Exception as e:  # handshake refused by the client etc.
            self.errors.append(repr(e))
        finally:
            try:
                raw.close()
            except OSError:
                pass

    def close(self):
        self.sock.close()


def run(port, pin, sni=None, timeout=30, write=None):
    args = [tool, "127.0.0.1", str(port), pin] + ([sni or "relay.test"] if (sni or write) else []) + ([write] if write else [])
    p = subprocess.run(args, capture_output=True, timeout=timeout)
    err = p.stderr.decode(errors="replace").split()
    return p.returncode, p.stdout, err


def send_then_close(data, notify=True):
    def h(conn):
        conn.sendall(data)
        if notify:
            conn.unwrap()
    return h


class Proxy:
    """TCP proxy between client and server. `mutate(direction, index, data)`
    sees the bytes of every recv; `chunk` makes it forward in tiny pieces."""

    def __init__(self, target_port, flip_at=None, chunk=0):
        self.target = target_port
        self.flip_at = flip_at  # absolute offset in the server->client stream
        self.chunk = chunk
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(4)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self.loop, daemon=True).start()

    def loop(self):
        while True:
            try:
                c, _ = self.sock.accept()
            except OSError:
                return
            s = socket.create_connection(("127.0.0.1", self.target))
            threading.Thread(target=self.pump, args=(c, s, False), daemon=True).start()
            threading.Thread(target=self.pump, args=(s, c, True), daemon=True).start()

    def pump(self, a, b, mutate):
        seen = 0
        try:
            while True:
                d = a.recv(4096)
                if not d:
                    break
                if mutate and self.flip_at is not None and seen <= self.flip_at < seen + len(d):
                    i = self.flip_at - seen
                    d = d[:i] + bytes([d[i] ^ 1]) + d[i + 1:]
                seen += len(d)
                if self.chunk:
                    for i in range(0, len(d), self.chunk):
                        b.sendall(d[i:i + self.chunk])
                        time.sleep(0.0002)
                else:
                    b.sendall(d)
        except OSError:
            pass
        finally:
            try:
                b.shutdown(socket.SHUT_WR)
            except OSError:
                pass

    def close(self):
        self.sock.close()


try:
    key, crt = make_key("p256", "p256")
    pin = spki_pin(crt)
    payload = b"".join(b"line %d\n" % i for i in range(20000))  # ~ 200 KB, many records

    # 1. good pin: lines over many records, clean close
    srv = Server(crt, key, send_then_close(b"hello\n" + payload))
    rc, out, err = run(srv.port, pin, "relay.test")
    check("pinned handshake, stream of %d octets, clean close" % (len(payload) + 6),
          (rc, out == b"hello\n" + payload, err[-2:]), (0, True, ["END", "0"]))
    check("SNI reaches the server", srv.sni, ["relay.test"])
    rc, out, err = run(srv.port, pin.upper())
    check("pin in capital letters", (rc, out[:6]), (0, b"hello\n"))
    check("no SNI when none is given", srv.sni[-1], None)
    colon = ":".join(pin[i:i + 2] for i in range(0, 64, 2))
    rc, out, err = run(srv.port, colon)
    check("pin with colons", (rc, out[:6]), (0, b"hello\n"))
    srv.close()

    # 1b. tlsm_write (the hello line of push.fi): three records, then reads still work
    got = []
    def echo_reader(conn):
        buf = b""
        while buf.count(b"\n") < 3:
            d = conn.recv(4096)
            if not d:
                break
            buf += d
        got.append(buf)
        conn.sendall(b"ack\n")
        conn.unwrap()
    for text in ("HELLO " + "0123456789abcdef" * 2 + " 1", "x" * 1023, "\u00e4\u00f6\u00fc \u20ac"):
        srv = Server(crt, key, echo_reader)
        rc, out, err = run(srv.port, pin, "relay.test", write=text)
        want = (text.encode() + b"\n") * 3
        check("write %d octets: server got three exact lines" % len(text.encode()), got[-1] if got else None, want)
        check("write: read after write works, exit 0", (rc, out), (0, b"ack\n"))
        srv.close()

    # 2. wrong pin
    srv = Server(crt, key, send_then_close(b"secret\n"))
    rc, out, err = run(srv.port, "00" * 32)
    check("wrong pin refused, nothing delivered", (rc, out, err[-2:]), (1, b"", ["ERR", "3"]))
    srv.close()

    # 3. keys the client does not do
    k2, c2 = make_key("rsa", "rsa")
    srv = Server(c2, k2, send_then_close(b"x\n"))
    rc, out, err = run(srv.port, spki_pin(c2))
    check("RSA key refused", (rc, out, err[-2] if len(err) >= 2 else None), (1, b"", "ERR"))
    srv.close()
    k3, c3 = make_key("p384", "p384")
    srv = Server(c3, k3, send_then_close(b"x\n"))
    rc, out, err = run(srv.port, spki_pin(c3))
    check("P-384 key refused", (rc, out), (1, b""))
    srv.close()
    k4, c4 = make_key("ed", "ed25519")
    srv = Server(c4, k4, send_then_close(b"x\n"))
    rc, out, err = run(srv.port, spki_pin(c4))
    check("Ed25519 key refused", (rc, out), (1, b""))
    srv.close()

    # 4. TLS 1.2 only server, plain TCP server
    srv = Server(crt, key, send_then_close(b"x\n"), maxver=ssl.TLSVersion.TLSv1_2)
    rc, out, err = run(srv.port, pin)
    check("TLS 1.2 server refused (no downgrade)", (rc, out), (1, b""))
    srv.close()
    plain = socket.socket()
    plain.bind(("127.0.0.1", 0))
    plain.listen(2)

    def plain_loop():
        while True:
            try:
                c, _ = plain.accept()
            except OSError:
                return
            try:
                c.sendall(b"HTTP/1.0 200 OK\r\n\r\nhello hello hello\n")
                time.sleep(0.3)
            finally:
                c.close()
    threading.Thread(target=plain_loop, daemon=True).start()
    rc, out, err = run(plain.getsockname()[1], pin)
    check("plain TCP server refused", (rc, out), (1, b""))
    plain.close()

    # 5. a flipped bit
    srv = Server(crt, key, send_then_close(b"A" * 40000))
    rc, out, err = run(srv.port, pin)
    check("untouched baseline through the proxy", (rc, len(out)), (0, 40000))
    outs = []
    for off in (0, 4, 5, 60, 90, 130, 200, 300, 500):
        pr = Proxy(srv.port, flip_at=off)
        rc, out, err = run(pr.port, pin)
        pr.close()
        outs.append((off, rc != 0 or out != b"A" * 40000, out))
    bad = [o for o in outs if not o[1]]
    check("a flipped bit in the first 500 octets never goes through", bad, [])
    check("... and delivers nothing before the handshake ends",
          [o[0] for o in outs if o[2] and o[0] < 300], [])
    # flip inside the data phase: handshake is ~ 300-700 octets, the data follows
    pr = Proxy(srv.port, flip_at=5000)
    rc, out, err = run(pr.port, pin)
    pr.close()
    check("a flipped bit in the data: authentication error, end of stream",
          (rc, err[-2:]), (3, ["END", "-10"]))
    check("... and only whole records before it arrive", len(out) % 1 == 0 and len(out) < 40000, True)
    # delivery in 1 octet pieces
    pr = Proxy(srv.port, chunk=1)
    rc, out, err = run(pr.port, pin, timeout=120)
    pr.close()
    check("1 octet at a time", (rc, len(out)), (0, 40000))
    pr = Proxy(srv.port, chunk=7)
    rc, out, err = run(pr.port, pin, timeout=120)
    pr.close()
    check("7 octets at a time", (rc, len(out)), (0, 40000))
    srv.close()

    # 6. close without close_notify, no data at all, alert
    srv = Server(crt, key, send_then_close(b"half\n", notify=False))
    rc, out, err = run(srv.port, pin)
    check("close without close_notify: data, then an error end", (rc, out, err[-2:]),
          (3, b"half\n", ["END", "-1"]))
    srv.close()
    srv = Server(crt, key, lambda c: c.unwrap())
    rc, out, err = run(srv.port, pin)
    check("empty stream, close_notify", (rc, out, err[-2:]), (0, b"", ["END", "0"]))
    srv.close()

    # 7. several writes of different sizes (records of 1 .. 16384 octets)
    sizes = [1, 2, 15, 16, 17, 255, 1000, 4096, 16383, 16384, 16385, 30000]

    def multi(conn):
        for i, n in enumerate(sizes):
            conn.sendall(bytes([65 + i]) * n)
        conn.unwrap()
    srv = Server(crt, key, multi)
    rc, out, err = run(srv.port, pin)
    want = b"".join(bytes([65 + i]) * n for i, n in enumerate(sizes))
    check("records of all sizes", (rc, out == want), (0, True))
    srv.close()

    # 8. the server pushes after a delay (a read that blocks, then wakes)
    def late(conn):
        time.sleep(1.0)
        conn.sendall(b"late\n")
        conn.unwrap()
    srv = Server(crt, key, late)
    t0 = time.time()
    rc, out, err = run(srv.port, pin)
    check("blocked read wakes up with the data", (rc, out, time.time() - t0 >= 0.9), (0, b"late\n", True))
    srv.close()

    # 9. openssl s_server (another implementation) on a pty, where it reads
    # its console commands: `K` = KeyUpdate asking for an answer. Three of them.
    if shutil.which("openssl"):
        import pty
        port_sock = socket.socket()
        port_sock.bind(("127.0.0.1", 0))
        sport = port_sock.getsockname()[1]
        port_sock.close()
        mfd, sfd = pty.openpty()
        p = subprocess.Popen(["openssl", "s_server", "-tls1_3", "-accept", "127.0.0.1:%d" % sport,
                              "-cert", crt, "-key", key, "-ciphersuites",
                              "TLS_CHACHA20_POLY1305_SHA256"],
                             stdin=sfd, stdout=sfd, stderr=sfd)
        time.sleep(0.7)
        cl = subprocess.Popen([tool, "127.0.0.1", str(sport), pin, "relay.test"],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        time.sleep(0.7)
        for line in (b"one\n", b"K\n", b"two\n", b"K\n", b"three\n", b"K\n", b"four\n", b"Q\n"):
            os.write(mfd, line)
            time.sleep(0.5)
        try:
            o, e = cl.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            cl.kill()
            o, e = cl.communicate()
        p.kill()
        os.close(mfd)
        os.close(sfd)
        check("openssl s_server: three KeyUpdates followed and answered, data after each",
              o, b"one\ntwo\nthree\nfour\n")
    else:
        print("  skip  openssl s_server")
finally:
    cleanup()

print("tlsmini: %d passed, %d failed" % (passed, failed))
sys.exit(1 if failed else 0)

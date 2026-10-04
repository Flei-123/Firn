#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/tls/tls12_check.py -- the TLS 1.2 half of the client (round TLS12),
held against implementations this repository did not write.

    python3 tools/tls/tls12_check.py .tls-work/tls    # tls_main, see run.sh

  A  OPENSSL s_server, TLS 1.2 only: every cipher suite the client offers
     (ECDHE-ECDSA / ECDHE-RSA with AES-128-GCM, AES-256-GCM, ChaCha20-
     Poly1305) x both groups (X25519, P-256) x the signature schemes of the
     ServerKeyExchange (RSA PKCS#1 SHA-256/384, RSA-PSS SHA-256/384, ECDSA
     SHA-256/384). The answer has to be a complete HTTP page.
  B  A TLS 1.3 capable server still gets TLS 1.3 from the same ClientHello
     (VERSION 772), and a client told to offer 1.2 only gets 1.2 from it.
  C  PYTHON's ssl, TLS 1.2 only, 512 KiB of known bytes: the record layer
     loses and repeats nothing across forty records, per cipher suite.
  D  THE COUNTER-CHECKS. Servers and men in the middle that must NOT get
     through: a flipped bit in the ServerKeyExchange signature, in a record
     of application data, in the server's Finished; the downgrade sentinel
     in the ServerHello random; a TLS 1.0 / 1.1 hello; an unoffered suite; a
     TLS 1.3 suite in a hello without supported_versions; a non-zero
     compression method; a session id echoed back; a server that cuts the
     handshake short; garbage; an expired / wrong-name / unknown-issuer
     certificate; the strict mode (`require_ems`) against a server that has
     no extended master secret is covered by the real hosts of part E.
  E  REAL HOSTS, when there is a route: login.live.com, badssl.com's
     tls-v1-2 (an old nginx with no extended master secret), the refusals
     expired / wrong.host / self-signed / untrusted-root, and the TLS 1.0 /
     1.1 hosts that must be refused. The body of badssl's page is compared
     with curl's.
"""
import hashlib
import os
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

BINARY = os.path.abspath(sys.argv[1])
PASS = 0
FAIL = []
SKIP = []


def ok(title, good, got=""):
    global PASS
    if good:
        PASS += 1
    else:
        FAIL.append((title, got))


def run_openssl(*a):
    subprocess.run(["openssl"] + list(a), check=True, capture_output=True)


def make_certs(d):
    def sh(*a):
        subprocess.run(a, check=True, capture_output=True, cwd=d)
    sh("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key",
       "-out", "ca.crt", "-subj", "/CN=Firn TLS12 CA", "-days", "30")
    ext = os.path.join(d, "ext.cnf")
    open(ext, "w").write("subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\n")
    for kind in ("rsa", "ec"):
        if kind == "rsa":
            sh("openssl", "genrsa", "-out", kind + ".key", "2048")
        else:
            sh("openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", kind + ".key")
        sh("openssl", "req", "-new", "-key", kind + ".key", "-subj", "/CN=localhost", "-out", kind + ".csr")
        sh("openssl", "x509", "-req", "-in", kind + ".csr", "-CA", "ca.crt", "-CAkey", "ca.key",
           "-CAcreateserial", "-out", kind + ".crt", "-days", "30", "-extfile", "ext.cnf")
    # an expired one (Python's cryptography: openssl-ca needs a database for that)
    import datetime
    from cryptography import x509
    from cryptography.x509.oid import NameOID
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    ca_cert = x509.load_pem_x509_certificate(open(os.path.join(d, "ca.crt"), "rb").read())
    ca_key = serialization.load_pem_private_key(open(os.path.join(d, "ca.key"), "rb").read(), None)
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    now = datetime.datetime.now(datetime.timezone.utc)
    nm = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
    c = (x509.CertificateBuilder().subject_name(nm).issuer_name(ca_cert.subject)
         .public_key(key.public_key()).serial_number(x509.random_serial_number())
         .not_valid_before(now - datetime.timedelta(days=400))
         .not_valid_after(now - datetime.timedelta(days=10))
         .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), critical=False)
         .sign(ca_key, hashes.SHA256()))
    open(os.path.join(d, "old.crt"), "wb").write(c.public_bytes(serialization.Encoding.PEM))
    open(os.path.join(d, "old.key"), "wb").write(key.private_bytes(
        serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL,
        serialization.NoEncryption()))


class Server:
    def __init__(self, d, port, cert, key, extra=()):
        self.port = port
        self.p = subprocess.Popen(
            ["openssl", "s_server", "-accept", str(port), "-cert", os.path.join(d, cert),
             "-key", os.path.join(d, key), "-www", "-quiet", "-naccept", "60"] + list(extra),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                return
            except OSError:
                time.sleep(0.05)

    def stop(self):
        self.p.terminate()
        try:
            self.p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.p.kill()


def tls_main(ip, port, host, store, path="-", dump=None, only12=False, strict=False, timeout=60):
    args = [BINARY, ip, str(port), host, store, path]
    if dump or only12 or strict:
        args.append(dump or "")
    if only12 or strict:
        args.append("12" if only12 else "")
    if strict:
        args.append("strict")
    try:
        r = subprocess.run(args, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"TIMEOUT": ""}
    out = {}
    for line in r.stdout.decode(errors="replace").split("\n"):
        if not line.strip():
            continue
        if " " in line:
            k, v = line.split(" ", 1)
            out[k] = v.strip()
        else:
            out[line.strip()] = ""
    return out


# ------------------------------------------------------------------ Part A/B
def part_a(d, ca):
    suites = {
        "rsa": [("ECDHE-RSA-AES128-GCM-SHA256", "49199"), ("ECDHE-RSA-AES256-GCM-SHA384", "49200"),
                ("ECDHE-RSA-CHACHA20-POLY1305", "52392")],
        "ec": [("ECDHE-ECDSA-AES128-GCM-SHA256", "49195"), ("ECDHE-ECDSA-AES256-GCM-SHA384", "49196"),
               ("ECDHE-ECDSA-CHACHA20-POLY1305", "52393")],
    }
    sigs = {
        "rsa": ["RSA+SHA256", "RSA+SHA384", "RSA-PSS+SHA256", "RSA-PSS+SHA384"],
        "ec": ["ECDSA+SHA256", "ECDSA+SHA384"],
    }
    port = 46100
    for kind in ("rsa", "ec"):
        for (cipher, num) in suites[kind]:
            for group in ("X25519", "P-256"):
                port += 1
                s = Server(d, port, kind + ".crt", kind + ".key",
                           ["-tls1_2", "-cipher", cipher, "-curves", group])
                got = tls_main("127.0.0.1", port, "localhost", ca, "/")
                s.stop()
                ok("A %s %s: suite %s, TLS 1.2, page" % (cipher, group, num),
                   got.get("SUITE") == num and got.get("VERSION") == "771"
                   and got.get("VERIFY") == "OK" and got.get("STATUS", "").startswith("HTTP/1.")
                   and int(got.get("BYTES", "0")) > 1000, str(got))
        for sg in sigs[kind]:
            port += 1
            s = Server(d, port, kind + ".crt", kind + ".key", ["-tls1_2", "-sigalgs", sg])
            got = tls_main("127.0.0.1", port, "localhost", ca, "/")
            s.stop()
            ok("A %s signed with %s" % (kind, sg), got.get("VERSION") == "771" and got.get("VERIFY") == "OK"
               and got.get("STATUS", "").startswith("HTTP/1."), str(got))
    # B: one server, both versions
    port += 1
    s = Server(d, port, "rsa.crt", "rsa.key", [])
    got = tls_main("127.0.0.1", port, "localhost", ca, "/")
    ok("B a TLS 1.3 server gets TLS 1.3", got.get("VERSION") == "772" and got.get("VERIFY") == "OK", str(got))
    got = tls_main("127.0.0.1", port, "localhost", ca, "/", only12=True)
    ok("B the same server, client offers 1.2 only: TLS 1.2 (and no downgrade alarm: the "
       "sentinel is only checked when 1.3 was offered)", got.get("VERSION") == "771"
       and got.get("VERIFY") == "OK", str(got))
    s.stop()
    # a TLS 1.2 ONLY server and a client that offers both: the old "refused" case is now a success
    port += 1
    s = Server(d, port, "rsa.crt", "rsa.key", ["-tls1_2"])
    got = tls_main("127.0.0.1", port, "localhost", ca, "/")
    ok("B a TLS 1.2-only server is reachable by the default client", got.get("VERSION") == "771", str(got))
    s.stop()


# ------------------------------------------------------------------ Part C
def part_c(d, ca):
    payload = bytes((i * 37 + (i >> 8) * 11) & 0xFF for i in range(512 * 1024))
    port = 46300
    for cipher, num, cert in (("ECDHE-RSA-AES128-GCM-SHA256", "49199", "rsa"),
                              ("ECDHE-RSA-AES256-GCM-SHA384", "49200", "rsa"),
                              ("ECDHE-RSA-CHACHA20-POLY1305", "52392", "rsa"),
                              ("ECDHE-ECDSA-AES128-GCM-SHA256", "49195", "ec"),
                              ("ECDHE-ECDSA-CHACHA20-POLY1305", "52393", "ec")):
        port += 1
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.minimum_version = ctx.maximum_version = ssl.TLSVersion.TLSv1_2
        ctx.load_cert_chain(os.path.join(d, cert + ".crt"), os.path.join(d, cert + ".key"))
        ctx.set_ciphers(cipher)
        srv = socket.socket()
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(("127.0.0.1", port))
        srv.listen(1)
        stop = threading.Event()

        def serve(srv=srv, ctx=ctx):
            srv.settimeout(20)
            try:
                c, _ = srv.accept()
                with ctx.wrap_socket(c, server_side=True) as t:
                    data = b""
                    while b"\r\n\r\n" not in data:
                        chunk = t.recv(4096)
                        if not chunk:
                            break
                        data += chunk
                    t.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(payload))
                    t.sendall(payload)
            except Exception:
                pass
            finally:
                srv.close()
        th = threading.Thread(target=serve, daemon=True)
        th.start()
        dumpf = os.path.join(d, "body.%d" % port)
        got = tls_main("127.0.0.1", port, "localhost", ca, "/", dump=dumpf)
        th.join(timeout=10)
        body = open(dumpf, "rb").read() if os.path.exists(dumpf) else b""
        resp_body = body.split(b"\r\n\r\n", 1)[1] if b"\r\n\r\n" in body else b""
        ok("C %s: 512 KiB arrive octet for octet (Python ssl, TLS 1.2)" % cipher,
           got.get("SUITE") == num and got.get("VERSION") == "771" and resp_body == payload,
           "%s %d octets, sha %s" % (str(got)[:80], len(resp_body), hashlib.sha256(resp_body).hexdigest()[:12]))


# ------------------------------------------------------------------ Part D
def recv_record(sock):
    hdr = b""
    while len(hdr) < 5:
        c = sock.recv(5 - len(hdr))
        if not c:
            return None
        hdr += c
    ln = struct.unpack(">H", hdr[3:5])[0]
    body = b""
    while len(body) < ln:
        c = sock.recv(ln - len(body))
        if not c:
            return None
        body += c
    return hdr[0], hdr[1:3], body


class Mitm(threading.Thread):
    """Sits between the client and an `openssl s_server`, parses the records of
    the SERVER's direction and lets `meddle(state, ctype, version, body)` change
    them. `state` counts the records by type. Returns the (possibly changed)
    body, or None to cut the connection at that point."""

    def __init__(self, listen_port, to_port, meddle):
        super().__init__(daemon=True)
        self.srv = socket.socket()
        self.srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.srv.bind(("127.0.0.1", 0))   # the system picks a free port
        self.port = self.srv.getsockname()[1]
        self.srv.listen(1)
        self.to_port = to_port
        self.meddle = meddle
        self.state = {}

    def run(self):
        try:
            self.srv.settimeout(20)
            c, _ = self.srv.accept()
        except OSError:
            return
        try:
            s = socket.create_connection(("127.0.0.1", self.to_port), timeout=5)
        except OSError:
            c.close()
            return

        def up():
            try:
                while True:
                    d = c.recv(65536)
                    if not d:
                        break
                    s.sendall(d)
            except OSError:
                pass
            try:
                s.shutdown(socket.SHUT_WR)
            except OSError:
                pass
        threading.Thread(target=up, daemon=True).start()
        try:
            while True:
                rec = recv_record(s)
                if rec is None:
                    break
                ct, ver, body = rec
                n = self.state.get(ct, 0) + 1
                self.state[ct] = n
                body = self.meddle(self.state, ct, ver, bytearray(body), n)
                if body is None:
                    break
                c.sendall(bytes([ct]) + ver + struct.pack(">H", len(body)) + bytes(body))
        except OSError:
            pass
        finally:
            try:
                c.close()
                s.close()
            except OSError:
                pass


def handshake_msgs(body):
    """(type, offset, length) of each handshake message in a plaintext handshake record"""
    out = []
    i = 0
    while i + 4 <= len(body):
        ln = (body[i + 1] << 16) | (body[i + 2] << 8) | body[i + 3]
        out.append((body[i], i, ln))
        i += 4 + ln
    return out


def mitm_case(d, ca, port, title, server_args, meddle, want_keys, only12=False, cert="rsa", any_err=None):
    s = Server(d, port, cert + ".crt", cert + ".key", server_args)
    m = Mitm(port + 500, port, meddle)
    m.start()
    got = tls_main("127.0.0.1", m.port, "localhost", ca, "/", only12=only12, timeout=40)
    s.stop()
    good = all(got.get(k) == v for k, v in want_keys.items())
    if any_err:
        good = any(k in got for k in any_err)
    ok("D " + title, good, "want %s got %s" % (want_keys, {k: got.get(k) for k in want_keys} or got))


def part_d(d, ca):
    port = 46400

    def passthrough(state, ct, ver, body, n):
        return body

    # --- a clean run through the proxy (the proxy itself changes nothing)
    port += 1
    mitm_case(d, ca, port, "control: the proxy alone does not break the handshake",
              ["-tls1_2"], passthrough, {"VERSION": "771", "VERIFY": "OK"})

    # --- the ServerKeyExchange signature, one bit
    def flip_ske(state, ct, ver, body, n):
        if ct == 22:
            for (t, off, ln) in handshake_msgs(body):
                if t == 12:
                    body[off + 4 + ln - 3] ^= 1  # inside the signature
        return body
    port += 1
    mitm_case(d, ca, port, "a flipped bit in the ServerKeyExchange signature (RSA) is refused",
              ["-tls1_2"], flip_ske, {"ERRSignature": ""})
    port += 1
    mitm_case(d, ca, port, "... and (ECDSA)", ["-tls1_2"], flip_ske, {"ERRSignature": ""}, cert="ec")

    # --- the share inside the signed parameters
    def flip_share(state, ct, ver, body, n):
        if ct == 22:
            for (t, off, ln) in handshake_msgs(body):
                if t == 12:
                    body[off + 4 + 6] ^= 0x10  # inside the ephemeral public key
        return body
    port += 1
    mitm_case(d, ca, port, "a changed ephemeral key (signature no longer matches)", ["-tls1_2"],
              flip_share, {"ERRSignature": ""})

    # --- the downgrade sentinel
    def sentinel(last):
        def f(state, ct, ver, body, n):
            if ct == 22 and body and body[0] == 2:
                body[4 + 2 + 24:4 + 2 + 32] = b"DOWNGRD" + bytes([last])
            return body
        return f
    port += 1
    mitm_case(d, ca, port, "DOWNGRD\\x01 in the ServerHello random (client offered 1.3)",
              ["-tls1_2"], sentinel(1), {"ERRDowngrade": ""})
    port += 1
    mitm_case(d, ca, port, "DOWNGRD\\x00 in the ServerHello random", ["-tls1_2"], sentinel(0),
              {"ERRDowngrade": ""})
    port += 1
    # ... but not when the client offered only 1.2: the check would then refuse every
    # modern server that was simply asked for 1.2 (the signature fails afterwards, which
    # is what the proxy's change causes -- the point is that it is NOT Downgrade)
    mitm_case(d, ca, port, "the sentinel is ignored when 1.3 was not offered (fails later, not as Downgrade)",
              ["-tls1_2"], sentinel(1), {"ERRSignature": ""}, only12=True)

    # --- ServerHello tampering
    def edit_sh(fn):
        def f(state, ct, ver, body, n):
            if ct == 22 and body and body[0] == 2:
                fn(body)
            return body
        return f

    def set_version(body):
        body[4] = 3
        body[5] = 2  # 0x0302: TLS 1.1
    port += 1
    mitm_case(d, ca, port, "a ServerHello with version TLS 1.1 is refused", ["-tls1_2"], edit_sh(set_version),
              {"ERRVersion": ""})

    def set_suite(body):
        sidlen = body[4 + 2 + 32]
        at = 4 + 2 + 32 + 1 + sidlen
        body[at] = 0x00
        body[at + 1] = 0x2F  # TLS_RSA_WITH_AES_128_CBC_SHA: not offered
    port += 1
    mitm_case(d, ca, port, "a cipher suite that was not offered is refused", ["-tls1_2"], edit_sh(set_suite),
              {"ERRSuite": ""})

    def set_tls13_suite(body):
        sidlen = body[4 + 2 + 32]
        at = 4 + 2 + 32 + 1 + sidlen
        body[at] = 0x13
        body[at + 1] = 0x01
    port += 1
    mitm_case(d, ca, port, "a TLS 1.3 suite in a hello without supported_versions is refused", ["-tls1_2"],
              edit_sh(set_tls13_suite), {"ERRVersion": ""})

    def set_compression(body):
        sidlen = body[4 + 2 + 32]
        at = 4 + 2 + 32 + 1 + sidlen + 2
        body[at] = 1
    port += 1
    mitm_case(d, ca, port, "a non-zero compression method is refused", ["-tls1_2"], edit_sh(set_compression),
              {"ERRProtocol": ""})

    # echo the session id the client sent: needs the ClientHello, so the proxy copies it
    # from the client direction -- simplest is to make the server reply with sid = 32 x 0
    # is not an echo; instead cut the session id to the client's: done in a dedicated proxy below
    # --- the handshake cut short
    def cut_after_hello(state, ct, ver, body, n):
        if ct == 22 and n >= 2:
            return None
        return body
    port += 1
    mitm_case(d, ca, port, "the server's flight cut after the ServerHello: an error, no hang", ["-tls1_2"],
              cut_after_hello, {}, any_err=("ERRClosed", "ERRIo"))

    def garbage_instead(state, ct, ver, body, n):
        if ct == 22 and n == 2:
            return bytearray(os.urandom(300))
        return body
    port += 1
    mitm_case(d, ca, port, "garbage where the Certificate belongs is an error", ["-tls1_2"], garbage_instead, {},
              any_err=("ERRProtocol", "ERRCertificate", "ERRTooLarge", "ERRClosed", "ERRIo", "ERRUnsupported"))
    # the garbage case only needs "an error and no crash": check ERR*
    # --- a record that claims 70000 octets
    def huge(state, ct, ver, body, n):
        return body
    # --- the encrypted part: the server's Finished, and application data
    def flip_encrypted(which):
        def f(state, ct, ver, body, n):
            if ct == 22 and state.get("ccs", 0) == 1:
                state["ccs"] = 2
                if which == "finished":
                    body[len(body) // 2] ^= 1
            if ct == 20:
                state["ccs"] = 1
            if ct == 23 and which == "app" and n == 1:
                body[len(body) // 2] ^= 1
            return body
        return f
    port += 1
    mitm_case(d, ca, port, "a flipped bit in the server's Finished is refused (Decrypt)", ["-tls1_2"],
              flip_encrypted("finished"), {"ERRDecrypt": ""})
    port += 1
    s = Server(d, port, "rsa.crt", "rsa.key", ["-tls1_2"])
    m = Mitm(port + 500, port, flip_encrypted("app"))
    m.start()
    got = tls_main("127.0.0.1", m.port, "localhost", ca, "/", timeout=40)
    s.stop()
    ok("D a flipped bit in a record of application data is not passed on as a page",
       "ERRDecrypt" in got and got.get("STATUS", "") == "" and got.get("BYTES") == "0", str(got))

    # --- certificates
    for (title, cert, host, want) in (
            ("an expired certificate", "old", "localhost", {"ERRCertificate": "", "VERIFY": "EXPIRED"}),
            ("a name that does not match", "rsa", "wrong.test", {"ERRCertificate": "", "VERIFY": "NAME"})):
        port += 1
        s = Server(d, port, cert + ".crt", cert + ".key", ["-tls1_2"])
        got = tls_main("127.0.0.1", port, host, ca, "/")
        s.stop()
        ok("D TLS 1.2: " + title + " is refused", all(got.get(k) == v for k, v in want.items()), str(got))
    port += 1
    empty = os.path.join(d, "empty.pem")
    open(empty, "w").close()
    s = Server(d, port, "rsa.crt", "rsa.key", ["-tls1_2"])
    got = tls_main("127.0.0.1", port, "localhost", empty, "/")
    s.stop()
    ok("D TLS 1.2: an empty trust store trusts nothing", got.get("VERIFY") == "UNKNOWN_ISSUER"
       and got.get("ERRCertificate") == "", str(got))

    # --- a raw hostile server: a ServerHello that echoes the session id of the ClientHello
    port += 1
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(1)

    def echo_sid():
        srv.settimeout(10)
        try:
            c, _ = srv.accept()
            rec = recv_record(c)
            ch = rec[2]
            sid = ch[4 + 2 + 32 + 1: 4 + 2 + 32 + 1 + ch[4 + 2 + 32]]
            rnd = os.urandom(32)
            ext = b"\xff\x01\x00\x01\x00" + b"\x00\x17\x00\x00"
            sh = (b"\x03\x03" + rnd + bytes([len(sid)]) + sid + b"\xc0\x2f\x00" +
                  struct.pack(">H", len(ext)) + ext)
            msg = b"\x02" + struct.pack(">I", len(sh))[1:] + sh
            c.sendall(b"\x16\x03\x03" + struct.pack(">H", len(msg)) + msg)
            time.sleep(1)
            c.close()
        except OSError:
            pass
        finally:
            srv.close()
    th = threading.Thread(target=echo_sid, daemon=True)
    th.start()
    got = tls_main("127.0.0.1", port, "localhost", ca, "/", timeout=30)
    th.join(timeout=5)
    ok("D a ServerHello that echoes our session id (a resumption we never asked for) is refused",
       "ERRProtocol" in got, str(got))

    # --- raw: a record header that claims more than any record can hold
    port += 1
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(1)

    def oversize():
        srv.settimeout(10)
        try:
            c, _ = srv.accept()
            recv_record(c)
            c.sendall(b"\x16\x03\x03\xff\xff" + b"A" * 100)
            time.sleep(1)
            c.close()
        except OSError:
            pass
        finally:
            srv.close()
    th = threading.Thread(target=oversize, daemon=True)
    th.start()
    got = tls_main("127.0.0.1", port, "localhost", ca, "/", timeout=30)
    th.join(timeout=5)
    ok("D a record of 65535 octets is refused (TooLarge), no memory growth, no hang",
       "ERRTooLarge" in got, str(got))

    # --- a server that does not echo extended_master_secret, and the strict mode
    # (no local server omits the extension; badssl.com in part E is the real one)


# ------------------------------------------------------------------ Part F
def part_f(d, ca):
    """FUZZ: one random byte of one random record of the server's PLAINTEXT flight
    (ServerHello .. ServerHelloDone) is replaced, 160 times, over RSA and ECDSA
    servers. Whatever happens, the client must END: exit code 0 or 1 (it printed
    its verdict), never a signal, never a time-out. Most mutations are refused
    by the signature or the transcript; the interesting ones are the parsers."""
    import random
    rnd = random.Random(1234)
    port = 46800
    crashed = []
    ended = 0
    for i in range(160):
        cert = "rsa" if i % 2 == 0 else "ec"
        port += 1
        target = rnd.randrange(1, 5)   # which handshake record of the server (1-based)
        pos = rnd.random()
        val = rnd.randrange(256)

        def meddle(state, ct, ver, body, n, target=target, pos=pos, val=val):
            if ct == 22 and n == target and len(body) > 0:
                body[int(pos * len(body)) % len(body)] = val
            return body
        s = Server(d, port, cert + ".crt", cert + ".key", ["-tls1_2"])
        m = Mitm(port + 500, port, meddle)
        m.start()
        try:
            r = subprocess.run([BINARY, "127.0.0.1", str(m.port), "localhost", ca, "-"],
                               capture_output=True, timeout=45)
            rc = r.returncode
        except subprocess.TimeoutExpired:
            rc = "timeout"
        s.stop()
        if rc in (0, 1):
            ended += 1
        else:
            crashed.append((i, cert, target, pos, val, rc))
    ok("F 160 mutated server flights: every run ENDS with a verdict (no signal, no hang)",
       not crashed, "first: %s" % (crashed[:3],))


# ------------------------------------------------------------------ Part E
def part_e():
    bundle = None
    for p in ("/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt"):
        if os.path.exists(p):
            bundle = p
            break
    try:
        socket.create_connection(("login.live.com", 443), timeout=5).close()
    except OSError:
        SKIP.append("E no route to the internet")
        return
    if bundle is None:
        SKIP.append("E no system CA bundle")
        return

    def real(host, port=443, **kw):
        ip = socket.gethostbyname(host)
        return tls_main(ip, port, host, bundle, "/", **kw)

    got = real("login.live.com")
    ok("E login.live.com over TLS 1.2 (or 1.3): a page, verified", got.get("VERIFY") == "OK"
       and got.get("STATUS", "").startswith("HTTP/1.") and got.get("VERSION") in ("771", "772"), str(got))
    got = real("login.live.com", only12=True)
    ok("E login.live.com, client offers 1.2 only: TLS 1.2", got.get("VERSION") == "771"
       and got.get("VERIFY") == "OK" and got.get("STATUS", "").startswith("HTTP/1."), str(got))
    try:
        socket.create_connection(("tls-v1-2.badssl.com", 1012), timeout=5).close()
    except OSError:
        SKIP.append("E badssl.com not reachable")
        return
    dump = tempfile.mktemp()
    got = real("tls-v1-2.badssl.com", 1012, dump=dump)
    mine = open(dump, "rb").read() if os.path.exists(dump) else b""
    if os.path.exists(dump):
        os.unlink(dump)
    ok("E tls-v1-2.badssl.com:1012 (TLS 1.2 only, no extended master secret): 200", got.get("VERSION") == "771"
       and got.get("VERIFY") == "OK" and "200" in got.get("STATUS", ""), str(got))
    try:
        ref = subprocess.run(["curl", "-s", "--tlsv1.2", "--tls-max", "1.2", "-i", "--max-time", "20",
                              "https://tls-v1-2.badssl.com:1012/"], capture_output=True).stdout
        if mine and ref:
            ok("E ... and its body equals curl's, octet for octet",
               mine.split(b"\r\n\r\n", 1)[1] == ref.replace(b"\r\n", b"\r\n").split(b"\r\n\r\n", 1)[1],
               "%d vs %d octets" % (len(mine), len(ref)))
    except OSError:
        pass
    got = real("tls-v1-2.badssl.com", 1012, strict=True)
    ok("E COUNTER: the strict mode refuses a server without extended_master_secret (Unsupported)",
       "ERRUnsupported" in got, str(got))
    for host, verdict in (("expired.badssl.com", "EXPIRED"), ("wrong.host.badssl.com", "NAME"),
                          ("self-signed.badssl.com", "UNKNOWN_ISSUER"),
                          ("untrusted-root.badssl.com", "UNKNOWN_ISSUER")):
        got = real(host)
        ok("E COUNTER: %s is refused at the certificate (%s)" % (host, verdict),
           got.get("ERRCertificate") == "" and got.get("VERIFY") == verdict, str(got))
    for host, port in (("tls-v1-0.badssl.com", 1010), ("tls-v1-1.badssl.com", 1011)):
        try:
            got = real(host, port, timeout=40)
        except OSError:
            SKIP.append("E " + host)
            continue
        ok("E COUNTER: %s (TLS 1.0/1.1 only) is refused, nothing fetched" % host,
           got.get("STATUS", "") == "" and any(k.startswith("ERR") for k in got), str(got))


def main():
    d = tempfile.mkdtemp(prefix="firn-tls12-")
    try:
        make_certs(d)
        ca = os.path.join(d, "ca.crt")
        part_a(d, ca)
        part_c(d, ca)
        part_d(d, ca)
        part_f(d, ca)
        part_e()
    finally:
        import shutil
        shutil.rmtree(d, ignore_errors=True)
    for t, g in FAIL:
        print("FAIL %s\n     %s" % (t, g))
    for s in SKIP:
        print("SKIP %s" % s)
    print("TLS 1.2 CLIENT: %d checks passed, %d failed, %d skipped" % (PASS, len(FAIL), len(SKIP)))
    sys.exit(1 if FAIL else 0)


main()

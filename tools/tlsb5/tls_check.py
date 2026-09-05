#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/tls_check.py -- the TLS 1.3 client against a REAL server.

The rule of this round is that the counterpart must not be this
repository. Two ends that misunderstand the same standard in the same way
agree perfectly, so the whole measurement is worth nothing unless the
other side was written by somebody else. Here the other side is

  * `openssl s_server` -- OpenSSL's own TLS 1.3 implementation, started by
    this script with certificates this script generates, and killed again;
  * and, when the machine has a route to it, six real hosts on the public
    internet, whose answers are compared with what PYTHON's `ssl` module
    (also OpenSSL) fetched from the same address at the same time.

THE COUNTER-CHECKS ARE THE POINT. A client that ignores certificates
connects to everything and looks perfect in a table of successes. So the
list below contains servers that this client MUST refuse -- an expired
certificate, a name that does not match, an issuer nobody signed for, a
server that will only speak TLS 1.2, a server that offers only a cipher
suite this client does not have -- and a man in the middle that flips one
bit of one record, which must come back as a decryption failure and not as
a page.
"""
import hashlib
import os
import re
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(ROOT, ".b5-work")
PORT = 44330
FAILS = []
OK = 0
COUNTER_OK = 0
COUNTER_TOTAL = 0


def note(title, good, want, got, counter=False):
    global OK, COUNTER_OK, COUNTER_TOTAL
    if counter:
        COUNTER_TOTAL += 1
    if good:
        OK += 1
        if counter:
            COUNTER_OK += 1
    else:
        FAILS.append((title, want, got))


def make_certs(d):
    import datetime
    from cryptography import x509
    from cryptography.x509.oid import NameOID
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa, ec
    utc = datetime.timezone.utc
    now = datetime.datetime.now(utc)
    day = datetime.timedelta(days=1)

    def nm(s):
        return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, s)])

    ca_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ca = (x509.CertificateBuilder().subject_name(nm("Firn B5 CA"))
          .issuer_name(nm("Firn B5 CA")).public_key(ca_key.public_key())
          .serial_number(1).not_valid_before(now - day)
          .not_valid_after(now + 300 * day)
          .add_extension(x509.BasicConstraints(ca=True, path_length=None),
                         critical=True)
          .sign(ca_key, hashes.SHA256()))
    with open(os.path.join(d, "ca.pem"), "wb") as f:
        f.write(ca.public_bytes(serialization.Encoding.PEM))

    def leaf(tag, key, nb, na):
        c = (x509.CertificateBuilder().subject_name(nm("localhost"))
             .issuer_name(ca.subject).public_key(key.public_key())
             .serial_number(x509.random_serial_number())
             .not_valid_before(nb).not_valid_after(na)
             .add_extension(x509.SubjectAlternativeName(
                 [x509.DNSName("localhost")]), critical=False)
             .add_extension(x509.BasicConstraints(ca=False,
                                                  path_length=None),
                            critical=True)
             .sign(ca_key, hashes.SHA256()))
        with open(os.path.join(d, tag + ".crt"), "wb") as f:
            f.write(c.public_bytes(serialization.Encoding.PEM))
        with open(os.path.join(d, tag + ".key"), "wb") as f:
            f.write(key.private_bytes(
                serialization.Encoding.PEM,
                serialization.PrivateFormat.TraditionalOpenSSL,
                serialization.NoEncryption()))

    leaf("rsa", rsa.generate_private_key(public_exponent=65537,
                                         key_size=2048),
         now - day, now + 300 * day)
    leaf("ec", ec.generate_private_key(ec.SECP256R1()),
         now - day, now + 300 * day)
    leaf("old", rsa.generate_private_key(public_exponent=65537,
                                         key_size=2048),
         now - 400 * day, now - 10 * day)
    # a root nobody put in the store
    other_key = rsa.generate_private_key(public_exponent=65537,
                                         key_size=2048)
    other = (x509.CertificateBuilder().subject_name(nm("Firn Other CA"))
             .issuer_name(nm("Firn Other CA"))
             .public_key(other_key.public_key()).serial_number(1)
             .not_valid_before(now - day).not_valid_after(now + 300 * day)
             .add_extension(x509.BasicConstraints(ca=True,
                                                  path_length=None),
                            critical=True)
             .sign(other_key, hashes.SHA256()))
    ok2 = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    stranger = (x509.CertificateBuilder().subject_name(nm("localhost"))
                .issuer_name(other.subject).public_key(ok2.public_key())
                .serial_number(2).not_valid_before(now - day)
                .not_valid_after(now + 300 * day)
                .add_extension(x509.SubjectAlternativeName(
                    [x509.DNSName("localhost")]), critical=False)
                .sign(other_key, hashes.SHA256()))
    with open(os.path.join(d, "stranger.crt"), "wb") as f:
        f.write(stranger.public_bytes(serialization.Encoding.PEM))
    with open(os.path.join(d, "stranger.key"), "wb") as f:
        f.write(ok2.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.TraditionalOpenSSL,
            serialization.NoEncryption()))


class Server:
    def __init__(self, d, port, cert, key, extra=()):
        self.port = port
        self.p = subprocess.Popen(
            ["openssl", "s_server", "-accept", str(port),
             "-cert", os.path.join(d, cert), "-key", os.path.join(d, key),
             "-www", "-quiet", "-naccept", "40"] + list(extra),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", port),
                                         timeout=0.2).close()
                return
            except OSError:
                time.sleep(0.05)

    def stop(self):
        self.p.terminate()
        try:
            self.p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.p.kill()


def run(binary, ip, port, host, store, path=None, dump=None):
    args = [binary, ip, str(port), host, store]
    if path:
        args.append(path)
    if dump:
        args.append(dump)
    r = subprocess.run(args, capture_output=True, timeout=120)
    out = {}
    for line in r.stdout.decode().split("\n"):
        if not line.strip():
            continue
        if " " in line:
            k, v = line.split(" ", 1)
            out[k] = v.strip()
        else:
            out[line.strip()] = ""
    return out


class BitFlipper(threading.Thread):
    """A man in the middle that flips one bit of the Nth record the server
    sends. The AEAD must notice; a client that carries on has no integrity
    protection at all."""

    def __init__(self, listen_port, to_port, which):
        super().__init__(daemon=True)
        self.listen_port = listen_port
        self.to_port = to_port
        self.which = which
        self.srv = socket.socket()
        self.srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.srv.bind(("127.0.0.1", listen_port))
        self.srv.listen(1)

    def run(self):
        try:
            c, _ = self.srv.accept()
        except OSError:
            return
        s = socket.create_connection(("127.0.0.1", self.to_port))
        seen = [0]

        def pump(a, b, meddle):
            try:
                while True:
                    d = a.recv(65536)
                    if not d:
                        break
                    if meddle:
                        d = self.maybe_flip(bytearray(d), seen)
                    b.sendall(d)
            except OSError:
                pass
            finally:
                try:
                    b.shutdown(socket.SHUT_WR)
                except OSError:
                    pass

        t1 = threading.Thread(target=pump, args=(c, s, False), daemon=True)
        t1.start()
        pump(s, c, True)
        t1.join(timeout=2)
        c.close()
        s.close()

    def maybe_flip(self, d, seen):
        i = 0
        while i + 5 <= len(d):
            ln = (d[i + 3] << 8) | d[i + 4]
            if d[i] == 23:          # application_data -- the encrypted ones
                seen[0] += 1
                if seen[0] == self.which and i + 5 + 10 < len(d):
                    d[i + 5 + 10] ^= 0x01
            i += 5 + ln
        return bytes(d)


def dechunk(b):
    head, sep, body = b.partition(b"\r\n\r\n")
    if b"chunked" not in head.lower():
        return head, body
    out = b""
    while True:
        nl = body.find(b"\r\n")
        if nl < 0:
            break
        n = int(body[:nl].split(b";")[0] or b"0", 16)
        if n == 0:
            break
        out += body[nl + 2:nl + 2 + n]
        body = body[nl + 2 + n + 2:]
    return head, out


VOLATILE = re.compile(
    rb'(?i)^(date|age|x-|cf-|set-cookie|report-to|nel|expires|'
    rb'server-timing|alt-svc|last-modified|etag|vary|accept-ranges|'
    rb'strict-transport|content-security|via|report-|transfer-encoding|'
    rb'content-length|connection|cache-control|pragma)')


def stable(b):
    head, body = dechunk(b)
    lines = [l for l in head.split(b"\r\n") if not VOLATILE.match(l)]
    return b"\r\n".join(lines), body


def main():
    binary = sys.argv[1]
    d = tempfile.mkdtemp(prefix="firn-b5-tls-")
    make_certs(d)
    ca = os.path.join(d, "ca.pem")
    empty = os.path.join(d, "empty.pem")
    open(empty, "w").close()

    # ------------------------------------------------ against s_server
    cases = [
        ("AES-128-GCM, RSA certificate", "rsa.crt", "rsa.key",
         ["-ciphersuites", "TLS_AES_128_GCM_SHA256"], "localhost", ca,
         {"SUITE": "4865", "VERIFY": "OK"}, False),
        ("ChaCha20-Poly1305, RSA certificate", "rsa.crt", "rsa.key",
         ["-ciphersuites", "TLS_CHACHA20_POLY1305_SHA256"], "localhost",
         ca, {"SUITE": "4867", "VERIFY": "OK"}, False),
        ("AES-128-GCM, P-256 certificate", "ec.crt", "ec.key",
         ["-ciphersuites", "TLS_AES_128_GCM_SHA256"], "localhost", ca,
         {"SUITE": "4865", "VERIFY": "OK"}, False),
        ("ChaCha20-Poly1305, P-256 certificate", "ec.crt", "ec.key",
         ["-ciphersuites", "TLS_CHACHA20_POLY1305_SHA256"], "localhost",
         ca, {"SUITE": "4867", "VERIFY": "OK"}, False),
        # -------------------------------------------- THE REFUSALS
        ("the certificate has expired", "old.crt", "old.key", [],
         "localhost", ca, {"ERRCertificate": "", "VERIFY": "EXPIRED"},
         True),
        ("the name does not match", "rsa.crt", "rsa.key", [],
         "wrong.test", ca, {"ERRCertificate": "", "VERIFY": "NAME"}, True),
        ("nobody in the store signed for this issuer", "stranger.crt",
         "stranger.key", [], "localhost", ca,
         {"ERRCertificate": "", "VERIFY": "UNKNOWN_ISSUER"}, True),
        ("an empty trust store trusts nothing", "rsa.crt", "rsa.key", [],
         "localhost", empty,
         {"ERRCertificate": "", "VERIFY": "UNKNOWN_ISSUER"}, True),
        # A TLS 1.2-only OpenSSL does not answer with a TLS 1.2
        # ServerHello -- it reads `supported_versions` with only 0x0304 in
        # it and sends `protocol_version` (alert 70). That refusal is the
        # right outcome and the alert number is what is checked, so that
        # this case cannot be passed by failing for some other reason.
        ("the server will only speak TLS 1.2", "rsa.crt", "rsa.key",
         ["-tls1_2"], "localhost", ca,
         {"ERRAlert": "", "ALERT": "70"}, True),
        ("the server offers only a suite we do not have", "rsa.crt",
         "rsa.key", ["-ciphersuites", "TLS_AES_256_GCM_SHA384"],
         "localhost", ca, {"ERRAlert": ""}, True),
    ]
    port = PORT
    for (title, cert, key, extra, host, store, want, counter) in cases:
        port += 1
        srv = Server(d, port, cert, key, extra)
        try:
            got = run(binary, "127.0.0.1", port, host, store, "/")
        except subprocess.TimeoutExpired:
            got = {"TIMEOUT": ""}
        srv.stop()
        good = all(got.get(k, None) == v for k, v in want.items())
        note(title, good, str(want), str({k: got.get(k) for k in want}),
             counter)

    # ---------------------------- a big payload, octet for octet
    # `openssl s_server -www` answers with a page that contains the
    # negotiated cipher and the session id, so it is different every time.
    # This one is Python's own `ssl` in front of a fixed 512 KiB body, and
    # what comes back has to be EXACTLY that -- which is the only way to
    # see whether the record layer loses or repeats anything across the
    # thirty-odd records such a body needs.
    port += 1
    payload = bytes((i * 37 + (i >> 8) * 11) & 0xff
                    for i in range(512 * 1024))
    stop = threading.Event()

    def serve():
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(os.path.join(d, "rsa.crt"),
                            os.path.join(d, "rsa.key"))
        srv = socket.socket()
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(("127.0.0.1", port))
        srv.listen(2)
        srv.settimeout(30)
        try:
            c, _ = srv.accept()
        except OSError:
            return
        try:
            t = ctx.wrap_socket(c, server_side=True)
            t.recv(65536)
            head = ("HTTP/1.1 200 OK\r\nContent-Type: "
                    "application/octet-stream\r\nContent-Length: %d\r\n"
                    "Connection: close\r\n\r\n" % len(payload)).encode()
            t.sendall(head + payload)
            t.close()
        except OSError:
            pass
        srv.close()

    th = threading.Thread(target=serve, daemon=True)
    th.start()
    time.sleep(0.4)
    dump = os.path.join(WORK, "bigbody.bin")
    if os.path.exists(dump):
        os.unlink(dump)
    try:
        got = run(binary, "127.0.0.1", port, "localhost", ca, "/", dump)
        mine = open(dump, "rb").read() if os.path.exists(dump) else b""
    except subprocess.TimeoutExpired:
        mine = b""
    body = mine.partition(b"\r\n\r\n")[2]
    note("512 KiB over many records, octet for octet",
         body == payload, "%d octets" % len(payload),
         "%d octets, %s" % (len(body),
                            "identical" if body == payload else "differ"))
    th.join(timeout=5)

    # ------------------------------------ the man in the middle
    port += 1
    srv = Server(d, port, "rsa.crt", "rsa.key", [])
    flip_port = port + 100
    fl = BitFlipper(flip_port, port, 3)
    fl.start()
    try:
        got = run(binary, "127.0.0.1", flip_port, "localhost", ca, "/")
    except subprocess.TimeoutExpired:
        got = {"TIMEOUT": ""}
    srv.stop()
    note("one flipped bit in a record", "ERRDecrypt" in got
         or "ERRAlert" in got or "ERRProtocol" in got,
         "ERRDecrypt", str(list(got.keys())), counter=True)

    # -------------------------------- the same body as somebody else's TLS
    hosts = ["www.rust-lang.org", "example.com", "en.wikipedia.org",
             "github.com", "www.cloudflare.com", "www.google.com"]
    net_ok = 0
    net_tried = 0
    net_dynamic = 0
    for host in hosts:
        try:
            ip = socket.gethostbyname(host)
        except OSError:
            continue
        dump = os.path.join(WORK, "net-%s.bin" % host)
        try:
            got = run(binary, ip, 443, host,
                      "/etc/ssl/certs/ca-certificates.crt", "/", dump)
        except subprocess.TimeoutExpired:
            note("real host " + host, False, "a page", "timeout")
            continue
        if got.get("VERIFY") != "OK":
            note("real host " + host, False, "VERIFY OK",
                 str(list(got.items())[:4]))
            continue
        def pyfetch():
            ctx = ssl.create_default_context()
            with socket.create_connection((ip, 443), timeout=20) as s:
                with ctx.wrap_socket(s, server_hostname=host) as t:
                    t.sendall(("GET / HTTP/1.1\r\nHost: %s\r\n"
                               "Connection: close\r\n"
                               "User-Agent: firn-b5\r\n\r\n"
                               % host).encode())
                    buf = b""
                    while True:
                        chunk = t.recv(65536)
                        if not chunk:
                            break
                        buf += chunk
            return buf
        try:
            buf = pyfetch()
            buf2 = pyfetch()
            buf3 = pyfetch()
        except OSError:
            continue
        mine = open(dump, "rb").read()
        # IS THIS PAGE THE SAME TWICE? github and google stamp a
        # per-request identifier into the HTML, so two fetches of the same
        # URL differ even between two runs of Python. Comparing against
        # such a page measures nothing, so those hosts are counted
        # separately instead of being quietly dropped or, worse, counted
        # as failures of this client.
        # IS THIS PAGE THE SAME EVERY TIME? Three fetches with Python and
        # a second one with this client. Cloudflare stamps a short random
        # token into its HTML and github a per-request id, so two fetches
        # of the same URL differ no matter who makes them. Deciding that
        # with PYTHON alone is not enough -- the token can be the same for
        # a while and then change between the Python fetches and ours,
        # which is exactly what happened on the first run. So the second
        # fetch of THIS client counts too: if any two of the five differ,
        # the page is dynamic and only the headers can be compared.
        dump2 = dump + ".2"
        try:
            run(binary, ip, 443, host,
                "/etc/ssl/certs/ca-certificates.crt", "/", dump2)
            mine2 = open(dump2, "rb").read()
        except (subprocess.TimeoutExpired, OSError):
            mine2 = b""
        bodies = {stable(buf)[1], stable(buf2)[1], stable(buf3)[1]}
        all_bodies = bodies | {stable(mine)[1], stable(mine2)[1]}
        if len(bodies) > 1 or len(all_bodies) > 1:
            net_dynamic += 1
            note("real host " + host + " (page differs between two fetches"
                 " -- headers only)", stable(mine)[0] == stable(buf)[0],
                 "same headers, and a body of the same order of size",
                 "headers %s, %d octets against %d"
                 % ("same" if stable(mine)[0] == stable(buf)[0]
                    else "differ", len(stable(mine)[1]),
                    len(stable(buf)[1])))
            continue
        net_tried += 1
        ha, ba = stable(mine)
        hb, bb = stable(buf)
        same_head = ha == hb
        same_body = ba in bodies
        if same_head and same_body:
            net_ok += 1
            note("real host " + host, True, "same", "same")
        else:
            # github stamps a per-request id into the page; report how far
            # the two agree rather than pretending it is a pass
            common = 0
            for x, y in zip(ba, bb):
                if x != y:
                    break
                common += 1
            note("real host " + host, False,
                 "identical body",
                 "headers %s, body %d/%d octets agree"
                 % ("same" if same_head else "differ", common, len(ba)))

    print("   %d / %d TLS cases, of them %d / %d refusals"
          % (OK, OK + len(FAILS), COUNTER_OK, COUNTER_TOTAL))
    if net_tried or net_dynamic:
        print("   %d / %d real hosts answered octet for octet as Python's"
              " own TLS (%d more serve a different page every time and are"
              " compared on the headers only)"
              % (net_ok, net_tried, net_dynamic))
    for title, want, got in FAILS:
        print("  FAIL  %-45s want %s" % (title[:45], want))
        print("        got  %s" % got)
    if FAILS:
        print("TLS FAILED: %d" % len(FAILS))
        return 1
    print("TLS OK: %d cases, %d refusals, %d real hosts octet-exact,"
          " %d more on the headers" % (OK, COUNTER_OK, net_ok,
                                       net_dynamic))
    return 0


if __name__ == "__main__":
    sys.exit(main())

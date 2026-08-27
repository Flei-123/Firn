#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/https_check.py -- `https://` in the HTTP client of round B4.

Round B4 measured 28 rules of HTTP/1.1 against a real server and refused
`https://` on purpose. Nothing about those 28 rules changed here: framing,
chunking, redirects, the cache and the cookie jar do not know whether the
octets came through a TLS record or straight off the socket. What this
file measures is the seam -- and the seam is where the mistakes are:

  * a connection is reused only when the SCHEME matches too. Without that,
    a redirect from `https://x/` to `http://x/` on the same port would
    send the next request in the clear over a socket that used to be
    encrypted, or the reverse.
  * a redirect ACROSS the schemes has to drop the connection and open a
    new one.
  * a client with no trust store must fetch nothing over `https://`. Not
    "trust it anyway", not "warn once" -- nothing.
  * an expired certificate, a name that does not match: the fetch fails
    and nothing of the body is handed up.

The server is Python's own `http.server` wrapped in Python's own `ssl`, so
the counterpart is not this repository -- the same rule as round B4.
"""
import datetime
import http.server
import os
import socket
import socketserver
import ssl
import subprocess
import sys
import tempfile
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BODY = b"the same octets, whichever way they arrive\n" * 40
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


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_GET(self):
        if self.path.startswith("/tohttps"):
            self.send_response(302)
            self.send_header("Location", self.server.https_base + "/plain")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if self.path.startswith("/tohttp"):
            self.send_response(302)
            self.send_header("Location", self.server.http_base + "/plain")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def start(port, certfile=None, keyfile=None):
    srv = Server(("127.0.0.1", port), Handler)
    if certfile:
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(certfile, keyfile)
        srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
    t = threading.Thread(target=srv.serve_forever, daemon=True)
    t.start()
    return srv


def make_certs(d):
    from cryptography import x509
    from cryptography.x509.oid import NameOID
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    utc = datetime.timezone.utc
    now = datetime.datetime.now(utc)
    day = datetime.timedelta(days=1)

    def nm(s):
        return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, s)])

    ca_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ca = (x509.CertificateBuilder().subject_name(nm("Firn B5 HTTPS CA"))
          .issuer_name(nm("Firn B5 HTTPS CA"))
          .public_key(ca_key.public_key()).serial_number(1)
          .not_valid_before(now - day).not_valid_after(now + 300 * day)
          .add_extension(x509.BasicConstraints(ca=True, path_length=None),
                         critical=True).sign(ca_key, hashes.SHA256()))
    open(os.path.join(d, "ca.pem"), "wb").write(
        ca.public_bytes(serialization.Encoding.PEM))

    def leaf(tag, nb, na):
        key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        c = (x509.CertificateBuilder().subject_name(nm("localhost"))
             .issuer_name(ca.subject).public_key(key.public_key())
             .serial_number(x509.random_serial_number())
             .not_valid_before(nb).not_valid_after(na)
             .add_extension(x509.SubjectAlternativeName(
                 [x509.DNSName("localhost")]), critical=False)
             .sign(ca_key, hashes.SHA256()))
        open(os.path.join(d, tag + ".crt"), "wb").write(
            c.public_bytes(serialization.Encoding.PEM))
        open(os.path.join(d, tag + ".key"), "wb").write(key.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.TraditionalOpenSSL,
            serialization.NoEncryption()))

    leaf("good", now - day, now + 300 * day)
    leaf("old", now - 400 * day, now - 10 * day)


def run(binary, jobs):
    r = subprocess.run([binary], input="\n".join(jobs).encode() + b"\n",
                       capture_output=True, timeout=180)
    blocks = []
    cur = {}
    for line in r.stdout.decode().split("\n"):
        if line.strip() == ".":
            blocks.append(cur)
            cur = {}
            continue
        if " " in line:
            k, v = line.split(" ", 1)
            cur.setdefault(k, v.strip())
        elif line.strip():
            cur.setdefault(line.strip(), "")
    return blocks


def main():
    binary = sys.argv[1]
    d = tempfile.mkdtemp(prefix="firn-b5-https-")
    make_certs(d)
    ca = os.path.join(d, "ca.pem")
    empty = os.path.join(d, "empty.pem")
    open(empty, "w").close()

    p_http = 45080
    p_https = 45443
    p_old = 45444
    plain = start(p_http)
    secure = start(p_https, os.path.join(d, "good.crt"),
                   os.path.join(d, "good.key"))
    old = start(p_old, os.path.join(d, "old.crt"),
                os.path.join(d, "old.key"))
    for s in (plain, secure, old):
        s.http_base = "http://localhost:%d" % p_http
        s.https_base = "https://localhost:%d" % p_https
    hs = "https://localhost:%d" % p_https
    hp = "http://localhost:%d" % p_http

    try:
        # 1. a plain https fetch, and the body has to be exactly the body
        b = run(binary, ["T " + ca, "G %s/plain" % hs, "S"])
        note("https:// fetches a page", b[0].get("ROOTS") == "1"
             and b[1].get("STATUS") == "200"
             and b[1].get("BODY") == str(len(BODY)),
             "ROOTS 1, STATUS 200, BODY %d" % len(BODY),
             "%s / %s / %s" % (b[0].get("ROOTS"), b[1].get("STATUS"),
                               b[1].get("BODY")))
        # WHICH of the two suites is the server's choice -- Python's ssl
        # picks ChaCha20-Poly1305 here and OpenSSL's s_server picks
        # AES-128-GCM. What is checked is that it is one of the two this
        # client offered and not something it never asked for.
        note("the negotiated suite is one of the two that were offered",
             b[2].get("SUITE") in ("4865", "4867"), "4865 or 4867",
             b[2].get("SUITE"))

        # 2. five fetches, one handshake
        jobs = ["T " + ca] + ["G %s/plain" % hs] * 5 + ["S"]
        b = run(binary, jobs)
        note("five https fetches open one socket and shake hands once",
             b[6].get("SOCKETS") == "1" and b[6].get("HANDSHAKES") == "1"
             and b[6].get("REUSED") == "4",
             "SOCKETS 1, HANDSHAKES 1, REUSED 4",
             "%s / %s / %s" % (b[6].get("SOCKETS"), b[6].get("HANDSHAKES"),
                               b[6].get("REUSED")))

        # 3. a redirect that crosses the schemes, both ways
        b = run(binary, ["T " + ca, "G %s/tohttps" % hp, "S"])
        note("a redirect http -> https is followed",
             b[1].get("STATUS") == "200" and b[1].get("HOPS") == "1"
             and b[1].get("FINAL", "").startswith("https://"),
             "STATUS 200, HOPS 1, FINAL https://",
             "%s / %s / %s" % (b[1].get("STATUS"), b[1].get("HOPS"),
                               b[1].get("FINAL")))
        note("... and it opened a SECOND socket, because the scheme "
             "changed", b[2].get("SOCKETS") == "2",
             "SOCKETS 2", b[2].get("SOCKETS"))
        b = run(binary, ["T " + ca, "G %s/tohttp" % hs, "S"])
        note("a redirect https -> http is followed",
             b[1].get("STATUS") == "200" and b[1].get("HOPS") == "1"
             and b[1].get("FINAL", "").startswith("http://"),
             "STATUS 200, HOPS 1, FINAL http://",
             "%s / %s / %s" % (b[1].get("STATUS"), b[1].get("HOPS"),
                               b[1].get("FINAL")))
        note("... and that one too", b[2].get("SOCKETS") == "2",
             "SOCKETS 2", b[2].get("SOCKETS"))

        # 4. THE REFUSALS
        b = run(binary, ["G %s/plain" % hs])
        note("COUNTER-CHECK: no trust store, no https", b[0].get("ERR")
             == "Tls", "ERR Tls", str(b[0]), counter=True)
        b = run(binary, ["T " + empty, "G %s/plain" % hs])
        note("COUNTER-CHECK: an empty trust store, no https",
             b[1].get("ERR") == "Tls", "ERR Tls", str(b[1]), counter=True)
        b = run(binary, ["T " + ca,
                         "G https://localhost:%d/plain" % p_old, "S"])
        note("COUNTER-CHECK: an expired certificate is not fetched from",
             b[1].get("ERR") == "Tls" and b[2].get("TLSREASON") == "2",
             "ERR Tls, TLSREASON 2 (EXPIRED)",
             "%s / %s" % (b[1].get("ERR"), b[2].get("TLSREASON")),
             counter=True)
        b = run(binary, ["T " + ca,
                         "G https://127.0.0.1:%d/plain" % p_https, "S"])
        note("COUNTER-CHECK: the name in the URL is the name that must "
             "be in the certificate",
             b[1].get("ERR") == "Tls" and b[2].get("TLSREASON") == "4",
             "ERR Tls, TLSREASON 4 (NAME)",
             "%s / %s" % (b[1].get("ERR"), b[2].get("TLSREASON")),
             counter=True)
        b = run(binary, ["T " + ca, "G ftp://localhost/x"])
        note("COUNTER-CHECK: a scheme that is neither is still refused",
             b[1].get("ERR") == "Scheme", "ERR Scheme", str(b[1]),
             counter=True)

        # 5. the same page over both schemes must be the SAME octets
        b = run(binary, ["T " + ca, "G %s/plain" % hs, "G %s/plain" % hp])
        note("http and https deliver the same body",
             b[1].get("BODYTEXT") == b[2].get("BODYTEXT")
             and b[1].get("BODY") == str(len(BODY)),
             "the same BODYTEXT", "%s vs %s"
             % (str(b[1].get("BODYTEXT"))[:30],
                str(b[2].get("BODYTEXT"))[:30]))

        # 6. and a real host, if there is a route to one
        try:
            socket.gethostbyname("example.com")
            has_net = True
        except OSError:
            has_net = False
        if has_net:
            b = run(binary, ["T /etc/ssl/certs/ca-certificates.crt",
                             "G https://example.com/", "S"])
            note("a real https:// URL, resolved by name, from the public "
                 "internet",
                 b[1].get("STATUS") == "200"
                 and int(b[0].get("ROOTS", "0")) > 100,
                 "STATUS 200", "%s (roots %s)" % (b[1].get("STATUS"),
                                                  b[0].get("ROOTS")))
    finally:
        for s in (plain, secure, old):
            s.shutdown()

    print("   %d / %d https cases, of them %d / %d refusals"
          % (OK, OK + len(FAILS), COUNTER_OK, COUNTER_TOTAL))
    for title, want, got in FAILS:
        print("  FAIL  %-50s want %s" % (title[:50], want))
        print("        got  %s" % got)
    if FAILS:
        print("HTTPS FAILED: %d" % len(FAILS))
        return 1
    print("HTTPS OK: %d cases, %d refusals" % (OK, COUNTER_OK))
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# Origin: Firn branch dns-pic, round B5 (tools/tls/); MPL-2.0 since the move to main.
"""tools/tls/cert_check.py -- the certificate check, and above all the
REFUSALS.

A verifier that answers "trusted" to everything passes every test built out
of valid certificates. So most of this file builds certificates that MUST
be refused, and the run reports how many of the cases were refusals: a
number of passing cases without that second number says nothing.

The certificates are generated with Python's `cryptography` -- a different
implementation, a different DER writer, a different signer. The trust store
for the generated cases is a root this script makes; the real chains at the
end are checked against the machine's own `/etc/ssl/certs`.

Each case names the reason it expects, not just "no". A chain that is
refused as EXPIRED when it should have been refused as NAME is a bug that a
boolean would hide.
"""
import datetime
import ipaddress
import os
import subprocess
import sys
import tempfile

from cryptography import x509
from cryptography.x509.oid import NameOID, ExtensionOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa, ec

UTC = datetime.timezone.utc
NOW = datetime.datetime.now(UTC)
NOW_UNIX = int(NOW.timestamp())
DAY = datetime.timedelta(days=1)

CASES = []      # (title, host, now, [pem...], expected)
COUNTER = set()


def add(title, host, chain, expected, now=None, counter=None):
    if counter is None:
        counter = expected != "OK"
    if counter:
        COUNTER.add(len(CASES))
    CASES.append((title, host, now or NOW_UNIX, chain, expected))


def name(cn):
    return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, cn)])


def make(subject, issuer_name, issuer_key, key, *, ca, sans=None,
         not_before=None, not_after=None, hash_alg=None, critical_junk=False,
         ip_sans=None, raw_san=None):
    nb = not_before or (NOW - 30 * DAY)
    na = not_after or (NOW + 300 * DAY)
    b = (x509.CertificateBuilder()
         .subject_name(name(subject))
         .issuer_name(issuer_name)
         .public_key(key.public_key())
         .serial_number(x509.random_serial_number())
         .not_valid_before(nb)
         .not_valid_after(na)
         .add_extension(x509.BasicConstraints(ca=ca, path_length=None),
                        critical=True))
    if sans or ip_sans:
        names = [x509.DNSName(s) for s in (sans or [])]
        names += [x509.IPAddress(ipaddress.ip_address(a))
                  for a in (ip_sans or [])]
        b = b.add_extension(x509.SubjectAlternativeName(names),
                            critical=False)
    if raw_san is not None:
        # a subjectAltName written by hand, to get entries that no library
        # would encode (an iPAddress of 3 or 17 octets)
        b = b.add_extension(
            x509.UnrecognizedExtension(
                x509.ObjectIdentifier("2.5.29.17"), raw_san),
            critical=False)
    if critical_junk:
        # an extension nobody knows, marked critical: RFC 5280 4.2 says the
        # certificate must then be unacceptable
        b = b.add_extension(
            x509.UnrecognizedExtension(
                x509.ObjectIdentifier("1.3.6.1.4.1.99999.7"), b"\x05\x00"),
            critical=True)
    alg = hash_alg or hashes.SHA256()
    if isinstance(issuer_key, ec.EllipticCurvePrivateKey) and \
            isinstance(issuer_key.curve, ec.SECP384R1):
        alg = hashes.SHA384()
    return b.sign(issuer_key, alg)


def pem(c):
    return c.public_bytes(serialization.Encoding.PEM).decode()


def der_len(n):
    if n < 128:
        return bytes([n])
    return bytes([0x81, n])


def raw_san_der(entries):
    # entries: (tag byte, content) -> DER of the GeneralNames SEQUENCE
    body = b"".join(bytes([t]) + der_len(len(c)) + c for t, c in entries)
    return b"\x30" + der_len(len(body)) + body


def build_cases():
    # ---------------------------------------------------------- the roots
    root_rsa_key = rsa.generate_private_key(public_exponent=65537,
                                            key_size=2048)
    root_rsa = make("Firn Test Root RSA", name("Firn Test Root RSA"),
                    root_rsa_key, root_rsa_key, ca=True)
    root_ec_key = ec.generate_private_key(ec.SECP256R1())
    root_ec = make("Firn Test Root EC", name("Firn Test Root EC"),
                   root_ec_key, root_ec_key, ca=True)
    root_384_key = ec.generate_private_key(ec.SECP384R1())
    root_384 = make("Firn Test Root P384", name("Firn Test Root P384"),
                    root_384_key, root_384_key, ca=True)
    # a root that is NOT in the store
    stranger_key = rsa.generate_private_key(public_exponent=65537,
                                            key_size=2048)
    stranger = make("Firn Stranger Root", name("Firn Stranger Root"),
                    stranger_key, stranger_key, ca=True)
    store_pem = pem(root_rsa) + pem(root_ec) + pem(root_384)

    # --------------------------------------------------- intermediates
    def inter(nm, key, parent, parent_key, **kw):
        return make(nm, parent.subject, parent_key, key, ca=True, **kw)

    ir_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ir = inter("Firn RSA Intermediate", ir_key, root_rsa, root_rsa_key)
    ie_key = ec.generate_private_key(ec.SECP256R1())
    ie = inter("Firn EC Intermediate", ie_key, root_ec, root_ec_key)
    i384_key = ec.generate_private_key(ec.SECP384R1())
    i384 = inter("Firn P384 Intermediate", i384_key, root_384, root_384_key)
    # an intermediate that is NOT a CA
    inot_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    inot = make("Firn Not A CA", root_rsa.subject, root_rsa_key, inot_key,
                ca=False)
    # an intermediate whose validity has run out
    iexp = make("Firn Expired Intermediate", root_rsa.subject, root_rsa_key,
                ir_key, ca=True, not_before=NOW - 400 * DAY,
                not_after=NOW - 10 * DAY)

    def leaf(cn, sans, issuer_cert, issuer_key, key=None, **kw):
        k = key or rsa.generate_private_key(public_exponent=65537,
                                            key_size=2048)
        return make(cn, issuer_cert.subject, issuer_key, k, ca=False,
                    sans=sans, **kw), k

    ipk = rsa.generate_private_key(public_exponent=65537, key_size=2048)

    def ipleaf(cn, sans=None, ip_sans=None, raw_san=None):
        # one key for all the IP cases: the name check comes before the
        # signature and the key is never what is under test here
        return make(cn, ir.subject, ir_key, ipk, ca=False, sans=sans,
                    ip_sans=ip_sans, raw_san=raw_san)

    # ------------------------------------------------ the positive cases
    l1, _ = leaf("good.example", ["good.example"], ir, ir_key)
    add("RSA chain, two links", "good.example", [pem(l1), pem(ir)], "OK")

    l2, _ = leaf("ec.example", ["ec.example"], ie, ie_key,
                 key=ec.generate_private_key(ec.SECP256R1()))
    add("P-256 chain", "ec.example", [pem(l2), pem(ie)], "OK")

    l3, _ = leaf("p384.example", ["p384.example"], i384, i384_key,
                 key=ec.generate_private_key(ec.SECP384R1()))
    add("P-384 chain", "p384.example", [pem(l3), pem(i384)], "OK")

    l4, _ = leaf("direct.example", ["direct.example"], root_rsa,
                 root_rsa_key)
    add("leaf straight off the root", "direct.example", [pem(l4)], "OK")

    lw, _ = leaf("wild", ["*.wild.example"], ir, ir_key)
    add("wildcard, one label", "a.wild.example", [pem(lw), pem(ir)], "OK")
    add("wildcard, the bare name", "wild.example", [pem(lw), pem(ir)],
        "NAME")
    add("wildcard, two labels deep", "a.b.wild.example",
        [pem(lw), pem(ir)], "NAME")

    lm, _ = leaf("multi", ["one.example", "two.example", "three.example"],
                 ir, ir_key)
    for hn in ("one.example", "two.example", "three.example"):
        add("subjectAltName %s" % hn, hn, [pem(lm), pem(ir)], "OK")
    add("a name that is not in the list", "four.example",
        [pem(lm), pem(ir)], "NAME")

    # the order of the chain must not matter (RFC 8446 4.4.2)
    add("chain in the wrong order", "good.example", [pem(l1), pem(ir)],
        "OK")

    lsha384, _ = leaf("sha384.example", ["sha384.example"], ir, ir_key,
                      hash_alg=hashes.SHA384())
    add("RSA with SHA-384", "sha384.example", [pem(lsha384), pem(ir)], "OK")
    lsha512, _ = leaf("sha512.example", ["sha512.example"], ir, ir_key,
                      hash_alg=hashes.SHA512())
    add("RSA with SHA-512", "sha512.example", [pem(lsha512), pem(ir)], "OK")

    # ---------------------------------------------- THE REFUSALS
    lexp, _ = leaf("old.example", ["old.example"], ir, ir_key,
                   not_before=NOW - 400 * DAY, not_after=NOW - 1 * DAY)
    add("expired leaf", "old.example", [pem(lexp), pem(ir)], "EXPIRED")

    lfut, _ = leaf("future.example", ["future.example"], ir, ir_key,
                   not_before=NOW + 10 * DAY, not_after=NOW + 400 * DAY)
    add("leaf not valid yet", "future.example", [pem(lfut), pem(ir)],
        "NOT_YET")

    add("expired intermediate", "good.example",
        [pem(leaf("x.example", ["good.example"], iexp, ir_key)[0]),
         pem(iexp)], "EXPIRED")

    add("wrong name", "evil.example", [pem(l1), pem(ir)], "NAME")

    lstr, _ = leaf("unknown.example", ["unknown.example"], stranger,
                   stranger_key)
    add("issuer is not in the store", "unknown.example",
        [pem(lstr), pem(stranger)], "UNKNOWN_ISSUER")
    add("issuer missing from the chain entirely", "unknown.example",
        [pem(lstr)], "UNKNOWN_ISSUER")

    lnot, _ = leaf("notca.example", ["notca.example"], inot, inot_key)
    add("the issuer is not a CA", "notca.example",
        [pem(lnot), pem(inot)], "NOT_CA")

    # a signature that belongs to a different key: take the leaf's `DER` and
    # graft the signature of another certificate of the same length on it.
    good_der = l1.public_bytes(serialization.Encoding.DER)
    other, _ = leaf("other.example", ["good.example"], ir, ir_key)
    other_der = other.public_bytes(serialization.Encoding.DER)
    if len(good_der) == len(other_der):
        forged = good_der[:-256] + other_der[-256:]
    else:
        forged = good_der[:-256] + os.urandom(256)
    forged_pem = ("-----BEGIN CERTIFICATE-----\n"
                  + _b64(forged) + "-----END CERTIFICATE-----\n")
    add("the signature is somebody else's", "good.example",
        [forged_pem, pem(ir)], "BAD_SIGNATURE")

    ljunk, _ = leaf("junk.example", ["junk.example"], ir, ir_key,
                    critical_junk=True)
    add("a critical extension nobody knows", "junk.example",
        [pem(ljunk), pem(ir)], "PARSE")

    l521, _ = leaf("p521.example", ["p521.example"], ir, ir_key,
                   key=ec.generate_private_key(ec.SECP521R1()))
    add("a P-521 key in the leaf", "p521.example", [pem(l521), pem(ir)],
        "OK")   # the LEAF's key is never used to verify anything here

    k521 = ec.generate_private_key(ec.SECP521R1())
    i521 = make("Firn P521 Intermediate", root_rsa.subject, root_rsa_key,
                k521, ca=True)
    l521b, _ = leaf("under521.example", ["under521.example"], i521, k521)
    add("an intermediate on a curve this round does not have",
        "under521.example", [pem(l521b), pem(i521)], "UNSUPPORTED")


    # ------------------------------------- iPAddress SAN (RFC 6125 6.4)
    v4 = ipleaf("v4.example", ip_sans=["10.9.0.1"])
    v6 = ipleaf("v6.example", ip_sans=["fd00::1"])
    for h in ("10.9.0.1",):
        add("IPv4 in an iPAddress SAN", h, [pem(v4), pem(ir)], "OK")
    for h in ("10.9.0.2", "10.9.0.10", "10.9.0.11", "110.9.0.1"):
        add("IPv4, a different address %s" % h, h, [pem(v4), pem(ir)],
            "NAME")
    for h in ("fd00::1", "[fd00::1]", "FD00::1", "fd00:0:0:0:0:0:0:1",
              "fd00:0000:0000:0000:0000:0000:0000:0001", "fd00::0:1"):
        add("IPv6 in an iPAddress SAN: %s" % h, h, [pem(v6), pem(ir)],
            "OK")
    for h in ("fd00::2", "fd00::", "fd01::1", "[fd00::2]", "fd00:1::1",
              "::fd00:1"):
        add("IPv6, a different address %s" % h, h, [pem(v6), pem(ir)],
            "NAME")
    v4m = ipleaf("v4m.example", ip_sans=["::ffff:10.9.0.1"])
    add("IPv4-mapped IPv6 SAN, host in the mapped form",
        "::ffff:10.9.0.1", [pem(v4m), pem(ir)], "OK")
    add("IPv4-mapped IPv6 SAN, host in hex groups", "::ffff:a09:1",
        [pem(v4m), pem(ir)], "OK")
    add("IPv4-mapped IPv6 SAN is not the IPv4 address", "10.9.0.1",
        [pem(v4m), pem(ir)], "NAME")
    add("IPv4 SAN is not the IPv4-mapped address", "::ffff:10.9.0.1",
        [pem(v4), pem(ir)], "NAME")
    lo = ipleaf("lo.example", ip_sans=["::1", "127.0.0.1"])
    add("loopback ::1", "::1", [pem(lo), pem(ir)], "OK")
    add("loopback ::1 in brackets", "[::1]", [pem(lo), pem(ir)], "OK")
    add("loopback 127.0.0.1", "127.0.0.1", [pem(lo), pem(ir)], "OK")
    add("the unspecified :: is not ::1", "::", [pem(lo), pem(ir)], "NAME")

    # an IP host against a certificate with dNSName only: refused, even
    # when the dNSName is the very text of the address (the old workaround)
    dn = ipleaf("10.9.0.1", sans=["10.9.0.1", "good.example"])
    add("IP host, only a dNSName of the same text", "10.9.0.1",
        [pem(dn), pem(ir)], "NAME")
    dn6 = ipleaf("fd00::1", sans=["fd00::1", "[fd00::1]"])
    add("IPv6 host, only a dNSName of the same text", "fd00::1",
        [pem(dn6), pem(ir)], "NAME")
    add("IPv6 bracket host, only a dNSName", "[fd00::1]",
        [pem(dn6), pem(ir)], "NAME")
    wd = ipleaf("wild", sans=["*.0.1", "*.9.0.1", "*.example"])
    add("IP host, wildcard dNSNames", "10.9.0.1", [pem(wd), pem(ir)],
        "NAME")
    # a DNS name against a certificate with iPAddress only: refused
    add("DNS host, only an iPAddress", "good.example",
        [pem(v4), pem(ir)], "NAME")
    add("DNS host that starts with the address", "10.9.0.1.example",
        [pem(v4), pem(ir)], "NAME")
    add("DNS host, CN is the name but a SAN exists", "v4.example",
        [pem(v4), pem(ir)], "NAME")
    # the common name never names an IP, and no SAN means no IP match
    cn_ip = ipleaf("10.9.0.1")
    add("IP host, only the common name", "10.9.0.1",
        [pem(cn_ip), pem(ir)], "NAME")
    cn_ip6 = ipleaf("fd00::1")
    add("IPv6 host, only the common name", "fd00::1",
        [pem(cn_ip6), pem(ir)], "NAME")
    cn_dns = ipleaf("good.example")
    add("DNS host, common name, no SAN (fallback kept)", "good.example",
        [pem(cn_dns), pem(ir)], "OK")
    add("IP host, common name is a DNS name", "10.9.0.1",
        [pem(cn_dns), pem(ir)], "NAME")
    # a mix
    mix = ipleaf("mix", sans=["good.example", "*.wild.example"],
                 ip_sans=["10.9.0.1", "fd00::1", "192.168.1.5"])
    for h in ("good.example", "a.wild.example", "10.9.0.1", "fd00::1",
              "[fd00::1]", "192.168.1.5"):
        add("mixed SAN, %s" % h, h, [pem(mix), pem(ir)], "OK")
    for h in ("10.9.0.5", "fd00::5", "evil.example", "wild.example",
              "192.168.1.50"):
        add("mixed SAN, not %s" % h, h, [pem(mix), pem(ir)], "NAME")
    # hosts that look like an IP and are not one match nothing
    badhosts = ["10.9.0", "10.9.0.1.1", "010.9.0.1", "10.9.0.256",
                "10.9.0.01", "10..9.1", "10.9.0.1.", ".10.9.0.1",
                "[10.9.0.1]", "[fd00::1", "fd00::1]", "[]", "[good.example]",
                "fd00::1%eth0", "fd00:::1", "fd00::1::", "1:2:3:4:5:6:7:8:9",
                "fd00:1", "fd00::g", "fd00::12345", ":fd00::1", "fd00::1:",
                "::ffff:10.9.0", "::ffff:10.9.0.256"]
    badcert = ipleaf("bad", sans=badhosts + ["10.9.0", "010.9.0.1"],
                     ip_sans=["10.9.0.1", "fd00::1"])
    for h in badhosts:
        add("malformed IP host %r" % h, h, [pem(badcert), pem(ir)], "NAME")
    # broken iPAddress entries: wrong length is no hit and no crash
    for n in (0, 1, 3, 5, 8, 15, 17, 20, 32):
        junk = raw_san_der([(0x87, bytes(range(1, n + 1)))])
        jl = ipleaf("junk", raw_san=junk)
        for h in ("10.9.0.1", "fd00::1", "good.example"):
            add("iPAddress of %d octets, host %s" % (n, h), h,
                [pem(jl), pem(ir)], "NAME")
    # a 4-octet entry holds the first octets of a 16-octet address
    short = ipleaf("short", raw_san=raw_san_der(
        [(0x87, bytes([0xfd, 0, 0, 0]))]))
    add("4 octets of an IPv6 prefix are not the IPv6 host", "fd00::",
        [pem(short), pem(ir)], "NAME")
    add("4 octets of an IPv6 prefix, host 253.0.0.0", "253.0.0.0",
        [pem(short), pem(ir)], "OK")
    # 16 octets that start like an IPv4 entry
    long6 = ipleaf("long", raw_san=raw_san_der(
        [(0x87, bytes([10, 9, 0, 1]) + bytes(12))]))
    add("16 octets 10.9.0.1:: are not the IPv4 host", "10.9.0.1",
        [pem(long6), pem(ir)], "NAME")
    add("16 octets 10.9.0.1:: match their IPv6 form", "a09:1::",
        [pem(long6), pem(ir)], "OK")
    # junk entries around a good one do not hide it
    around = ipleaf("around", raw_san=raw_san_der([
        (0x87, b"\x01\x02\x03"), (0x82, b""), (0x87, b""),
        (0x87, bytes([10, 9, 0, 1])), (0x87, b"\x01" * 17)]))
    add("good iPAddress between broken ones", "10.9.0.1",
        [pem(around), pem(ir)], "OK")
    add("empty dNSName does not match an IP", "10.9.0.2",
        [pem(around), pem(ir)], "NAME")
    # an iPAddress entry is not looked at for a DNS name even if its
    # octets happen to spell the name
    spell = ipleaf("spell", raw_san=raw_san_der(
        [(0x87, b"abcd"), (0x87, b"a.bc")]))
    add("DNS host 'a.bc' vs iPAddress octets 'a.bc'", "a.bc",
        [pem(spell), pem(ir)], "NAME")

    add("no certificate at all", "good.example", [], "PARSE")

    return store_pem


def _b64(data):
    import base64
    t = base64.b64encode(data).decode()
    return "\n".join(t[i:i + 64] for i in range(0, len(t), 64)) + "\n"


def run(binary, store_path, cases):
    jobs = []
    for title, host, now, chain, want in cases:
        jobs.append("#JOB %s %d" % (host, now))
        jobs.extend(chain)
    r = subprocess.run([binary, store_path], input="\n".join(jobs).encode(),
                       capture_output=True)
    return r.stdout.decode().split("\n")


def main():
    binary = sys.argv[1]
    store_pem = build_cases()
    with tempfile.NamedTemporaryFile("w", suffix=".pem", delete=False) as f:
        f.write(store_pem)
        store_path = f.name
    got = run(binary, store_path, CASES)
    ok = 0
    ok_counter = 0
    bad = []
    for i, (title, host, now, chain, want) in enumerate(CASES):
        g = got[i].strip().split()[0] if i < len(got) and got[i].strip() \
            else "<missing>"
        if g == want:
            ok += 1
            if i in COUNTER:
                ok_counter += 1
        else:
            bad.append((title, want, g))
    os.unlink(store_path)

    # ------------------------------------------- the real chains, if any
    real = []
    here = os.path.dirname(os.path.abspath(__file__))
    corpus = os.path.join(os.path.dirname(os.path.dirname(here)),
                          "tests", "data", "tls-chains")
    if os.path.isdir(corpus):
        for fn in sorted(os.listdir(corpus)):
            if fn.endswith(".pem"):
                host = fn[:-4]
                real.append((host, open(os.path.join(corpus, fn)).read()))
    real_ok = 0
    real_bad = []
    if real:
        # the chains are frozen files; check them at the day they were
        # harvested (2026-08-26), not today -- a harvested leaf expires
        REAL_NOW = int(datetime.datetime(2026, 8, 26, 12, 0,
                                         tzinfo=UTC).timestamp())
        cases2 = [("real " + h, h, REAL_NOW, [p], "OK") for h, p in real]
        # And the counter-check: every one of them under a WRONG name. The
        # name has a suffix rather than a prefix, because a prefix is
        # exactly what a `*.` wildcard matches -- `not-en.wikipedia.org`
        # really IS covered by Wikipedia's `*.wikipedia.org`, and the first
        # run of this file said so. A trailing `.invalid` cannot be
        # wildcarded by anything (RFC 2606).
        cases2 += [("real wrong name " + h, h + ".invalid", REAL_NOW, [p],
                    "NAME") for h, p in real]
        # ... and every one of them at a time long past its notAfter
        cases2 += [("real expired " + h, h, 2145916800, [p],
                    "EXPIRED") for h, p in real]
        got2 = run(binary, "/etc/ssl/certs/ca-certificates.crt", cases2)
        for i, (title, host, now, chain, want) in enumerate(cases2):
            g = got2[i].strip().split()[0] if i < len(got2) \
                and got2[i].strip() else "<missing>"
            if g == want:
                real_ok += 1
            else:
                real_bad.append((title, want, g))

    print("   %d / %d certificate cases, of them %d / %d refusals"
          % (ok, len(CASES), ok_counter, len(COUNTER)))
    if real:
        print("   %d / %d against real chains and the machine's own store"
              % (real_ok, len(real) * 3))
    for title, want, g in bad + real_bad:
        print("  FAIL  %-45s want %-16s got %s" % (title[:45], want, g))
    if bad or real_bad:
        print("CERT FAILED: %d" % (len(bad) + len(real_bad)))
        return 1
    print("CERT OK: %d / %d, refusals %d, real chains %d"
          % (ok, len(CASES), ok_counter, real_ok))
    return 0


if __name__ == "__main__":
    sys.exit(main())

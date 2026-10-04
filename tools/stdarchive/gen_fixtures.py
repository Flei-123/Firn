#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Makes the fixtures of tests/2071..2073 in tests/data/archives/ with GNU tar, Info-ZIP,
Python's tarfile/zipfile and (for the vault) Python `cryptography` + PyNaCl.
They are committed; run this only to regenerate them (the tests embed them with
__include_str and check the exact contents listed at the end)."""
import hashlib, io, os, shutil, stat, subprocess, sys, tarfile, tempfile, zipfile

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "tests", "data", "archives")
os.makedirs(OUT, exist_ok=True)
T = tempfile.mkdtemp()

def w(path, data):
    p = os.path.join(T, path)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "wb").write(data)
    return p

# --- the tree: a miniature JDK
root = os.path.join(T, "tree")
os.makedirs(root)
def put(rel, data, mode):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "wb").write(data)
    os.chmod(p, mode)
put("jdk-21/bin/java", b"#!/bin/sh\necho java 21\n", 0o755)
put("jdk-21/bin/keytool", b"#!/bin/sh\necho keytool\n", 0o755)
put("jdk-21/lib/modules", b"modules\n" * 200, 0o644)
put("jdk-21/release", b'JAVA_VERSION="21.0.4"\n', 0o644)
os.makedirs(os.path.join(root, "jdk-21/legal"))
os.symlink("../lib/modules", os.path.join(root, "jdk-21/bin/modules-link"))
os.symlink("bin/java", os.path.join(root, "jdk-21/java"))
for fmt in ("gnu", "ustar", "pax"):
    pass
subprocess.check_call("tar --format=gnu --sort=name --mtime=@1700000000 --owner=0 --group=0 --numeric-owner -czf %s -C %s jdk-21" % (os.path.join(OUT, "jdk.tar.gz"), root), shell=True)
subprocess.check_call("cd %s && zip -qry -X %s jdk-21" % (root, os.path.join(OUT, "jdk.zip")), shell=True)

# --- GNU long name / long link, ustar prefix, pax unicode
long_name = "gnu-long/" + "/".join(["segment%02d_" % i + "n" * 10 for i in range(9)]) + "/file.txt"   # > 100
long_link_target = "t" * 130
g = os.path.join(T, "g")
os.makedirs(os.path.join(g, os.path.dirname(long_name)))
open(os.path.join(g, long_name), "wb").write(b"gnu long name content\n")
os.symlink(long_link_target, os.path.join(g, "gnu-long/longlink"))
subprocess.check_call("tar --format=gnu --sort=name --mtime=@1700000000 --owner=0 --group=0 --numeric-owner -cf %s -C %s gnu-long" % (os.path.join(OUT, "gnu_long.tar"), g), shell=True)

with tarfile.open(os.path.join(OUT, "ustar_prefix.tar"), "w", format=tarfile.USTAR_FORMAT) as t:
    name = "p" * 60 + "/" + "q" * 60 + "/file.txt"       # 130 octets: prefix + name
    ti = tarfile.TarInfo(name); data = b"ustar prefix content\n"; ti.size = len(data); ti.mtime = 1700000000
    t.addfile(ti, io.BytesIO(data))

with tarfile.open(os.path.join(OUT, "pax_unicode.tar"), "w", format=tarfile.PAX_FORMAT) as t:
    ti = tarfile.TarInfo("pax/Übersicht/Schütz – Ω " + "u" * 120 + ".txt"); data = b"pax unicode content\n"
    ti.size = len(data); ti.mtime = 1700000000.5
    t.addfile(ti, io.BytesIO(data))

# --- hostile archives (tiny)
def hostile(name, builder, fmt=tarfile.GNU_FORMAT):
    with tarfile.open(os.path.join(OUT, name), "w", format=fmt) as t:
        builder(t)
def add_file(t, n, data=b"x", mode=0o644):
    ti = tarfile.TarInfo(n); ti.size = len(data); ti.mode = mode; t.addfile(ti, io.BytesIO(data))
def add_link(t, n, target, hard=False):
    ti = tarfile.TarInfo(n); ti.type = tarfile.LNKTYPE if hard else tarfile.SYMTYPE; ti.linkname = target; t.addfile(ti)
hostile("evil_dotdot.tar", lambda t: (add_file(t, "ok.txt"), add_file(t, "a/../../evil.txt")))
hostile("evil_symlink_file.tar", lambda t: (add_link(t, "x", "/etc"), add_file(t, "x/passwd")))
with zipfile.ZipFile(os.path.join(OUT, "slip.zip"), "w") as z:
    z.writestr("ok.txt", b"x"); z.writestr("../evil.txt", b"x")

# --- the password vault (known answer test for std.secret)
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
import nacl.pwhash.argon2id as a2
salt = bytes(range(16)); nonce = bytes(range(100, 112)); pw = "firn-test-password"
key = a2.kdf(32, pw.encode(), salt, opslimit=1, memlimit=256 * 1024)
recs = [("svc", "tok", b"hello"), ("svc2", "key", b"\x00\xffbinary")]
plain = b""
for s_, k_, v_ in recs:
    plain += len(s_).to_bytes(2, "little") + len(k_).to_bytes(2, "little") + len(v_).to_bytes(4, "little") + s_.encode() + k_.encode() + v_
hdr = b"FSECRET1" + bytes([2, 0, 0, 0]) + salt + (256).to_bytes(4, "little") + (1).to_bytes(4, "little") + nonce
open(os.path.join(OUT, "vault_pw.v1"), "wb").write(hdr + ChaCha20Poly1305(key).encrypt(nonce, plain, hdr))

for n in sorted(os.listdir(OUT)):
    d = open(os.path.join(OUT, n), "rb").read()
    print("%-22s %6d  sha256 %s" % (n, len(d), hashlib.sha256(d).hexdigest()[:16]))
shutil.rmtree(T)

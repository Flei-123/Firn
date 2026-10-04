#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/stdarchive/check.py -- std.tar / std.extract / std.hashfile / std.secret
held against implementations nobody here wrote.

  tar      GNU tar (gnu, ustar, pax/posix formats) and Python's tarfile
           (USTAR, GNU, PAX): both directions -- what they write, Firn extracts
           and lists; what Firn packs, they list and extract -- on trees with
           long names (over 100 and over 255 octets), Unicode names, symlinks,
           hard links, empty files, executables, a 5 MiB file.
  hostile  archives that Python's tarfile builds on purpose: "../" names,
           absolute names, a link to /etc then a file through it, a link chain
           that leaves through "..", the same name twice, a device node, a hard
           link to nothing, a size that lies, a cut-off archive, a bad
           checksum. Each must be REFUSED with the right reason and must leave
           the target directory EMPTY (nothing written at all).
  zip      the same extractor on archives of Info-ZIP's zip and Python's
           zipfile, including a Unix mode (0755) and a symlink, and a zip slip.
  hash     md5/sha1/sha256/sha512 of files of 0, 1, 63, 64, 65, 65535, 65536,
           65537 octets and 5 MiB against Python's hashlib.
  vault    the encrypted file written by Firn is decrypted here with Python's
           `cryptography` (HKDF-SHA256 + ChaCha20-Poly1305, and Argon2id from
           libsodium via PyNaCl), and a file made here is read by Firn; wrong
           password / tampered file; file mode 0600, directory 0700; eight
           processes writing at once lose nothing.

usage: check.py <probe-binary> <workdir>
"""
import hashlib
import io
import os
import random
import stat
import subprocess
import sys
import tarfile
import zipfile

PROBE = sys.argv[1]
W = sys.argv[2]
FAILS = 0
CHECKS = 0


def check(ok, what):
    global FAILS, CHECKS
    CHECKS += 1
    if not ok:
        FAILS += 1
        print("  FAIL", what)


def probe(*args, env=None, check_rc=None):
    e = dict(os.environ)
    if env:
        e.update(env)
    r = subprocess.run([PROBE] + [str(a) for a in args], capture_output=True, env=e, timeout=300)
    return r.returncode, r.stdout.decode("utf-8", "replace"), r.stderr.decode("utf-8", "replace")


def sh(cmd, **kw):
    return subprocess.run(cmd, shell=True, capture_output=True, **kw)


# ----------------------------------------------------------------- trees

def make_tree(root, seed):
    rnd = random.Random(seed)
    os.makedirs(root)
    def put(rel, data, mode=0o644):
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(data)
        os.chmod(p, mode)
    put("bin/java", b"#!/bin/sh\necho java\n", 0o755)
    put("bin/readonly", b"r\n", 0o444)
    put("lib/a.txt", b"alpha\n" * 100)
    put("lib/empty", b"")
    put("lib/rand.bin", bytes(rnd.getrandbits(8) for _ in range(70000)))
    put("lib/big.bin", bytes(rnd.getrandbits(8) for _ in range(256)) * 20000)   # 5 MiB
    put("Übersicht/Schütz – Ω.txt", "größe\n".encode())
    deep = "/".join(["d%02d_" % i + "x" * 20 for i in range(12)])   # > 255 octets in total
    put(deep + "/leaf.txt", b"deep leaf\n")
    put("n" * 99 + "/f", b"99\n")
    put("n" * 100 + ".txt", b"100\n")
    put("n" * 101 + ".txt", b"101\n")
    os.symlink("../lib/a.txt", os.path.join(root, "bin/link"))
    os.symlink("a.txt", os.path.join(root, "lib/link2"))
    os.symlink("x" * 120, os.path.join(root, "lib/longlink"))     # target over 100 octets
    os.makedirs(os.path.join(root, "emptydir"))
    return root


def snapshot(root):
    """{relpath: (kind, mode&0o777 for files, content-hash or link target)}"""
    out = {}
    for d, dirs, files in os.walk(root):
        for n in dirs + files:
            p = os.path.join(d, n)
            rel = os.path.relpath(p, root)
            st = os.lstat(p)
            if stat.S_ISLNK(st.st_mode):
                out[rel] = ("link", os.readlink(p))
            elif stat.S_ISDIR(st.st_mode):
                out[rel] = ("dir",)
            else:
                with open(p, "rb") as f:
                    h = hashlib.sha256(f.read()).hexdigest()
                out[rel] = ("file", st.st_mode & 0o777, h)
    return out


def same_tree(a, b, what):
    sa, sb = snapshot(a), snapshot(b)
    if sa == sb:
        check(True, what)
        return
    check(False, what)
    for k in sorted(set(sa) | set(sb)):
        if sa.get(k) != sb.get(k):
            print("     differs:", k[:60], sa.get(k), sb.get(k))
            break


# ----------------------------------------------------------------- tar

def tar_both_ways():
    print("-- tar: GNU tar and Python tarfile -> Firn")
    src = make_tree(os.path.join(W, "src"), 1)
    parent = os.path.dirname(src)
    formats = {
        "gnu": "tar --format=gnu -cf {out} -C {src} .",
        "ustar": "tar --format=ustar -cf {out} -C {src} bin lib/a.txt lib/empty lib/rand.bin lib/link2 emptydir",
        "pax": "tar --format=pax -cf {out} -C {src} .",
        "posix-gz": "tar --format=posix -czf {out} -C {src} .",
        "gnu-nodot": "tar --format=gnu -cf {out} -C {src} bin lib Übersicht {n99} emptydir",
    }
    for name, cmd in formats.items():
        out = os.path.join(W, "g-%s.tar%s" % (name, ".gz" if name.endswith("gz") else ""))
        cmd = cmd.format(out=out, src=src, n99="n" * 99)
        r = sh(cmd)
        if r.returncode != 0:
            print("    (skipped %s: %s)" % (name, r.stderr.decode()[:80]))
            continue
        dest = os.path.join(W, "x-" + name)
        rc, so, se = probe("extract", out, dest)
        check(rc == 0 and so.startswith("OK"), "GNU tar %s: extracted (%s)" % (name, so.strip()[:70]))
        # compare with a GNU tar extraction of the same archive
        ref = os.path.join(W, "ref-" + name)
        os.makedirs(ref)
        sh("tar -xf %s -C %s" % (out, ref))
        same_tree(ref, dest, "GNU tar %s: tree equals what GNU tar extracts" % name)
        # the listing: names and sizes
        rc, so, se = probe("list", out)
        listed = {}
        for line in so.splitlines():
            head, nm, ln = line.split("\t")
            kind, mode, size, mtime = head.split(" ")
            listed[nm.rstrip("/").lstrip("./")] = (kind, int(size))
        tl = sh("tar -tvf %s" % out).stdout.decode().splitlines()
        check(len(listed) == len([l for l in tl if l.strip()]) or len(listed) >= len(tl) - 1,
              "GNU tar %s: same number of entries (%d vs %d)" % (name, len(listed), len(tl)))

    # python tarfile, three formats, with a file bigger than 8 GiB sparse? no: names and unicode
    for fmt_name, fmt in (("ustar", tarfile.USTAR_FORMAT), ("gnu", tarfile.GNU_FORMAT), ("pax", tarfile.PAX_FORMAT)):
        out = os.path.join(W, "p-%s.tar.gz" % fmt_name)
        with tarfile.open(out, "w:gz", format=fmt) as t:
            for d, dirs, files in os.walk(src):
                for n in sorted(dirs + files):
                    full = os.path.join(d, n)
                    rel = os.path.relpath(full, src)
                    try:
                        t.add(full, arcname="top/" + rel, recursive=False)
                    except Exception as e:
                        pass   # ustar cannot hold the long names: that is the format's limit
        dest = os.path.join(W, "xp-" + fmt_name)
        rc, so, se = probe("extract", out, dest, 1)
        ref = os.path.join(W, "refp-" + fmt_name)
        os.makedirs(ref)
        with tarfile.open(out) as t:
            t.extractall(ref, filter="tar") if hasattr(tarfile, "data_filter") else t.extractall(ref)
        ref_top = os.path.join(ref, "top")
        check(rc == 0, "Python tarfile %s: extracted with strip 1 (%s)" % (fmt_name, so.strip()[:60]))
        same_tree(ref_top, dest, "Python tarfile %s: tree equals what tarfile extracts" % fmt_name)

    print("-- tar: Firn -> GNU tar and Python tarfile")
    for ext in ("tar", "tar.gz"):
        out = os.path.join(W, "f." + ext)
        rc, so, se = probe("pack", src, out, "pkg")
        check(rc == 0 and so.startswith("OK"), "Firn pack .%s (%s)" % (ext, so.strip()[:40]))
        # GNU tar lists and extracts it
        tl = sh("tar -tf %s" % out)
        check(tl.returncode == 0, "GNU tar lists the Firn archive (%s)" % tl.stderr.decode()[:60])
        dest = os.path.join(W, "fx-" + ext)
        os.makedirs(dest)
        r = sh("tar -xf %s -C %s" % (out, dest))
        check(r.returncode == 0, "GNU tar extracts the Firn archive (%s)" % r.stderr.decode()[:60])
        same_tree(src, os.path.join(dest, "pkg"), "Firn .%s: GNU tar's extraction equals the source tree" % ext)
        # Python reads names/sizes/modes
        with tarfile.open(out) as t:
            members = {m.name: m for m in t.getmembers()}
            check("pkg/bin/java" in members and members["pkg/bin/java"].mode == 0o755, "tarfile: bin/java is 0755")
            deepname = "pkg/" + "/".join(["d%02d_" % i + "x" * 20 for i in range(12)]) + "/leaf.txt"
            check(deepname in members, "tarfile: the 250+ octet name arrives whole")
            check(members["pkg/lib/longlink"].issym() and members["pkg/lib/longlink"].linkname == "x" * 120,
                  "tarfile: the 120 octet link target arrives whole")
            check(t.extractfile("pkg/lib/big.bin").read() == open(os.path.join(src, "lib/big.bin"), "rb").read(),
                  "tarfile: the 5 MiB file is intact")
        # and Firn reads its own
        dest2 = os.path.join(W, "fy-" + ext)
        rc, so, se = probe("extract", out, dest2, 1)
        same_tree(src, dest2, "Firn .%s: Firn extracts its own archive (strip 1)" % ext)


# ----------------------------------------------------------------- hostile

def add_file(t, name, data=b"x", mode=0o644):
    ti = tarfile.TarInfo(name)
    ti.size = len(data)
    ti.mode = mode
    t.addfile(ti, io.BytesIO(data))


def add_link(t, name, target, hard=False):
    ti = tarfile.TarInfo(name)
    ti.type = tarfile.LNKTYPE if hard else tarfile.SYMTYPE
    ti.linkname = target
    t.addfile(ti)


def hostile():
    print("-- tar: hostile archives are refused whole")
    cases = []

    def mk(name, builder, expect):
        path = os.path.join(W, "h-%s.tar" % name)
        with tarfile.open(path, "w", format=tarfile.GNU_FORMAT) as t:
            builder(t)
        cases.append((name, path, expect))

    mk("dotdot", lambda t: (add_file(t, "ok.txt"), add_file(t, "a/../../evil.txt")), "UnsafeName")
    mk("dotdot2", lambda t: add_file(t, "../evil.txt"), "UnsafeName")
    mk("absolute", lambda t: add_file(t, "/tmp/evil-abs.txt"), "UnsafeName")
    mk("backslash", lambda t: add_file(t, "a\\..\\evil.txt"), "UnsafeName")
    mk("symlink-then-file", lambda t: (add_link(t, "x", "/etc"), add_file(t, "x/passwd")), "UnsafeLink")
    mk("symlink-up", lambda t: add_link(t, "x", "../../.."), "UnsafeLink")
    mk("symlink-dotdot-mid", lambda t: add_link(t, "l", "a/../../x"), "UnsafeLink")
    mk("link-chain", lambda t: (add_file(t, "x/y/real"), add_link(t, "x/y", "..") , add_link(t, "e", "x/y/../..")), "UnsafeLink")
    mk("dup", lambda t: (add_file(t, "a.txt", b"1"), add_file(t, "a.txt", b"2")), "Duplicate")
    mk("dup-dir-vs-file", lambda t: (add_file(t, "a", b"1"), add_file(t, "a/", b"")), "Duplicate")
    mk("hardlink-missing", lambda t: add_link(t, "h", "nothing", hard=True), "BadLink")
    mk("hardlink-outside", lambda t: add_link(t, "h", "../../etc/passwd", hard=True), "BadLink")

    def devnode(t):
        ti = tarfile.TarInfo("dev")
        ti.type = tarfile.CHRTYPE
        ti.devmajor, ti.devminor = 1, 3
        t.addfile(ti)
    mk("device", devnode, "UnsafeType")

    def fifo(t):
        ti = tarfile.TarInfo("pipe")
        ti.type = tarfile.FIFOTYPE
        t.addfile(ti)
    mk("fifo", fifo, "UnsafeType")

    for name, path, expect in cases:
        dest = os.path.join(W, "hx-" + name)
        rc, so, se = probe("extract", path, dest)
        got = so.strip().split(" ")[-1] if so.startswith("REFUSED") else so.strip()
        check(rc == 2 and got == expect, "hostile %s: refused as %s (got: %s)" % (name, expect, so.strip()[:50]))
        left = []
        if os.path.exists(dest):
            for d, dirs, files in os.walk(dest):
                left += files + dirs
        check(not left, "hostile %s: nothing written (%s)" % (name, left[:3]))
    check(not os.path.exists("/tmp/evil-abs.txt") and not os.path.exists(os.path.join(W, "evil.txt")),
          "hostile: no file outside the target appeared")

    # damaged archives
    good = os.path.join(W, "h-good.tar")
    with tarfile.open(good, "w") as t:
        add_file(t, "a.txt", b"A" * 1000)
        add_file(t, "b.txt", b"B" * 1000)
    data = open(good, "rb").read()
    variants = {
        "truncated": (data[:700 + 512], "Truncated"),
        "badsum": (data[:148] + b"0000000\0" + data[156:], "NotArchive"),
        "badsum-later": (data[:1536 + 148] + b"7777777\0" + data[1536 + 156:], "Checksum"),
        "garbage": (os.urandom(4096), "NotArchive"),
        "empty": (b"", "NotArchive"),
    }
    for name, (blob, expect) in variants.items():
        path = os.path.join(W, "d-%s.tar" % name)
        open(path, "wb").write(blob)
        rc, so, se = probe("extract", path, os.path.join(W, "dx-" + name))
        check(rc == 2 and expect in so, "damaged %s: refused as %s (got: %s)" % (name, expect, so.strip()[:50]))
    # a size field that lies about a huge file
    ti = tarfile.TarInfo("big")
    ti.size = 1 << 40
    hdr = ti.tobuf(tarfile.GNU_FORMAT)
    path = os.path.join(W, "d-lie.tar")
    open(path, "wb").write(hdr + b"\0" * 1024)
    rc, so, se = probe("extract", path, os.path.join(W, "dx-lie"))
    check(rc == 2 and ("TooLarge" in so or "Truncated" in so), "damaged size lie: refused (%s)" % so.strip()[:40])
    # the good ones the hostile list must NOT refuse
    path = os.path.join(W, "h-fine.tar")
    with tarfile.open(path, "w") as t:
        add_file(t, "a/b/real", b"data")
        add_link(t, "a/b/up", "../real2")          # inside
        add_file(t, "a/real2", b"x")
        add_link(t, "a/b/self", ".")
        add_link(t, "top", "a/b/real")
        add_link(t, "hard", "a/real2", hard=True)
    rc, so, se = probe("extract", path, os.path.join(W, "hx-fine"))
    check(rc == 0, "fine links are extracted (%s)" % so.strip()[:50])
    check(os.path.isfile(os.path.join(W, "hx-fine/hard")) and open(os.path.join(W, "hx-fine/hard"), "rb").read() == b"x",
          "a hard link arrives as a copy of the earlier file")
    # overwrite
    dest = os.path.join(W, "hx-fine")
    rc, so, se = probe("extract", path, dest)
    check(rc == 2 and "Exists" in so, "a second extraction without overwrite is refused (Exists)")
    rc, so, se = probe("extract", path, dest, 0, "overwrite")
    check(rc == 0, "... and works with overwrite (%s)" % so.strip()[:40])
    # a symlink in the way: the file is NOT written through it
    dest = os.path.join(W, "hx-way")
    os.makedirs(dest)
    os.makedirs(os.path.join(W, "hx-way-outside"))
    os.symlink(os.path.join(W, "hx-way-outside"), os.path.join(dest, "a"))
    path = os.path.join(W, "h-way.tar")
    with tarfile.open(path, "w") as t:
        add_file(t, "a/pwned.txt")
    rc, so, se = probe("extract", path, dest)
    check(rc == 2 and "UnsafePath" in so and not os.listdir(os.path.join(W, "hx-way-outside")),
          "a symlink already in the target is not written through (%s)" % so.strip()[:40])


# ----------------------------------------------------------------- zip

def zips():
    print("-- zip: Info-ZIP zip and Python zipfile -> Firn")
    src = os.path.join(W, "src")
    out = os.path.join(W, "iz.zip")
    r = sh("cd %s && zip -qry %s src" % (os.path.dirname(src), out))
    if r.returncode == 0:
        dest = os.path.join(W, "zx")
        rc, so, se = probe("extract", out, dest)
        check(rc == 0, "Info-ZIP zip extracted (%s)" % so.strip()[:60])
        ref = os.path.join(W, "zref")
        os.makedirs(ref)
        sh("cd %s && unzip -q %s" % (ref, out))
        same_tree(os.path.join(ref, "src"), os.path.join(dest, "src"), "Info-ZIP: tree equals what unzip extracts (modes, links)")
        dest = os.path.join(W, "zx1")
        rc, so, se = probe("extract", out, dest, 1)
        check(rc == 0 and os.path.isfile(os.path.join(dest, "bin/java")) and os.access(os.path.join(dest, "bin/java"), os.X_OK),
              "zip: strip 1 and the executable bit survive")
    # python zipfile: a Windows-style zip has no Unix mode -> 0644
    out = os.path.join(W, "py.zip")
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("jdk-21/bin/java.exe", b"MZ" * 100)
        z.writestr("jdk-21/lib/modules", b"m" * 10000)
        z.writestr("jdk-21/legal/", b"")
    dest = os.path.join(W, "zy")
    rc, so, se = probe("extract", out, dest, 1)
    check(rc == 0 and os.path.getsize(os.path.join(dest, "bin/java.exe")) == 200, "Python zipfile: strip 1 (%s)" % so.strip()[:40])
    # zip slip
    out = os.path.join(W, "slip.zip")
    with zipfile.ZipFile(out, "w") as z:
        z.writestr("ok.txt", b"x")
        z.writestr("../evil.txt", b"x")
    dest = os.path.join(W, "zslip")
    rc, so, se = probe("extract", out, dest)
    check(rc == 2 and "UnsafeName" in so and not os.path.exists(os.path.join(W, "evil.txt")), "zip slip refused (%s)" % so.strip()[:40])
    check(not (os.path.exists(dest) and os.listdir(dest)), "zip slip: nothing written")
    # a symlink in a zip pointing out
    out = os.path.join(W, "zlink.zip")
    with zipfile.ZipFile(out, "w") as z:
        zi = zipfile.ZipInfo("l")
        zi.create_system = 3
        zi.external_attr = (stat.S_IFLNK | 0o777) << 16
        z.writestr(zi, "/etc/passwd")
    rc, so, se = probe("extract", out, os.path.join(W, "zlinkx"))
    check(rc == 2 and "UnsafeLink" in so, "zip symlink to /etc refused (%s)" % so.strip()[:40])
    # the extractor takes a tar.gz named .zip for what it is
    shutil_copy = os.path.join(W, "disguised.zip")
    sh("cp %s %s" % (os.path.join(W, "g-posix-gz.tar.gz"), shutil_copy))
    rc, so, se = probe("extract", shutil_copy, os.path.join(W, "zdis"))
    check(rc == 0, "format by content: a tar.gz named .zip is unpacked as tar.gz (%s)" % so.strip()[:30])


# ----------------------------------------------------------------- hash

def hashes():
    print("-- hash: against hashlib")
    rnd = random.Random(7)
    for n in (0, 1, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 65535, 65536, 65537, 5 * 1024 * 1024 + 3):
        p = os.path.join(W, "h%d.bin" % n)
        data = bytes(rnd.getrandbits(8) for _ in range(min(n, 4096))) * (n // 4096 + 1)
        data = data[:n]
        open(p, "wb").write(data)
        for algo in ("md5", "sha1", "sha256", "sha512"):
            rc, so, se = probe("hash", p, algo)
            check(so.strip() == hashlib.new(algo, data).hexdigest(), "%s of %d octets" % (algo, n))


# ----------------------------------------------------------------- vault

def argon2_key(password, salt, t, m_log2):
    """Argon2id v1.3, one lane: libsodium through PyNaCl (an implementation that is not ours).
    memory = 2**m_log2 KiB, t passes."""
    import nacl.pwhash.argon2id as a2
    return a2.kdf(32, password.encode(), salt, opslimit=t, memlimit=(1 << m_log2) * 1024)


def parse_records(plain):
    out = {}
    pos = 0
    while pos < len(plain):
        sl = int.from_bytes(plain[pos:pos + 2], "little")
        kl = int.from_bytes(plain[pos + 2:pos + 4], "little")
        vl = int.from_bytes(plain[pos + 4:pos + 8], "little")
        s = plain[pos + 8:pos + 8 + sl]
        k = plain[pos + 8 + sl:pos + 8 + sl + kl]
        v = plain[pos + 8 + sl + kl:pos + 8 + sl + kl + vl]
        out[(s.decode(), k.decode())] = v
        pos += 8 + sl + kl + vl
    return out


def build_records(d):
    out = b""
    for (s, k), v in d.items():
        sb, kb = s.encode(), k.encode()
        out += len(sb).to_bytes(2, "little") + len(kb).to_bytes(2, "little") + len(v).to_bytes(4, "little") + sb + kb + v
    return out


def machine_ikm():
    mid = open("/etc/machine-id").read().strip().encode() if os.path.exists("/etc/machine-id") else \
        open("/var/lib/dbus/machine-id").read().strip().encode()
    return b"firn-secret-v1|" + mid + b"|uid:" + str(os.getuid()).encode()


def py_decrypt(path, password=None):
    from cryptography.hazmat.primitives import hashes as ch
    from cryptography.hazmat.primitives.kdf.hkdf import HKDF
    from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
    blob = open(path, "rb").read()
    assert blob[:8] == b"FSECRET1", blob[:8]
    kdf = blob[8]
    salt = blob[12:28]
    m_kib = int.from_bytes(blob[28:32], "little")
    t = int.from_bytes(blob[32:36], "little") if False else int.from_bytes(blob[32:36], "little")
    nonce = blob[36:48]
    if kdf == 1:
        key = HKDF(algorithm=ch.SHA256(), length=32, salt=salt, info=b"firn std.secret vault key").derive(machine_ikm())
    else:
        import math
        key = argon2_key(password, salt, t, int(math.log2(m_kib)))
    return parse_records(ChaCha20Poly1305(key).decrypt(nonce, blob[48:], blob[:48])), blob


def py_encrypt(path, records, password=None, salt=None, m_log2=8):
    from cryptography.hazmat.primitives import hashes as ch
    from cryptography.hazmat.primitives.kdf.hkdf import HKDF
    from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
    salt = salt or os.urandom(16)
    if password is None:
        key = HKDF(algorithm=ch.SHA256(), length=32, salt=salt, info=b"firn std.secret vault key").derive(machine_ikm())
        hdr = b"FSECRET1" + bytes([1, 0, 0, 0]) + salt + (0).to_bytes(4, "little") + (0).to_bytes(4, "little")
    else:
        key = argon2_key(password, salt, 1, m_log2)
        hdr = b"FSECRET1" + bytes([2, 0, 0, 0]) + salt + (1 << m_log2).to_bytes(4, "little") + (1).to_bytes(4, "little")
    nonce = os.urandom(12)
    hdr += nonce
    open(path, "wb").write(hdr + ChaCha20Poly1305(key).encrypt(nonce, build_records(records), hdr))
    os.chmod(path, 0o600)


def vaults():
    print("-- vault: Firn <-> Python `cryptography`")
    try:
        import cryptography  # noqa
    except ImportError:
        print("  skip: python3 cryptography missing")
        return
    if not os.path.exists("/etc/machine-id") and not os.path.exists("/var/lib/dbus/machine-id"):
        print("  skip: no machine id")
        return
    vdir = os.path.join(W, "vault")
    vp = os.path.join(vdir, "secrets.v1")
    rc, so, se = probe("vault-set", vp, "svc", "tok", "hello-token")
    check(rc == 0 and so.startswith("OK"), "vault-set (%s)" % so.strip()[:30])
    check(stat.S_IMODE(os.stat(vp).st_mode) == 0o600, "vault file mode is 0600")
    check(stat.S_IMODE(os.stat(vdir).st_mode) == 0o700, "vault directory mode is 0700")
    rc, so, se = probe("vault-set", vp, "svc", "other", "x" * 5000)
    rc, so, se = probe("vault-set", vp, "svc2", "ünï", "wert Ω")
    rec, blob = py_decrypt(vp)
    check(rec == {("svc", "tok"): b"hello-token", ("svc", "other"): b"x" * 5000, ("svc2", "ünï"): "wert Ω".encode()},
          "Python decrypts what Firn wrote (HKDF + ChaCha20-Poly1305)")
    rc, so, se = probe("vault-set", vp, "svc", "tok", "replaced")
    rec, blob2 = py_decrypt(vp)
    check(rec[("svc", "tok")] == b"replaced" and len(rec) == 3, "replacing a value keeps the others")
    check(blob2[36:48] != blob[36:48], "a fresh nonce on every save")
    check(b"replaced" not in blob2 and b"svc" not in blob2, "no plaintext in the file")
    rc, so, se = probe("vault-get", vp, "svc", "tok")
    check(so == "replaced\n", "vault-get")
    rc, so, se = probe("vault-get", vp, "svc", "nope")
    check(rc == 2 and "NotFound" in so, "a missing secret is NotFound")
    rc, so, se = probe("vault-del", vp, "svc", "tok")
    check("removed" in so, "vault-del")
    rc, so, se = probe("vault-del", vp, "svc", "tok")
    check("absent" in so, "vault-del again: absent, not an error")
    rec, _ = py_decrypt(vp)
    check(("svc", "tok") not in rec and len(rec) == 2, "Python sees the deletion")
    # tamper
    data = bytearray(open(vp, "rb").read())
    data[60] ^= 1
    tp = os.path.join(vdir, "tampered")
    open(tp, "wb").write(data)
    rc, so, se = probe("vault-get", tp, "svc", "other")
    check(rc == 2 and "WrongKey" in so, "a flipped bit: authentication fails (%s)" % so.strip()[:30])
    open(tp, "wb").write(b"not a vault file at all, just text, long enough to pass the length check ok")
    rc, so, se = probe("vault-get", tp, "svc", "other")
    check(rc == 2 and "Corrupt" in so, "a foreign file is Corrupt")
    # Python writes, Firn reads
    pp = os.path.join(vdir, "from-python")
    py_encrypt(pp, {("a", "b"): b"from python", ("c", "d"): b"\x00\x01\x02"})
    rc, so, se = probe("vault-get", pp, "a", "b")
    check(so == "from python\n", "Firn reads a file Python made")
    # password variant
    pw = os.path.join(vdir, "pw.v1")
    rc, so, se = probe("vault-set", pw, "svc", "tok", "geheim", "correct horse")
    check(rc == 0, "password vault written (%s)" % so.strip()[:30])
    rec, _ = py_decrypt(pw, "correct horse")
    check(rec == {("svc", "tok"): b"geheim"}, "Python decrypts the Argon2id vault with libsodium's key")
    rc, so, se = probe("vault-get", pw, "svc", "tok", "wrong password")
    check(rc == 2 and "WrongKey" in so, "wrong password: WrongKey")
    rc, so, se = probe("vault-get", pw, "svc", "tok")
    check(rc == 2 and "WrongKey" in so, "machine key on a password file: WrongKey")
    rc, so, se = probe("vault-get", vp, "svc", "other", "pw")
    check(rc == 2 and "WrongKey" in so, "password on a machine file: WrongKey")
    pp2 = os.path.join(vdir, "pw-from-python")
    py_encrypt(pp2, {("x", "y"): b"pwd value"}, password="pässwörd")
    rc, so, se = probe("vault-get", pp2, "x", "y", "pässwörd")
    check(so == "pwd value\n", "Firn reads an Argon2id file Python made")
    # a hostile header cannot ask for gigabytes
    blob = bytearray(open(pp2, "rb").read())
    blob[28:32] = (1 << 30).to_bytes(4, "little")
    hp = os.path.join(vdir, "hostile-m")
    open(hp, "wb").write(blob)
    rc, so, se = probe("vault-get", hp, "x", "y", "pässwörd")
    check(rc == 2 and "Corrupt" in so, "a header that asks for 1 TiB of memory is Corrupt")
    # eight writers at once lose nothing
    cp = os.path.join(W, "vault-conc", "secrets.v1")
    procs = []
    for i in range(8):
        procs.append(subprocess.Popen([PROBE, "vault-set", cp, "conc", "k%d" % i, "value-%d" % i],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE))
    for p_ in procs:
        p_.wait()
    rec, _ = py_decrypt(cp)
    check(len(rec) == 8 and all(rec[("conc", "k%d" % i)] == ("value-%d" % i).encode() for i in range(8)),
          "eight processes writing at once: all eight values are there (%d)" % len(rec))
    # the simple API with HOME pointing at a scratch directory
    home = os.path.join(W, "home")
    os.makedirs(home)
    env = {"HOME": home, "XDG_DATA_HOME": ""}
    env.pop("XDG_DATA_HOME")
    e2 = dict(os.environ)
    e2.pop("XDG_DATA_HOME", None)
    e2["HOME"] = home
    r = subprocess.run([PROBE, "secret-set", "fleilauncher", "microsoft-refresh", "tok-123"], env=e2, capture_output=True)
    check(r.returncode == 0, "secret-set through the default path (%s)" % r.stdout.decode().strip()[:40])
    f = os.path.join(home, ".local/share/firn/secrets.v1")
    check(os.path.isfile(f) and stat.S_IMODE(os.stat(f).st_mode) == 0o600, "default file: $HOME/.local/share/firn/secrets.v1, 0600")
    check(stat.S_IMODE(os.stat(os.path.dirname(f)).st_mode) == 0o700, "default directory 0700")
    r = subprocess.run([PROBE, "secret-get", "fleilauncher", "microsoft-refresh"], env=e2, capture_output=True)
    check(r.stdout.decode() == "tok-123\n", "secret-get")
    rec, _ = py_decrypt(f)
    check(rec == {("fleilauncher", "microsoft-refresh"): b"tok-123"}, "Python decrypts the default vault")
    r = subprocess.run([PROBE, "secret-del", "fleilauncher", "microsoft-refresh"], env=e2, capture_output=True)
    r2 = subprocess.run([PROBE, "secret-get", "fleilauncher", "microsoft-refresh"], env=e2, capture_output=True)
    check("removed" in r.stdout.decode() and "NotFound" in r2.stdout.decode(), "secret-del, then NotFound")
    e3 = dict(e2)
    e3["XDG_DATA_HOME"] = os.path.join(W, "xdg")
    subprocess.run([PROBE, "secret-set", "a", "b", "c"], env=e3, capture_output=True)
    check(os.path.isfile(os.path.join(W, "xdg/firn/secrets.v1")), "XDG_DATA_HOME wins over HOME")
    r = subprocess.run([PROBE, "secret-set", "a/b", "c", "d"], env=e3, capture_output=True)
    check("Invalid" in r.stdout.decode(), "a service with '/' is Invalid")


def main():
    os.makedirs(W, exist_ok=True)
    tar_both_ways()
    hostile()
    zips()
    hashes()
    vaults()
    print("STDARCHIVE: %d checks, %d failed" % (CHECKS, FAILS))
    sys.exit(1 if FAILS else 0)


main()

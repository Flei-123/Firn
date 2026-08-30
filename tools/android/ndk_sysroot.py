#!/usr/bin/env python3
"""tools/android/ndk_sysroot.py -- the 17 MB of the NDK that Firn needs.

ROUND ANDROID.

The Android NDK is a 633 MB download that unpacks to about 2.6 GB. Firn uses
none of the 2.6 GB: it brings its own code generator and links with
binutils-aarch64-linux-gnu. What it needs is the aarch64 Bionic SYSROOT of
one API level -- the stub libraries (libc.so, libm.so, libdl.so, liblog.so)
and the start files (crtbegin_dynamic.o, crtend_android.o, crtbegin_so.o,
crtend_so.o). That is about 8 MB per level.

On the machine this round was done on there were 3.6 GB free and eleven
other builds running, so unpacking 2.6 GB was not acceptable. This script
takes the members out of the zip WITHOUT downloading it: a zip carries its
directory at the END, the server serves byte ranges, and a deflate member
can be fetched and inflated on its own. Downloaded: a few megabytes.

If a full NDK is installed anyway (sdkmanager, ANDROID_NDK_HOME), this
script is not needed -- the compiler finds either the same way
(compiler/src/android.rs).

Usage:

    python3 tools/android/ndk_sysroot.py <zip url> <destination> [regex ...]

Example (what round ANDROID ran):

    python3 tools/android/ndk_sysroot.py \\
        https://dl.google.com/android/repository/android-ndk-r27c-linux.zip \\
        "$HOME/android-sdk/ndk-partial" \\
        'sysroot/usr/lib/aarch64-linux-android/(24|26)/' \\
        'sysroot/usr/lib/aarch64-linux-android/[^/]+\\.(so|a)$'

    -> $HOME/android-sdk/ndk-partial/android-ndk-r27c/...  (17 MB)

The compiler looks in `<sdk>/ndk-partial/<version>` on purpose, so a sysroot
unpacked this way is found without any environment variable.
"""
import os
import re
import struct
import sys
import urllib.request
import zlib

if len(sys.argv) < 3:
    sys.exit(__doc__)

URL = sys.argv[1]
DEST = sys.argv[2]
PATTERNS = [re.compile(p) for p in sys.argv[3:]] or [re.compile(".")]


def get(start, end):
    """One byte range out of the archive."""
    req = urllib.request.Request(URL, headers={"Range": f"bytes={start}-{end}"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def total_size():
    req = urllib.request.Request(URL, method="HEAD")
    with urllib.request.urlopen(req, timeout=60) as r:
        # Google's download server answers a HEAD with the identity length in
        # a header of its own; Content-Length is the transfer length.
        n = r.headers.get("x-identity-content-length") or r.headers["Content-Length"]
        return int(n)


total = total_size()
print(f"zip: {total} octets", flush=True)

# --- the central directory, out of the last 70 KiB
tail = get(max(0, total - 70000), total - 1)
i = tail.rfind(b"PK\x05\x06")
if i < 0:
    sys.exit("no end-of-central-directory record found")
entries = struct.unpack("<H", tail[i + 10 : i + 12])[0]
cd_size, cd_off = struct.unpack("<II", tail[i + 12 : i + 20])
j = tail.rfind(b"PK\x06\x06")
if j >= 0:  # zip64 -- an NDK has more than 65535 members
    entries, cd_size, cd_off = struct.unpack("<QQQ", tail[j + 32 : j + 56])
print(f"directory: {entries} members, {cd_size} octets at {cd_off}", flush=True)
cd = get(cd_off, cd_off + cd_size - 1)

# --- pick the members
wanted = []
p = 0
while p < len(cd) - 4 and cd[p : p + 4] == b"PK\x01\x02":
    (_v, _vn, _fl, method, _t, _d, _crc, csize, usize, nlen, elen, clen,
     _dk, _ia, eattr, lho) = struct.unpack("<HHHHHHIIIHHHHHII", cd[p + 4 : p + 46])
    name = cd[p + 46 : p + 46 + nlen].decode("utf-8", "replace")
    extra = cd[p + 46 + nlen : p + 46 + nlen + elen]
    if 0xFFFFFFFF in (usize, csize, lho):  # zip64 extra field
        q = 0
        while q + 4 <= len(extra):
            hid, hsz = struct.unpack("<HH", extra[q : q + 4])
            if hid == 0x0001:
                d = extra[q + 4 : q + 4 + hsz]
                k = 0
                if usize == 0xFFFFFFFF:
                    usize = struct.unpack("<Q", d[k : k + 8])[0]; k += 8
                if csize == 0xFFFFFFFF:
                    csize = struct.unpack("<Q", d[k : k + 8])[0]; k += 8
                if lho == 0xFFFFFFFF:
                    lho = struct.unpack("<Q", d[k : k + 8])[0]; k += 8
            q += 4 + hsz
    if any(r.search(name) for r in PATTERNS):
        wanted.append((name, method, csize, usize, lho, eattr))
    p += 46 + nlen + elen + clen

print(f"matched: {len(wanted)} members, {sum(w[2] for w in wanted)} octets compressed",
      flush=True)
if not wanted:
    sys.exit("nothing matched -- check the patterns")

# --- fetch and inflate each of them
for name, method, csize, usize, lho, eattr in wanted:
    if name.endswith("/"):
        continue
    hdr = get(lho, lho + 29)
    if hdr[:4] != b"PK\x03\x04":
        sys.exit(f"bad local header for {name}")
    nlen, elen = struct.unpack("<HH", hdr[26:30])
    off = lho + 30 + nlen + elen
    raw = get(off, off + csize - 1) if csize else b""
    if method == 0:
        blob = raw
    elif method == 8:
        blob = zlib.decompressobj(-15).decompress(raw)
    else:
        sys.exit(f"unsupported compression {method} for {name}")
    if len(blob) != usize:
        sys.exit(f"size mismatch for {name}: {len(blob)} != {usize}")
    out = os.path.join(DEST, name)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    mode = (eattr >> 16) & 0xFFFF
    if mode & 0o170000 == 0o120000:  # a symbolic link
        target = blob.decode()
        if os.path.lexists(out):
            os.remove(out)
        os.symlink(target, out)
        print(f"  link {name} -> {target}", flush=True)
        continue
    with open(out, "wb") as f:
        f.write(blob)
    if mode & 0o111:
        os.chmod(out, 0o755)
    print(f"  {len(blob):>9}  {name}", flush=True)
print("done", flush=True)

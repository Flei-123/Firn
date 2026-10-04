# SPDX-License-Identifier: MPL-2.0
"""OrientOS: a store package (.opk) from a program.

The format is OrientOS's own (docs/PAKETE.md in the OrientOS tree, written
by pkg/opk.py and read by orientstore/werkzeug/opkleser.py):

    0   8   "OPKG0001"
    8   8   length of the metadata    (u64 LE)
    16  8   length of the data        (u64)
    24  32  SHA-256 over (metadata || data)   <- the content hash
    56  8   zero
    64      metadata: key=value lines, name fassung titel info keys, then
            braucht= and handle= (sorted), then further keys sorted
            (here: arch=)
            data: a DETERMINISTIC archive, no tar:
              per entry: 'd'|'f', mode u16, name length u16, name (UTF-8,
              no leading slash), content length u64, content
              sorted by the octets of the name; mode 0755 for `start`,
              0644 for everything else (the host's umask must not change the hash)

An OrientOS program is `start` (an ELF built for OrientOS), `INFO`
(name/info/keys/fassung -- what the launcher lists), an optional `symbol`
(OSYM: "OSYM", width u32, height u32, BGRA pixels) and `data/...`.
Written from the format document; the same bytes come out as from
`pkg/opk.py bauen` (tests/2207 compares with it where the OrientOS tree is
there). NOT run on OrientOS itself.
"""

import hashlib
import struct
import zlib

from . import common
from .common import PackError

FELDER = ("name", "fassung", "titel", "info", "keys")
RESERVED = ("app", "kernel", "source", "setting", "account", "pref", "arch")


def meta_bytes(fields, braucht=(), handles=()):
    lines = ["%s=%s" % (f, fields.get(f, "")) for f in FELDER]
    lines += ["braucht=%s" % b for b in sorted(braucht)]
    lines += ["handle=%s" % h for h in sorted(handles)]
    for k in sorted(fields):
        if k in FELDER or k in ("braucht", "handle"):
            continue
        lines.append("%s=%s" % (k, fields[k]))
    return ("\n".join(lines) + "\n").encode("utf-8")


def archive_bytes(files):
    """files: dict name -> bytes (directories are made from the names)."""
    entries = {}
    for name, data in files.items():
        parts = name.split("/")
        for i in range(1, len(parts)):
            entries["/".join(parts[:i])] = None
        entries[name] = data
    out = bytearray()
    for name in sorted(entries, key=lambda n: n.encode("utf-8")):
        nb = name.encode("utf-8")
        data = entries[name]
        if data is None:
            out += b"d" + struct.pack("<HH", 0o755, len(nb)) + nb + struct.pack("<Q", 0)
        else:
            mode = 0o755 if name.split("/")[-1] == "start" else 0o644
            out += b"f" + struct.pack("<HH", mode, len(nb)) + nb + struct.pack("<Q", len(data)) + data
    return bytes(out)


def pack(meta, data):
    h = hashlib.sha256(meta + data).digest()
    head = bytearray(64)
    head[0:8] = b"OPKG0001"
    struct.pack_into("<QQ", head, 8, len(meta), len(data))
    head[24:56] = h
    return bytes(head) + meta + data, h.hex()


def png_rgba(data):
    """Decode a non-interlaced 8-bit RGB/RGBA PNG (what tools/pack/icons.fi writes)."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise PackError("not a PNG")
    pos, idat, w = 8, b"", None
    while pos < len(data):
        n, = struct.unpack_from(">I", data, pos)
        tag = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + n]
        if tag == b"IHDR":
            w, h, depth, ctype, _, _, inter = struct.unpack(">IIBBBBB", body)
            if depth != 8 or ctype not in (2, 6) or inter:
                raise PackError("only 8-bit RGB/RGBA PNG without interlace is read here")
        elif tag == b"IDAT":
            idat += body
        pos += 12 + n
    bpp = 4 if ctype == 6 else 3
    raw = zlib.decompress(idat)
    stride = w * bpp
    rows, prev = [], bytearray(stride)
    p = 0
    for _ in range(h):
        f = raw[p]
        cur = bytearray(raw[p + 1:p + 1 + stride])
        p += 1 + stride
        for i in range(stride):
            a = cur[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                cur[i] = (cur[i] + a) & 255
            elif f == 2:
                cur[i] = (cur[i] + b) & 255
            elif f == 3:
                cur[i] = (cur[i] + ((a + b) >> 1)) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                cur[i] = (cur[i] + pr) & 255
        rows.append(bytes(cur))
        prev = cur
    out = bytearray()
    for r in rows:
        for x in range(w):
            px = r[x * bpp:x * bpp + bpp]
            out += bytes(px) if bpp == 4 else bytes(px) + b"\xff"
    return w, h, bytes(out)


def osym(png_bytes, size=32):
    """A PNG as an OSYM symbol of size x size (box filter), BGRA."""
    w, h, px = png_rgba(png_bytes)
    out = bytearray(b"OSYM" + struct.pack("<II", size, size))
    for y in range(size):
        for x in range(size):
            x0, x1 = x * w // size, max((x + 1) * w // size, x * w // size + 1)
            y0, y1 = y * h // size, max((y + 1) * h // size, y * h // size + 1)
            r = g = b = a = n = 0
            for yy in range(y0, min(y1, h)):
                for xx in range(x0, min(x1, w)):
                    o = (yy * w + xx) * 4
                    pa = px[o + 3]
                    r += px[o] * pa
                    g += px[o + 1] * pa
                    b += px[o + 2] * pa
                    a += pa
                    n += 1
            if a:
                out += bytes([b // a, g // a, r // a, a // n])
            else:
                out += b"\0\0\0\0"
    return bytes(out)


def opk(app, start_path, out, icon_png=None, handles=("config", "state", "cache", "console"),
        braucht=(), extra_dir=None, keys=None, arch=None):
    if app.id in RESERVED:
        raise PackError("'%s' is a PLAN type and cannot be a package name" % app.id)
    start = common.read(start_path)
    m = common.elf_machine(start)
    if m is None:
        raise PackError("%s is not an ELF file (an OrientOS program is one)" % start_path)
    machine = {62: "x86_64", 183: "aarch64"}.get(m)
    if machine is None:
        raise PackError("ELF machine %d: only x86-64 and aarch64 are known" % m)
    if arch and arch != machine:
        raise PackError("--arch %s but the program is %s" % (arch, machine))
    files = {"start": start}
    info = "name=%s\ninfo=%s\nkeys=%s\nfassung=%s\n" % (app.name, app.summary or app.name,
                                                           keys or app.id, app.version.split(".")[0])
    files["INFO"] = info.encode("utf-8")
    if icon_png:
        files["symbol"] = osym(common.read(icon_png))
    if extra_dir:
        for rel, p, isdir in common.walk_files(extra_dir):
            if not isdir:
                files["data/" + rel] = common.read(p)
    fields = {"name": app.id, "fassung": app.version, "titel": app.name,
              "info": app.summary or app.name, "keys": keys or app.id, "arch": machine}
    blob, h = pack(meta_bytes(fields, braucht, handles), archive_bytes(files))
    common.write(out, blob)
    return out, h

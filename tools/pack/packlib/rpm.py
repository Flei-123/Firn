# SPDX-License-Identifier: MPL-2.0
"""A .rpm written from the file format (no rpmbuild): lead, signature header,
header, gzip'd cpio payload.

What an RPM is (rpm.org's "File Format" document, as read by rpm 4.x):

    lead (96)            magic 0xedabeedb, version 3.0, type binary, name, signature type 5
    signature header     magic 8eade801, index entries, data: SIZE, MD5, SHA1/SHA256 of the
                         header, PAYLOADSIZE -- padded to 8 octets
    header               the same container: name/version/release/arch/..., the file list as
                         parallel arrays (sizes, modes, digests, directory indexes, basenames,
                         directories), payload format `cpio`, compressor `gzip`
    payload              a cpio archive (new ASCII format "070701"), names "./usr/bin/x"

The tags written are the ones rpm needs to install, query and verify the package
(`rpm -qip`, `-qlp`, `-K`, `-i`); the header carries the region tag (62 / 63), so rpm treats it as one unit;
the package is **not signed with a key** (`rpm -K` says `digests OK`).
Files only (directories are implied by the path); owner root:root, one directory index per distinct directory.
"""

import gzip
import hashlib
import io
import struct

from . import common
from .common import PackError

# tag types
INT16, INT32, STRING, BIN, STRING_ARRAY, I18NSTRING = 3, 4, 6, 7, 8, 9

RPM_ARCH = {"x86_64": "x86_64", "aarch64": "aarch64"}


def _header(entries, region=None):
    """entries: list of (tag, type, value). Returns the header structure bytes
    (magic, counts, index, store). `region` (62 signature / 63 immutable) adds the
    region tag: an entry whose data is a trailer at the END of the store that
    points back at the whole index -- it marks the header as one unit, which rpm
    4.x wants (without it rpm calls the package "v3")."""
    entries = sorted(entries, key=lambda e: e[0])
    store = bytearray()
    index = bytearray()
    for tag, typ, val in entries:
        if typ == INT16:
            vals = val if isinstance(val, list) else [val]
            while len(store) % 2:
                store.append(0)
            off = len(store)
            for v in vals:
                store += struct.pack(">H", v)
            count = len(vals)
        elif typ == INT32:
            vals = val if isinstance(val, list) else [val]
            while len(store) % 4:
                store.append(0)
            off = len(store)
            for v in vals:
                store += struct.pack(">I", v & 0xFFFFFFFF)
            count = len(vals)
        elif typ in (STRING, I18NSTRING):
            off = len(store)
            store += val.encode("utf-8") + b"\0"
            count = 1
        elif typ == STRING_ARRAY:
            off = len(store)
            for v in val:
                store += v.encode("utf-8") + b"\0"
            count = len(val)
        elif typ == BIN:
            off = len(store)
            store += val
            count = len(val)
        else:
            raise PackError("unknown RPM tag type %d" % typ)
        index += struct.pack(">IIII", tag, typ, off, count)
    if region:
        n = len(entries) + 1
        rindex = struct.pack(">IIII", region, BIN, len(store), 16)
        store += struct.pack(">IIiI", region, BIN, -(n * 16), 16)
        index = bytearray(rindex) + index
        return (b"\x8e\xad\xe8\x01\0\0\0\0" + struct.pack(">II", n, len(store)) + bytes(index) + bytes(store))
    return b"\x8e\xad\xe8\x01\0\0\0\0" + struct.pack(">II", len(entries), len(store)) + bytes(index) + bytes(store)


def cpio_newc(files):
    """files: (name like './usr/bin/x', data, mode, inode). New ASCII cpio archive."""
    out = bytearray()
    for name, data, mode, ino in files:
        nb = name.encode("utf-8") + b"\0"
        hdr = "070701%08X%08X%08X%08X%08X%08X%08X%08X%08X%08X%08X%08X%08X" % (
            ino, mode, 0, 0, 1, common.epoch(), len(data), 0, 0, 0, 0, len(nb), 0)
        out += hdr.encode("ascii") + nb
        out += b"\0" * (-len(out) % 4)
        out += data
        out += b"\0" * (-len(out) % 4)
    nb = b"TRAILER!!!\0"
    out += ("070701" + "00000000" * 4 + "00000001" + "00000000" * 6 + "%08X" % len(nb) + "00000000").encode("ascii") + nb
    out += b"\0" * (-len(out) % 4)
    return bytes(out)


def rpm(app, exe_path, out, icons_dir=None, arch=None, release="1", requires=None):
    from . import linux
    exe = common.read(exe_path)
    a = arch or linux.arch_of(app, exe)
    version = app.version.replace("-", "~")          # 1.2.3-beta.1 sorts before 1.2.3 as 1.2.3~beta.1
    files = [("/usr/bin/%s" % app.id, exe, 0o100755),
             ("/usr/share/applications/%s.desktop" % app.id, linux.desktop_entry(app, "/usr/bin/%s" % app.id), 0o100644)]
    for rel, data in linux.icon_files(icons_dir, app):
        files.append(("/usr/share/icons/" + rel, data, 0o100644))
    files.sort(key=lambda f: f[0])
    dirs, dirindex, basenames = [], [], []
    for path, _, _ in files:
        d, _, b = path.rpartition("/")
        d += "/"
        if d not in dirs:
            dirs.append(d)
        dirindex.append(dirs.index(d))
        basenames.append(b)
    payload = gzip.compress(cpio_newc([("." + p, d, m, i + 1) for i, (p, d, m) in enumerate(files)]),
                            compresslevel=9, mtime=0)
    raw_size = sum(len(f[1]) for f in files)
    desc = (app.description or app.summary or app.name).replace("\\n", "\n")
    req_names = ["rpmlib(CompressedFileNames)", "rpmlib(PayloadFilesHavePrefix)"]
    req_ver = ["3.0.4-1", "4.0-1"]
    req_flags = [0x1000000 | 0x4 | 0x8, 0x1000000 | 0x4 | 0x8]
    for r in (requires or []):
        req_names.append(r)
        req_ver.append("")
        req_flags.append(0)
    hdr = _header([
        (100, STRING_ARRAY, ["C"]),
        (1000, STRING, app.id), (1001, STRING, version), (1002, STRING, release),
        (1004, I18NSTRING, app.summary or app.name), (1005, I18NSTRING, desc),
        (1006, INT32, common.epoch()), (1007, STRING, "pack.fleitec.invalid"),
        (1009, INT32, raw_size), (1014, STRING, app.license), (1016, I18NSTRING, "Unspecified"),
        (1021, STRING, "linux"), (1022, STRING, RPM_ARCH.get(a, a)),
        (1028, INT32, [len(f[1]) for f in files]),
        (1030, INT16, [f[2] & 0xFFFF for f in files]),
        (1033, INT16, [0] * len(files)),
        (1034, INT32, [common.epoch()] * len(files)),
        (1035, STRING_ARRAY, [hashlib.sha256(f[1]).hexdigest() for f in files]),
        (1036, STRING_ARRAY, [""] * len(files)),
        (1037, INT32, [0] * len(files)),
        (1039, STRING_ARRAY, ["root"] * len(files)),
        (1040, STRING_ARRAY, ["root"] * len(files)),
        (1048, INT32, req_flags), (1049, STRING_ARRAY, req_names), (1050, STRING_ARRAY, req_ver),
        (1064, STRING, "4.0.4"),
        (1095, INT32, [1] * len(files)),
        (1096, INT32, list(range(1, len(files) + 1))),
        (1097, STRING_ARRAY, [""] * len(files)),
        (1116, INT32, dirindex), (1117, STRING_ARRAY, basenames), (1118, STRING_ARRAY, dirs),
        (1124, STRING, "cpio"), (1125, STRING, "gzip"), (1126, STRING, "9"),
        (5011, INT32, 8),                                  # FILEDIGESTALGO: SHA-256
    ], region=63)
    body = hdr + payload
    sig = _header([
        (1000, INT32, len(body)),
        (1004, BIN, hashlib.md5(body).digest()),
        (269, STRING, hashlib.sha1(hdr).hexdigest()),
        (273, STRING, hashlib.sha256(hdr).hexdigest()),
        (1007, INT32, raw_size),
    ], region=62)
    sig += b"\0" * (-len(sig) % 8)
    name = ("%s-%s-%s" % (app.id, version, release))[:65]
    lead = struct.pack(">IBBhh66shh16s", 0xedabeedb, 3, 0, 0, 1 if a == "x86_64" else 12,
                       name.encode("ascii", "replace"), 1, 5, b"\0" * 16)
    assert len(lead) == 96, len(lead)
    common.write(out, lead + sig + body)
    return out

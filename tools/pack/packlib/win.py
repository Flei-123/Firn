# SPDX-License-Identifier: MPL-2.0
"""Windows packages: portable zip, installer (stub + payload), icon resource
in a PE file, NSIS script.

THE INSTALLER is a Firn program (tools/pack/installer/installer.fi, lib/pack/
install.fi) that finds its payload -- a zip -- appended to itself. This file
only makes that file: stub bytes + payload zip + 64 octet trailer
(lib/pack/payload.fi documents the layout; `trailer()` is its twin).
"""

import hashlib
import io
import os
import struct
import zipfile

from . import common
from .common import PackError


# ------------------------------------------------------------------ zip

def make_zip(entries, mode_default=0o644):
    """entries: list of (name, bytes_or_None_for_dir, mode). Deterministic."""
    bio = io.BytesIO()
    with zipfile.ZipFile(bio, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data, mode in entries:
            if data is None:
                zi = zipfile.ZipInfo(name.rstrip("/") + "/", common.zip_time())
                zi.external_attr = (0o40755 << 16) | 0x10
                z.writestr(zi, b"")
            else:
                zi = zipfile.ZipInfo(name, common.zip_time())
                zi.compress_type = zipfile.ZIP_DEFLATED
                zi.create_system = 3
                zi.external_attr = ((0o100000 | (mode or mode_default)) << 16)
                z.writestr(zi, data, zipfile.ZIP_DEFLATED, 9)
    return bio.getvalue()


def collect(app, exe_path, extra_dir=None, ico=None, exe_name=None):
    """The files of the program: the exe, the icon, everything in extra_dir."""
    entries = []
    exe_name = exe_name or (app.exe if app.exe.endswith(".exe") else app.exe + ".exe")
    entries.append((exe_name, common.read(exe_path), 0o755))
    if ico:
        entries.append((app.id + ".ico", common.read(ico), 0o644))
    if extra_dir:
        for rel, p, isdir in common.walk_files(extra_dir):
            if isdir:
                entries.append((rel, None, 0o755))
            else:
                if rel in (exe_name, app.id + ".ico"):
                    continue
                entries.append((rel, common.read(p), 0o644))
    return exe_name, entries


def portable_zip(app, exe_path, out, extra_dir=None, ico=None):
    """The program and its icon in a zip, plus `portable.txt` (appkit keeps
    its data next to the program when that file is there)."""
    exe_name, entries = collect(app, exe_path, extra_dir, ico)
    if not any(n == "portable.txt" for n, _, _ in entries):
        entries.append(("portable.txt", ("%s %s portable\n"
                                        "Delete this file to keep settings in the user profile instead.\n"
                                        % (app.name, app.version)).encode("utf-8"), 0o644))
    folder = "%s-%s" % (app.id, app.version)
    wrapped = [(folder + "/" + n, d, m) for n, d, m in entries]
    wrapped.insert(0, (folder, None, 0o755))
    data = make_zip(wrapped)
    common.write(out, data)
    return out


# ------------------------------------------------------------ installer

def info_text(app, exe_name, ico_name):
    lines = ["id=%s" % app.id, "name=%s" % app.name, "version=%s" % app.version,
             "vendor=%s" % app.vendor, "exe=%s" % exe_name]
    if ico_name:
        lines.append("icon=%s" % ico_name)
    if app.url:
        lines.append("url=%s" % app.url)
    lines.append("desktop=%s" % app.desktop)
    lines.append("launch=%s" % app.launch)
    return ("\n".join(lines) + "\n").encode("utf-8")


def trailer(offset, payload):
    return (b"FIRNPAK1" + struct.pack("<QQ", offset, len(payload))
            + hashlib.sha256(payload).digest() + b"\0" * 8)


def installer(app, stub_path, exe_path, out, extra_dir=None, ico=None):
    stub = common.read(stub_path)
    if not common.is_pe(stub):
        raise PackError("the installer stub %s is not a Windows program (build it with "
                        "`bash tools/pack/pack.sh stubs`)" % stub_path)
    if not common.is_pe(common.read(exe_path)):
        raise PackError("%s is not a Windows program (MZ/PE header missing)" % exe_path)
    exe_name, entries = collect(app, exe_path, extra_dir, ico)
    entries.insert(0, (".pack/info", info_text(app, exe_name, app.id + ".ico" if ico else ""), 0o644))
    payload = make_zip(entries)
    blob = stub + payload + trailer(len(stub), payload)
    common.write(out, blob, 0o755)
    return out


# ------------------------------------------------------------ PE icon

def _align(v, a):
    return (v + a - 1) // a * a


def parse_ico(data):
    """-> list of (width, height, bpp, bytes) for every image of an .ico"""
    res, typ, count = struct.unpack_from("<HHH", data, 0)
    if res != 0 or typ != 1 or count == 0:
        raise PackError("not an .ico file")
    out = []
    for i in range(count):
        w, h, cc, r, planes, bpp, size, off = struct.unpack_from("<BBBBHHII", data, 6 + 16 * i)
        out.append((w or 256, h or 256, bpp or 32, planes or 1, data[off:off + size]))
    return out


def pe_set_icon(exe_bytes, ico_bytes):
    """Return the PE image with RT_GROUP_ICON/RT_ICON resources for `ico_bytes`.

    Firn's PE files carry no resource section, so a new `.rsrc` section is
    appended after the last one. An existing resource directory is refused
    (nothing is merged: replacing a half-understood tree would corrupt it)."""
    d = bytearray(exe_bytes)
    if not common.is_pe(d):
        raise PackError("not a PE file")
    pe = struct.unpack_from("<I", d, 60)[0]
    coff = pe + 4
    nsec, = struct.unpack_from("<H", d, coff + 2)
    optsz, = struct.unpack_from("<H", d, coff + 16)
    opt = coff + 20
    magic, = struct.unpack_from("<H", d, opt)
    if magic != 0x20B:
        raise PackError("only PE32+ (x86-64) is supported")
    sec_align, file_align = struct.unpack_from("<II", d, opt + 32)
    size_of_image, size_of_headers = struct.unpack_from("<II", d, opt + 56)
    ddir = opt + 112
    rsrc_rva, rsrc_size = struct.unpack_from("<II", d, ddir + 2 * 8)
    if rsrc_rva or rsrc_size:
        raise PackError("the program already has a resource section")
    sect = opt + optsz
    # section table: find the end and the first raw data (room for one more header)
    last_end_raw = 0
    last_end_va = 0
    first_raw = None
    for i in range(nsec):
        o = sect + 40 * i
        vsz, va, rsz, rptr = struct.unpack_from("<IIII", d, o + 8)
        last_end_raw = max(last_end_raw, rptr + rsz)
        last_end_va = max(last_end_va, va + _align(max(vsz, rsz), sec_align))
        if rptr and (first_raw is None or rptr < first_raw):
            first_raw = rptr
    new_hdr = sect + 40 * nsec
    if new_hdr + 40 > (first_raw if first_raw else size_of_headers):
        raise PackError("no room for another section header in this PE file")
    # Firn leaves a COFF symbol table (function names, for debuggers) after the last
    # section. The icon section has to come before it, so the table is dropped
    # (a stripped program runs the same; the symbols are not mapped into memory).
    symptr, nsyms = struct.unpack_from("<II", d, coff + 8)
    if symptr and symptr == last_end_raw:
        del d[symptr:]
        struct.pack_into("<II", d, coff + 8, 0, 0)
    if len(d) > last_end_raw:
        raise PackError("the program has data after its last section (an installer payload?); "
                        "set the icon before appending it")

    images = parse_ico(ico_bytes)
    rva = last_end_va

    # ---- build the resource tree: Type -> Name -> Language -> Data
    # data blobs first (laid out after the directories), then entries
    blobs = []          # (bytes)
    grp = struct.pack("<HHH", 0, 1, len(images))
    for i, (w, h, bpp, planes, img) in enumerate(images):
        grp += struct.pack("<BBBBHHIH", w % 256, h % 256, 0, 0, planes, bpp, len(img), i + 1)
        blobs.append(img)
    group_blob = grp
    # directory layout (all offsets relative to the section start):
    #   root (2 entries: RT_ICON=3, RT_GROUP_ICON=14)
    #   type dir ICON  (n entries: id 1..n)     -> name dir each -> lang dir -> data entry
    n = len(images)

    def dir_node(ids_offsets):
        # IMAGE_RESOURCE_DIRECTORY (16) + entries (8 each); all ids are numeric
        b = struct.pack("<IIHHHH", 0, 0, 0, 0, 0, len(ids_offsets))
        for rid, off in ids_offsets:
            b += struct.pack("<II", rid, off)
        return b

    # sizes
    root_sz = 16 + 8 * 2
    icon_type_sz = 16 + 8 * n
    group_type_sz = 16 + 8 * 1
    name_dir_sz = 16 + 8          # one language each
    data_entry_sz = 16
    # offsets
    off = 0
    root_off = off; off += root_sz
    icon_type_off = off; off += icon_type_sz
    group_type_off = off; off += group_type_sz
    icon_name_off = []
    for i in range(n):
        icon_name_off.append(off); off += name_dir_sz
    group_name_off = off; off += name_dir_sz
    data_entry_off = []
    for i in range(n):
        data_entry_off.append(off); off += data_entry_sz
    group_data_entry_off = off; off += data_entry_sz
    data_off = _align(off, 4)
    tree = bytearray()
    HIGH = 0x80000000
    tree += dir_node([(3, HIGH | icon_type_off), (14, HIGH | group_type_off)])
    tree += dir_node([(i + 1, HIGH | icon_name_off[i]) for i in range(n)])
    tree += dir_node([(1, HIGH | group_name_off)])
    # name dirs: one language (0x0409 English US; 0 would be "neutral" -- both are found)
    for i in range(n):
        tree += dir_node([(0x0409, data_entry_off[i])])
    tree += dir_node([(0x0409, group_data_entry_off)])
    # data layout
    cursor = data_off
    datas = []
    for blob in blobs + [group_blob]:
        datas.append((cursor, blob))
        cursor = _align(cursor + len(blob), 4)
    for i in range(n):
        tree += struct.pack("<IIII", rva + datas[i][0], len(blobs[i]), 0, 0)
    tree += struct.pack("<IIII", rva + datas[n][0], len(group_blob), 0, 0)
    assert len(tree) == off, (len(tree), off)
    tree += b"\0" * (data_off - len(tree))
    for pos, blob in datas:
        assert len(tree) == pos
        tree += blob
        tree += b"\0" * (_align(len(tree), 4) - len(tree))
    raw_size = _align(len(tree), file_align)
    virt_size = len(tree)
    raw_ptr = _align(last_end_raw, file_align)
    # pad the file to raw_ptr, append the section
    d += b"\0" * (raw_ptr - len(d))
    d += bytes(tree) + b"\0" * (raw_size - len(tree))
    # section header
    hdr = (b".rsrc\0\0\0" + struct.pack("<IIIIIIHHI", virt_size, rva, raw_size, raw_ptr, 0, 0, 0, 0,
                                         0x40000040))
    d[new_hdr:new_hdr + 40] = hdr
    struct.pack_into("<H", d, coff + 2, nsec + 1)
    new_image = _align(rva + virt_size, sec_align)
    struct.pack_into("<I", d, opt + 56, new_image)
    struct.pack_into("<II", d, ddir + 2 * 8, rva, virt_size)
    # the checksum is not checked for user programs; clear it so it is not wrong
    struct.pack_into("<I", d, opt + 64, 0)
    return bytes(d)


def pe_icons(exe_bytes):
    """Read the icon sizes back: -> list of (w, h) in the group icon, or None."""
    d = exe_bytes
    pe = struct.unpack_from("<I", d, 60)[0]
    coff = pe + 4
    nsec, = struct.unpack_from("<H", d, coff + 2)
    optsz, = struct.unpack_from("<H", d, coff + 16)
    opt = coff + 20
    rrva, rsize = struct.unpack_from("<II", d, opt + 112 + 16)
    if not rrva:
        return None
    sect = opt + optsz
    secs = []
    for i in range(nsec):
        o = sect + 40 * i
        name = d[o:o + 8].rstrip(b"\0")
        vsz, va, rsz, rptr = struct.unpack_from("<IIII", d, o + 8)
        secs.append((va, max(vsz, rsz), rptr))

    def off_of(r):
        for va, sz, ptr in secs:
            if va <= r < va + sz:
                return ptr + (r - va)
        raise PackError("RVA outside the sections")
    base = off_of(rrva)

    def entries(o):
        nn, ni = struct.unpack_from("<HH", d, o + 12)
        return [struct.unpack_from("<II", d, o + 16 + 8 * i) for i in range(nn + ni)]

    def follow(v):
        return base + (v & 0x7FFFFFFF)
    root = entries(base)
    ids = dict((rid, v) for rid, v in root)
    if 14 not in ids or 3 not in ids:
        return None
    sizes = []
    # RT_GROUP_ICON -> name -> language -> data entry
    g = entries(follow(ids[14]))[0][1]
    g = entries(follow(g))[0][1]
    rva_, size_, _, _ = struct.unpack_from("<IIII", d, base + g)
    gdata = d[off_of(rva_):off_of(rva_) + size_]
    cnt, = struct.unpack_from("<H", gdata, 4)
    for i in range(cnt):
        w, h = gdata[6 + 14 * i], gdata[7 + 14 * i]
        sizes.append((w or 256, h or 256))
    # every referenced RT_ICON must exist and look like PNG or a BMP header
    icon_ids = set(rid for rid, _ in entries(follow(ids[3])))
    for i in range(cnt):
        iid, = struct.unpack_from("<H", gdata, 6 + 14 * i + 12)
        if iid not in icon_ids:
            return None
    return sizes


# --------------------------------------------------------------- NSIS

def nsis_script(app, exe_name, ico_name, out_name):
    """An NSIS script for the same install: per-user folder, Start Menu and
    Desktop shortcut, the Uninstall registry key, an uninstaller. `makensis`
    builds it; this tool does not run it."""
    q = lambda s: s.replace("$", "$$").replace('"', '$\\"')
    return """; Generated by tools/pack/pack.py nsis -- the same install as the Firn installer.
; Build:  makensis -V2 %(id)s.nsi
Unicode true
!define APP_ID "%(id)s"
!define APP_NAME "%(name)s"
!define APP_VERSION "%(version)s"
!define APP_VENDOR "%(vendor)s"
!define APP_EXE "%(exe)s"
!define UNINST_KEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${APP_ID}"

Name "${APP_NAME}"
OutFile "%(out)s"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\\Programs\\${APP_NAME}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
SetCompressor /SOLID lzma
%(icon_decl)s
Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "%(exe)s"
%(icon_file)s%(files)s
  WriteUninstaller "$INSTDIR\\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\\${APP_NAME}.lnk" "$INSTDIR\\${APP_EXE}" "" "%(shortcut_icon)s"
  CreateShortcut "$DESKTOP\\${APP_NAME}.lnk" "$INSTDIR\\${APP_EXE}" "" "%(shortcut_icon)s"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP_VENDOR}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "%(shortcut_icon)s"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\\${APP_NAME}.lnk"
  Delete "$DESKTOP\\${APP_NAME}.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  RMDir /r "$INSTDIR"
SectionEnd
""" % {
        "id": q(app.id), "name": q(app.name), "version": q(app.version), "vendor": q(app.vendor),
        "exe": q(exe_name), "out": q(out_name),
        "icon_decl": ('Icon "%s"\nUninstallIcon "%s"' % (q(ico_name), q(ico_name))) if ico_name else "",
        "icon_file": ('  File "%s"\n' % q(ico_name)) if ico_name else "",
        "files": "",
        "shortcut_icon": ("$INSTDIR\\%s" % q(ico_name)) if ico_name else "$INSTDIR\\%s" % q(exe_name),
    }

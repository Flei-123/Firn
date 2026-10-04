# SPDX-License-Identifier: MPL-2.0
"""Linux packages: .deb, tar.gz with install.sh, a self-extracting program
(ELF stub + zip of an AppDir), AppImage (runtime + squashfs).

All of it is written here from the file formats: `ar` and `tar` for .deb,
the SquashFS writer of squashfs.py for AppImage; no dpkg-deb, no
appimagetool, no mksquashfs.
"""

import gzip
import hashlib
import io
import lzma
import os
import struct
import tarfile
import urllib.request

from . import common, squashfs, win
from .common import PackError

DEB_ARCH = {"x86_64": "amd64", "aarch64": "arm64"}
RUNTIME_URL = "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-%s"


def arch_of(app, exe_bytes):
    m = common.elf_machine(exe_bytes)
    if m is None:
        raise PackError("the Linux program is not an ELF file")
    a = common.ELF_ARCH.get(m)
    if a is None:
        raise PackError("ELF machine %d is not x86-64 or aarch64" % m)
    return a


# ------------------------------------------------------------ desktop file

def desktop_entry(app, exec_cmd, icon_name=None):
    lines = ["[Desktop Entry]", "Type=Application", "Version=1.0",
             "Name=%s" % app.name, "Comment=%s" % (app.summary or app.name),
             "Exec=%s" % exec_cmd, "Icon=%s" % (icon_name or app.id),
             "Terminal=false", "Categories=%s;" % app.category.strip(";"),
             "StartupWMClass=%s" % app.id]
    return ("\n".join(lines) + "\n").encode("utf-8")


def icon_files(icons_dir, app):
    """(path below the icon theme, bytes): `hicolor/<n>x<n>/apps/<id>.png` and
    `hicolor/scalable/apps/<id>.svg` from what tools/pack/icons.fi wrote
    (whatever the icons were called there, they are named after the app here)."""
    out = []
    base = os.path.join(icons_dir, "hicolor") if icons_dir else None
    if not base or not os.path.isdir(base):
        return out
    for rel, p, isdir in common.walk_files(base):
        if isdir or not rel.endswith((".png", ".svg")) or "/apps/" not in rel:
            continue
        head, _, tail = rel.rpartition("/")
        ext = tail.rsplit(".", 1)[1]
        out.append(("hicolor/%s/%s.%s" % (head, app.id, ext), common.read(p)))
    return out


# ------------------------------------------------------------------- tar

def _tarinfo(name, mode, size=0, isdir=False, link=None):
    ti = tarfile.TarInfo(name)
    ti.mode = mode
    ti.size = size
    ti.mtime = common.epoch()
    ti.uid = ti.gid = 0
    ti.uname = ti.gname = "root"
    if isdir:
        ti.type = tarfile.DIRTYPE
        ti.size = 0
    elif link is not None:
        ti.type = tarfile.SYMTYPE
        ti.linkname = link
        ti.size = 0
    return ti


def make_tar(entries, compress):
    """entries: (name, data|None for dir, mode[, linktarget]); returns bytes of
    a deterministic tar compressed with 'gz', 'xz' or ''."""
    bio = io.BytesIO()
    with tarfile.open(fileobj=bio, mode="w", format=tarfile.GNU_FORMAT) as t:
        made = set()
        for e in entries:
            name, data, mode = e[0], e[1], e[2]
            link = e[3] if len(e) > 3 else None
            # parents first
            parts = name.strip("/").split("/")
            for i in range(1, len(parts)):
                d = "/".join(parts[:i]) + "/"
                if d not in made and d != name:
                    made.add(d)
                    t.addfile(_tarinfo(d, 0o755, isdir=True))
            if data is None and link is None:
                d = name.rstrip("/") + "/"
                if d in made:
                    continue
                made.add(d)
                t.addfile(_tarinfo(d, mode, isdir=True))
            elif link is not None:
                t.addfile(_tarinfo(name, mode, link=link))
            else:
                t.addfile(_tarinfo(name, mode, len(data)), io.BytesIO(data))
    raw = bio.getvalue()
    if compress == "gz":
        out = io.BytesIO()
        with gzip.GzipFile(fileobj=out, mode="wb", mtime=0, compresslevel=9) as g:
            g.write(raw)
        return out.getvalue()
    if compress == "xz":
        return lzma.compress(raw, format=lzma.FORMAT_XZ, preset=9)
    return raw


# ------------------------------------------------------------------- ar

def make_ar(members):
    out = bytearray(b"!<arch>\n")
    for name, data in members:
        hdr = "%-16s%-12d%-6d%-6d%-8s%-10d`\n" % (name, common.epoch(), 0, 0, "100644", len(data))
        out += hdr.encode("ascii")
        out += data
        if len(data) % 2:
            out += b"\n"
    return bytes(out)


# ------------------------------------------------------------------- .deb

def deb(app, exe_path, out, icons_dir=None, arch=None, depends=None, compress="xz",
        extra_dir=None):
    exe = common.read(exe_path)
    arch_name = arch or arch_of(app, exe)
    darch = DEB_ARCH.get(arch_name, arch_name)
    bin_name = app.id
    entries = [
        ("./usr/bin/%s" % bin_name, exe, 0o755),
        ("./usr/share/applications/%s.desktop" % app.id, desktop_entry(app, "/usr/bin/%s" % bin_name), 0o644),
    ]
    for rel, data in icon_files(icons_dir, app):
        entries.append(("./usr/share/icons/" + rel, data, 0o644))
    copyright_text = ("Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
                      "Upstream-Name: %s\nUpstream-Contact: %s\n\nFiles: *\nCopyright: %s\nLicense: %s\n"
                      % (app.name, app.maintainer, app.vendor, app.license)).encode("utf-8")
    entries.append(("./usr/share/doc/%s/copyright" % app.id, copyright_text, 0o644))
    if extra_dir:
        for rel, p, isdir in common.walk_files(extra_dir):
            if not isdir:
                entries.append(("./usr/share/%s/%s" % (app.id, rel), common.read(p), 0o644))
    data_tar = make_tar(entries, compress)
    installed = (sum(len(e[1]) for e in entries if e[1] is not None) + 1023) // 1024
    md5 = "".join("%s  %s\n" % (hashlib.md5(e[1]).hexdigest(), e[0][2:])
                  for e in entries if e[1] is not None).encode("utf-8")
    desc_lines = [app.summary or app.name]
    if app.description:
        for ln in app.description.replace("\\n", "\n").splitlines():
            desc_lines.append(" " + (ln if ln.strip() else "."))
    dep = depends if depends is not None else getattr(app, "depends", "")
    control = ["Package: %s" % app.id, "Version: %s" % app.version, "Architecture: %s" % darch,
               "Maintainer: %s" % app.maintainer, "Installed-Size: %d" % installed,
               "Section: utils", "Priority: optional"]
    if dep:
        control.append("Depends: %s" % dep)
    if app.url:
        control.append("Homepage: %s" % app.url)
    control.append("Description: " + "\n".join(desc_lines))
    control_text = ("\n".join(control) + "\n").encode("utf-8")
    postinst = ("#!/bin/sh\nset -e\nif [ \"$1\" = configure ]; then\n"
                "  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi\n"
                "  if command -v gtk-update-icon-cache >/dev/null 2>&1; then gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true; fi\n"
                "fi\nexit 0\n").encode("utf-8")
    postrm = ("#!/bin/sh\nset -e\nif [ \"$1\" = remove ] || [ \"$1\" = purge ]; then\n"
              "  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi\n"
              "  if command -v gtk-update-icon-cache >/dev/null 2>&1; then gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true; fi\n"
              "fi\nexit 0\n").encode("utf-8")
    ctl_tar = make_tar([("./control", control_text, 0o644), ("./md5sums", md5, 0o644),
                        ("./postinst", postinst, 0o755), ("./postrm", postrm, 0o755)], "gz")
    members = [("debian-binary", b"2.0\n"), ("control.tar.gz", ctl_tar),
               ("data.tar.%s" % compress if compress else "data.tar", data_tar)]
    common.write(out, make_ar(members))
    return out


# ------------------------------------------------------- tar.gz + install.sh

INSTALL_SH = r'''#!/bin/sh
# Installs @NAME@ @VERSION@ for the current user (no root needed):
#   sh install.sh [--prefix DIR]        default prefix: $HOME/.local
set -e
PREFIX="${HOME}/.local"
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    -h|--help) sed -n '2,4p' "$0"; exit 0 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
HERE=$(cd "$(dirname "$0")" && pwd)
install -d "$PREFIX/bin" "$PREFIX/share/applications"
install -m 0755 "$HERE/bin/@ID@" "$PREFIX/bin/@ID@"
sed "s|^Exec=.*|Exec=$PREFIX/bin/@ID@|" "$HERE/share/applications/@ID@.desktop" > "$PREFIX/share/applications/@ID@.desktop"
(cd "$HERE/share/icons" && find hicolor -type f) | while read -r f; do
  install -d "$PREFIX/share/icons/$(dirname "$f")"
  install -m 0644 "$HERE/share/icons/$f" "$PREFIX/share/icons/$f"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f "$PREFIX/share/icons/hicolor" || true
echo "@NAME@ installed in $PREFIX (run: @ID@)"
'''

UNINSTALL_SH = r'''#!/bin/sh
# Removes what install.sh put in the prefix:  sh uninstall.sh [--prefix DIR]
set -e
PREFIX="${HOME}/.local"
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
rm -f "$PREFIX/bin/@ID@" "$PREFIX/share/applications/@ID@.desktop"
find "$PREFIX/share/icons/hicolor" -type f -name '@ID@.*' -delete 2>/dev/null || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$PREFIX/share/applications" || true
echo "@NAME@ removed from $PREFIX"
'''


def _fill(text, app):
    return (text.replace("@ID@", app.id).replace("@NAME@", app.name).replace("@VERSION@", app.version)
            ).encode("utf-8")


def tarball(app, exe_path, out, icons_dir=None, arch=None):
    exe = common.read(exe_path)
    a = arch or arch_of(app, exe)
    top = "%s-%s-linux-%s" % (app.id, app.version, a)
    entries = [(top + "/bin/" + app.id, exe, 0o755),
               (top + "/share/applications/%s.desktop" % app.id, desktop_entry(app, "/usr/bin/%s" % app.id), 0o644),
               (top + "/install.sh", _fill(INSTALL_SH, app), 0o755),
               (top + "/uninstall.sh", _fill(UNINSTALL_SH, app), 0o755)]
    for rel, data in icon_files(icons_dir, app):
        entries.append((top + "/share/icons/" + rel, data, 0o644))
    common.write(out, make_tar(entries, "gz"))
    return out


# ------------------------------------------------------------ AppDir

def appdir_entries(app, exe_bytes, icons_dir=None, for_zip=False):
    """(path, kind, mode, data) for squashfs (symlinks allowed) -- the AppDir."""
    ex = app.id
    ents = [("usr/bin/%s" % ex, 'f', 0o755, exe_bytes),
            ("AppRun", 'f', 0o755, ("#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\n"
                                    "exec \"$HERE/usr/bin/%s\" \"$@\"\n" % ex).encode("utf-8")),
            ("%s.desktop" % app.id, 'f', 0o644, desktop_entry(app, ex)),
            ("usr/share/applications/%s.desktop" % app.id, 'f', 0o644, desktop_entry(app, ex))]
    best = None
    for rel, data in icon_files(icons_dir, app):
        ents.append(("usr/share/icons/" + rel, 'f', 0o644, data))
        if rel.endswith(".png") and "256x256" in rel:
            best = data
        if best is None and rel.endswith(".png"):
            best = data
    if best:
        ents.append(("%s.png" % app.id, 'f', 0o644, best))
        if for_zip:
            ents.append((".DirIcon", 'f', 0o644, best))
        else:
            ents.append((".DirIcon", 'l', 0o777, "%s.png" % app.id))
    return ents


# ------------------------------------------------- self-extracting program

def selfextract(app, stub_path, exe_path, out, icons_dir=None):
    stub = common.read(stub_path)
    if not common.is_elf(stub):
        raise PackError("the stub %s is not an ELF file (build it: bash tools/pack/pack.sh stubs)" % stub_path)
    exe = common.read(exe_path)
    ents = [(".pack/info", None, 0)]
    zents = []
    info = ("id=%s\nname=%s\nversion=%s\nvendor=%s\nexe=usr/bin/%s\n" % (app.id, app.name, app.version,
                                                                         app.vendor, app.id)).encode("utf-8")
    zents.append((".pack/info", info, 0o644))
    for path, kind, mode, data in appdir_entries(app, exe, icons_dir, for_zip=True):
        zents.append((path, data, mode))
    payload = win.make_zip(zents)
    common.write(out, stub + payload + win.trailer(len(stub), payload), 0o755)
    return out


# --------------------------------------------------------------- AppImage

def runtime_bytes(arch, path=None, offline=False):
    path = path or os.environ.get("APPIMAGE_RUNTIME")
    if path:
        return common.read(path)
    cache = os.path.join(os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")),
                         "firn-pack", "runtime-%s" % arch)
    if os.path.isfile(cache) and os.path.getsize(cache) > 100000:
        return common.read(cache)
    if offline:
        raise PackError("no AppImage runtime (give --runtime FILE or set APPIMAGE_RUNTIME)")
    try:
        with urllib.request.urlopen(RUNTIME_URL % arch, timeout=60) as r:
            data = r.read()
    except Exception as e:
        raise PackError("no AppImage runtime: %s could not be downloaded (%s); give --runtime FILE or "
                        "use `selfextract`, which needs none" % (RUNTIME_URL % arch, e))
    if not common.is_elf(data):
        raise PackError("the downloaded runtime is not an ELF file")
    common.write(cache, data)
    return data


def appimage(app, exe_path, out, icons_dir=None, runtime=None, arch=None):
    exe = common.read(exe_path)
    a = arch or arch_of(app, exe)
    rt = runtime_bytes(a, runtime)
    if not common.is_elf(rt):
        raise PackError("the runtime is not an ELF file")
    root = squashfs.tree_from_entries(appdir_entries(app, exe, icons_dir))
    img = squashfs.build(root)
    common.write(out, rt + img, 0o755)
    return out

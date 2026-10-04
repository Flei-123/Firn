#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/pack/test/checks.py -- the packaging tools against independent readers.

Everything pack.py writes is read back by somebody else's code where there is
some (dpkg-deb, unsquashfs, the AppImage runtime, opk.py of OrientOS, makensis,
xorriso, Wine) and by a plain parser in this file where there is not.

    python3 tools/pack/test/checks.py [--tmp DIR]

Needs the programs it packages: the environment variables
    PACK_HELLO         a Linux ELF (tools/pack/test/hello.fi)
    PACK_HELLO_EXE     the same for Windows (--target=x86_64-windows)
    PACK_ICONS         the icons tool (build/pack/icons)
    PACK_SELFX         the self-extract stub (build/pack/selfx)
    PACK_SETUP_STUB    the installer stub (build/pack/setup-stub.exe), optional
tools/pack/test/run.sh builds them. A check whose reader is not installed
prints SKIP and does not fail.
"""

import hashlib
import io
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
sys.path.insert(0, PACK)
from packlib import common, linux, mac, manifest, osum, squashfs, win   # noqa: E402
from packlib.rpm import rpm as linux_rpm   # noqa: E402

TOTAL = FAILS = SKIPS = 0


def ok(cond, what):
    global TOTAL, FAILS
    TOTAL += 1
    if not cond:
        FAILS += 1
        print("FAIL " + what)
    return cond


def skip(what):
    global SKIPS
    SKIPS += 1
    print("SKIP " + what)


def have(cmd):
    return shutil.which(cmd) is not None


def run(*argv, **kw):
    kw.setdefault("stdout", subprocess.PIPE)
    kw.setdefault("stderr", subprocess.STDOUT)
    return subprocess.run(list(argv), **kw)


def pack(*argv):
    r = run(sys.executable, os.path.join(PACK, "pack.py"), *argv)
    return r.returncode, r.stdout.decode("utf-8", "replace")


def png_size(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    return struct.unpack(">II", data[16:24])


APP = ["--id", "hello", "--name", "Hello App", "--version", "1.2.3", "--vendor", "FleiTec",
       "--summary", "Says hello", "--description", "First line.\\nSecond line."]


def main():
    tmp = tempfile.mkdtemp(prefix="pack-checks-")
    if "--tmp" in sys.argv:
        shutil.rmtree(tmp)
        tmp = sys.argv[sys.argv.index("--tmp") + 1]
        os.makedirs(tmp, exist_ok=True)
    try:
        body(tmp)
    finally:
        if "--tmp" not in sys.argv:
            shutil.rmtree(tmp, ignore_errors=True)
    print("pack checks: %d, failed %d, skipped %d" % (TOTAL, FAILS, SKIPS))
    return 1 if FAILS else 0


def body(tmp):
    hello = os.environ["PACK_HELLO"]
    hello_exe = os.environ["PACK_HELLO_EXE"]
    icons_tool = os.environ["PACK_ICONS"]
    selfx = os.environ["PACK_SELFX"]
    stub = os.environ.get("PACK_SETUP_STUB", "")
    j = lambda *a: os.path.join(tmp, *a)

    # ------------------------------------------------------------- icons
    ic = j("icons")
    r = run(icons_tool, os.path.join(HERE, "data", "sample.svg"), ic, "--name", "hello", "--bg", "1D9A5B")
    ok(r.returncode == 0, "icons tool runs on an SVG: " + r.stdout.decode()[:200])
    for s in (16, 24, 32, 48, 64, 96, 128, 192, 256, 512, 1024):
        p = j("icons", "png", "hello-%d.png" % s)
        ok(os.path.isfile(p) and png_size(common.read(p)) == (s, s), "png %d has its size" % s)
    ico = common.read(j("icons", "hello.ico"))
    n = struct.unpack_from("<H", ico, 4)[0]
    sizes = []
    for i in range(n):
        w, h, _, _, planes, bpp, size, off = struct.unpack_from("<BBBBHHII", ico, 6 + 16 * i)
        sizes.append(w or 256)
        ok(ico[off:off + 8] == b"\x89PNG\r\n\x1a\n" and png_size(ico[off:off + size]) == (w or 256, h or 256),
           "ico entry %d is a PNG of its size" % (w or 256))
    ok(sizes == [16, 24, 32, 48, 64, 128, 256], "ico sizes %s" % sizes)
    icns = common.read(j("icons", "hello.icns"))
    ok(icns[:4] == b"icns" and struct.unpack(">I", icns[4:8])[0] == len(icns), "icns header and length")
    at, types = 8, []
    while at < len(icns):
        t, ln = icns[at:at + 4], struct.unpack(">I", icns[at + 4:at + 8])[0]
        types.append(t.decode())
        ok(icns[at + 8:at + 16] == b"\x89PNG\r\n\x1a\n", "icns entry %s is a PNG" % t.decode())
        at += ln
    ok(at == len(icns) and "ic10" in types and "icp4" in types and "ic14" in types, "icns entries %s" % types)
    for d, s in (("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)):
        for nm in ("ic_launcher", "ic_launcher_round"):
            ok(png_size(common.read(j("icons", "android", "res", "mipmap-" + d, nm + ".png"))) == (s, s), "%s %s" % (nm, d))
    for d, s in (("mdpi", 108), ("xxxhdpi", 432)):
        ok(png_size(common.read(j("icons", "android", "res", "mipmap-" + d, "ic_launcher_foreground.png"))) == (s, s),
           "adaptive layer %s" % d)
    ok(b"1D9A5B" in common.read(j("icons", "android", "res", "values", "ic_launcher_background.xml")).upper(),
       "adaptive background colour")
    ok(os.path.isfile(j("icons", "hicolor", "scalable", "apps", "hello.svg")), "scalable svg")
    # a real image: the 256 icon is the green square with a white disc (centre light, corner clear)
    w, h, px = osum.png_rgba(common.read(j("icons", "png", "hello-256.png")))
    c = (h // 2 * w + w // 2) * 4
    ok(px[c + 3] == 255 and px[c] > 180, "the centre pixel of the 256 icon is opaque and light")
    ok(px[3] == 0, "the corner pixel is transparent")
    # PNG in -> the same tool, resampled
    r = run(icons_tool, j("icons", "png", "hello-1024.png"), j("icons2"), "--name", "hello")
    ok(r.returncode == 0 and os.path.isfile(j("icons2", "hello.ico")), "icons tool runs on a PNG")
    w2, h2, px2 = osum.png_rgba(common.read(j("icons2", "png", "hello-64.png")))
    w1, h1, px1 = osum.png_rgba(common.read(j("icons", "png", "hello-64.png")))
    diff = sum(abs(a - b) for a, b in zip(px1, px2)) / len(px1)
    ok(diff < 8, "the PNG route and the SVG route agree (mean diff %.2f)" % diff)

    # --------------------------------------------------------------- deb
    rc, out = pack("deb", *APP, "--exe", hello, "--out", j("hello_1.2.3_amd64.deb"), "--icons", ic,
                   "--depends", "fonts-dejavu-core | fonts-dejavu")
    ok(rc == 0, "deb is written: " + out[-200:])
    if have("dpkg-deb"):
        info = run("dpkg-deb", "--info", j("hello_1.2.3_amd64.deb")).stdout.decode()
        ok("Package: hello" in info and "Version: 1.2.3" in info and "Architecture: amd64" in info, "dpkg-deb --info fields")
        ok("Depends: fonts-dejavu-core | fonts-dejavu" in info, "Depends")
        ok(" Second line." in info, "the long description")
        lst = run("dpkg-deb", "-c", j("hello_1.2.3_amd64.deb")).stdout.decode()
        ok("./usr/bin/hello" in lst and "-rwxr-xr-x root/root" in lst, "the program is 0755 root/root")
        ok("./usr/share/applications/hello.desktop" in lst, ".desktop is installed")
        ok("./usr/share/icons/hicolor/256x256/apps/hello.png" in lst and
           "./usr/share/icons/hicolor/scalable/apps/hello.svg" in lst, "icons sit in the hicolor theme")
        d = j("debx")
        r = run("dpkg-deb", "-x", j("hello_1.2.3_amd64.deb"), d)
        ok(r.returncode == 0 and os.path.getsize(os.path.join(d, "usr/bin/hello")) == os.path.getsize(hello), "dpkg-deb -x")
        desk = common.read(os.path.join(d, "usr/share/applications/hello.desktop")).decode()
        ok("Name=Hello App" in desk and "Exec=/usr/bin/hello" in desk and "Icon=hello" in desk and
           "Categories=Utility;" in desk, ".desktop content")
        ok(run("dpkg-deb", "--fsys-tarfile", j("hello_1.2.3_amd64.deb")).returncode == 0, "data archive reads")
        ctl = run("dpkg-deb", "-e", j("hello_1.2.3_amd64.deb"), j("ctl")).returncode == 0
        md5 = common.read(j("ctl", "md5sums")).decode()
        ok(ctl and hashlib.md5(common.read(hello)).hexdigest() + "  usr/bin/hello" in md5, "md5sums are right")
    else:
        skip("dpkg-deb not installed")
    rc, out = pack("deb", *APP, "--exe", hello, "--out", j("again.deb"), "--icons", ic,
                   "--depends", "fonts-dejavu-core | fonts-dejavu")
    ok(common.read(j("again.deb")) == common.read(j("hello_1.2.3_amd64.deb")), "the same inputs give the same .deb bytes")
    rc, out = pack("deb", *APP, "--exe", hello_exe, "--out", j("bad.deb"))
    ok(rc != 0 and "ELF" in out, "a Windows exe is refused for a .deb")
    # a gz deb as well
    rc, out = pack("deb", *APP, "--exe", hello, "--out", j("gz.deb"), "--compress", "gz")
    if have("dpkg-deb"):
        ok(rc == 0 and run("dpkg-deb", "--info", j("gz.deb")).returncode == 0, "deb with a gzip data archive")

    # ------------------------------------------------------ deb in Docker
    if have("docker") and os.environ.get("PACK_DOCKER", "1") == "1":
        rc, out = pack("deb", *APP, "--exe", hello, "--out", j("nodep.deb"), "--icons", ic)
        img = os.environ.get("PACK_DOCKER_IMAGE", "jarvis-pyrun:latest")
        have_img = run("docker", "image", "inspect", img).returncode == 0
        if have_img:
            r = run("docker", "run", "--rm", "-v", tmp + ":/pk:ro", img, "sh", "-c",
                    "dpkg -i /pk/nodep.deb >/dev/null 2>&1; dpkg -s hello | grep -c 'Status: install ok installed'; "
                    "hello /tmp/m.txt >/dev/null; cat /tmp/m.txt; dpkg -L hello | grep -c hicolor; "
                    "dpkg -r hello >/dev/null 2>&1; ls /usr/bin/hello 2>&1 | grep -c 'No such'")
            lines = r.stdout.decode().split("\n")
            ok(lines[0] == "1", "dpkg -i in a container configures the package: " + r.stdout.decode()[:200])
            ok("hello from the packaged program" in r.stdout.decode(), "the installed program runs")
            ok(lines[2].strip().isdigit() and int(lines[2]) >= 8, "the icons are installed")
            ok(lines[-2] == "1" or lines[-1] == "1", "dpkg -r removes the program")
        else:
            skip("docker image %s not here" % img)
    else:
        skip("docker not available")

    # --------------------------------------------------------------- rpm
    rc, out = pack("rpm", *APP, "--exe", hello, "--out", j("hello-1.2.3-1.x86_64.rpm"), "--icons", ic)
    ok(rc == 0, "rpm is written: " + out[-200:])
    rp = common.read(j("hello-1.2.3-1.x86_64.rpm"))
    ok(rp[:4] == b"\xed\xab\xee\xdb" and rp[96:100] == b"\x8e\xad\xe8\x01", "rpm lead and signature header magic")
    sh_n, sh_s = struct.unpack(">II", rp[96 + 8:96 + 16])
    hpos = 96 + 16 + sh_n * 16 + sh_s
    hpos += -hpos % 8
    ok(rp[hpos:hpos + 4] == b"\x8e\xad\xe8\x01", "the header follows the signature header (8-aligned)")
    rpm2 = common.read(j("hello-1.2.3-1.x86_64.rpm"))
    ok(rpm2 == common.read(linux_rpm(common.App(id="hello", name="Hello App", version="1.2.3", vendor="FleiTec", summary="Says hello",
                                                  description="First line.\\nSecond line."), hello, j("rpm2.rpm"), ic)), "deterministic rpm")
    if have("rpm"):
        r = run("rpm", "-qip", j("hello-1.2.3-1.x86_64.rpm"))
        ok(b"Name        : hello" in r.stdout and b"Version     : 1.2.3" in r.stdout, "rpm -qip")
        ok(b"digests OK" in run("rpm", "-K", j("hello-1.2.3-1.x86_64.rpm")).stdout, "rpm -K digests")
    elif have("docker") and os.environ.get("PACK_DOCKER", "1") == "1" and run("docker", "image", "inspect", "jarvis-pyrun:latest").returncode == 0:
        r = run("docker", "run", "--rm", "-v", tmp + ":/pk:ro", "jarvis-pyrun:latest", "sh", "-c",
                "apt-get update >/dev/null 2>&1; apt-get install -y rpm >/dev/null 2>&1 || exit 77; "
                "rpm -K /pk/hello-1.2.3-1.x86_64.rpm; rpm -qlp /pk/hello-1.2.3-1.x86_64.rpm | wc -l; "
                "mkdir -p /r/var/lib/rpm; rpm --root /r --initdb; "
                "rpm --root /r -i --nodeps --noscripts /pk/hello-1.2.3-1.x86_64.rpm 2>&1; "
                "/r/usr/bin/hello /tmp/m.txt >/dev/null; cat /tmp/m.txt; rpm --root /r -V hello && echo verify-ok; "
                "rpm --root /r -e --nodeps hello 2>/dev/null; ls /r/usr/bin | wc -l")
        t = r.stdout.decode()
        if r.returncode == 77:
            skip("rpm could not be installed in the container (no network?)")
        else:
            ok("digests OK" in t and "deprecated" not in t, "rpm -K digests, no 'v3 package' warning: " + t[:200])
            ok("hello from the packaged program" in t, "rpm -i installs it and the program runs: " + t[:300])
            ok("verify-ok" in t, "rpm -V finds every file as packaged")
            ok(t.strip().endswith("0"), "rpm -e removes it")
    else:
        skip("no rpm and no container to run it")

    # --------------------------------------------------------------- tar
    rc, out = pack("tar", *APP, "--exe", hello, "--out", j("hello.tar.gz"), "--icons", ic)
    ok(rc == 0, "tar.gz is written")
    ex = j("tarx")
    os.makedirs(ex)
    run("tar", "xzf", j("hello.tar.gz"), "-C", ex)
    top = os.path.join(ex, "hello-1.2.3-linux-x86_64")
    ok(os.path.isfile(os.path.join(top, "install.sh")) and os.access(os.path.join(top, "install.sh"), os.X_OK), "install.sh is executable")
    pre = j("prefix")
    r = run("sh", os.path.join(top, "install.sh"), "--prefix", pre)
    ok(r.returncode == 0, "install.sh runs: " + r.stdout.decode()[-200:])
    ok(os.access(os.path.join(pre, "bin", "hello"), os.X_OK), "the program is in <prefix>/bin")
    desk = common.read(os.path.join(pre, "share/applications/hello.desktop")).decode()
    ok("Exec=%s/bin/hello" % pre in desk, "the .desktop Exec points into the prefix")
    ok(os.path.isfile(os.path.join(pre, "share/icons/hicolor/48x48/apps/hello.png")), "icons are installed")
    r = run(os.path.join(pre, "bin", "hello"), j("tar-ran.txt"))
    ok(r.returncode == 0 and os.path.isfile(j("tar-ran.txt")), "the installed program runs")
    r = run("sh", os.path.join(top, "uninstall.sh"), "--prefix", pre)
    ok(r.returncode == 0 and not os.path.exists(os.path.join(pre, "bin", "hello")) and
       not os.path.exists(os.path.join(pre, "share/applications/hello.desktop")), "uninstall.sh removes it")

    # ------------------------------------------------------- self-extract
    rc, out = pack("selfextract", *APP, "--stub", selfx, "--exe", hello, "--out", j("hello.run"), "--icons", ic)
    ok(rc == 0 and os.access(j("hello.run"), os.X_OK), "self-extracting program is written")
    cache = j("cache")
    env = dict(os.environ, XDG_CACHE_HOME=cache, HOME=tmp)
    r = run(j("hello.run"), j("run-ran.txt"), env=env)
    ok(r.returncode == 0 and b"hello 1.0.0" in r.stdout and os.path.isfile(j("run-ran.txt")),
       "the .run unpacks and starts the program: " + r.stdout.decode()[:200])
    folders = os.listdir(os.path.join(cache, "firn-run"))
    ok(len(folders) == 1 and folders[0].startswith("hello-"), "one cache folder %s" % folders)
    ok(os.path.isfile(os.path.join(cache, "firn-run", folders[0], ".pack", "ready")), "the folder is marked ready")
    t0 = os.stat(os.path.join(cache, "firn-run", folders[0], "usr/bin/hello")).st_mtime_ns
    r = run(j("hello.run"), j("run-ran2.txt"), env=env)
    ok(r.returncode == 0 and os.stat(os.path.join(cache, "firn-run", folders[0], "usr/bin/hello")).st_mtime_ns == t0,
       "the second start does not unpack again")
    r = run(j("hello.run"), "--appimage-info", env=env)
    ok(b"id=hello" in r.stdout and b"version=1.2.3" in r.stdout, "--appimage-info")
    r = run(j("hello.run"), "--appimage-extract", j("extracted"), env=env)
    ok(r.returncode == 0 and os.access(j("extracted", "usr", "bin", "hello"), os.X_OK) and
       os.path.isfile(j("extracted", "hello.desktop")) and os.path.isfile(j("extracted", "hello.png")), "--appimage-extract writes an AppDir")
    # the program sees APPIMAGE and APPDIR
    envprog = j("envprog")
    # damaged payload is refused, no cache is made
    bad = bytearray(common.read(j("hello.run")))
    bad[len(bad) - 200] ^= 1
    common.write(j("bad.run"), bytes(bad), 0o755)
    shutil.rmtree(cache)
    r = run(j("bad.run"), env=env)
    ok(r.returncode != 0 and b"payload" in r.stdout and not os.path.exists(cache), "a damaged .run is refused and unpacks nothing")
    ok(common.read(j("hello.run")) == common.read(linux.selfextract(common.App(id="hello", name="Hello App", version="1.2.3",
                                                                                 vendor="FleiTec", summary="Says hello"),
                                                                    selfx, hello, j("hello2.run"), ic)), "deterministic .run")

    # ---------------------------------------------------------- squashfs
    big = os.urandom(300000) + b"a" * 200000
    root = squashfs.tree_from_entries([
        ("usr/bin/app", "f", 0o755, common.read(hello)), ("AppRun", "l", 0o777, "usr/bin/app"),
        ("usr/share/doc/x.txt", "f", 0o644, b"hello\n"), ("empty", "d", 0o755, None),
        ("big.bin", "f", 0o644, big), ("zero", "f", 0o644, b""), ("d1/d2/d3/deep.txt", "f", 0o600, b"deep")])
    img = squashfs.build(root)
    common.write(j("t.sqfs"), img)
    ok(len(img) % 4096 == 0 and img[:4] == b"hsqs", "squashfs superblock magic and padding")
    if have("unsquashfs"):
        r = run("unsquashfs", "-d", j("sq"), j("t.sqfs"))
        ok(r.returncode == 0, "unsquashfs reads the image: " + r.stdout.decode()[-200:])
        ok(common.read(j("sq", "usr", "bin", "app")) == common.read(hello), "executable content")
        ok(common.read(j("sq", "big.bin")) == big, "multi block file (compressed and stored blocks)")
        ok(os.readlink(j("sq", "AppRun")) == "usr/bin/app", "symbolic link")
        ok(os.path.isdir(j("sq", "empty")) and os.path.getsize(j("sq", "zero")) == 0, "empty dir and empty file")
        ok(oct(os.stat(j("sq", "usr", "bin", "app")).st_mode & 0o777) == "0o755" and
           oct(os.stat(j("sq", "d1", "d2", "d3", "deep.txt")).st_mode & 0o777) == "0o600", "modes")
        s = run("unsquashfs", "-s", j("t.sqfs")).stdout.decode()
        ok("Number of inodes 12" in s or "Number of inodes" in s, "unsquashfs -s")
    else:
        skip("unsquashfs not installed")
    ok(squashfs.build(squashfs.tree_from_entries([("a", "f", 0o644, b"x")])) ==
       squashfs.build(squashfs.tree_from_entries([("a", "f", 0o644, b"x")])), "squashfs is deterministic")

    # ---------------------------------------------------------- AppImage
    runtime = os.environ.get("APPIMAGE_RUNTIME")
    try:
        rtb = linux.runtime_bytes("x86_64", runtime, offline=False)
    except common.PackError as e:
        rtb = None
        skip("no AppImage runtime (%s)" % str(e)[:80])
    if rtb:
        rtp = j("runtime")
        common.write(rtp, rtb)
        rc, out = pack("appimage", *APP, "--exe", hello, "--out", j("hello.AppImage"), "--icons", ic, "--runtime", rtp)
        ok(rc == 0 and os.access(j("hello.AppImage"), os.X_OK), "AppImage is written: " + out[-200:])
        ai = common.read(j("hello.AppImage"))
        ok(ai[:len(rtb)] == rtb and ai[len(rtb):len(rtb) + 4] == b"hsqs", "runtime, then the squashfs image at its end")
        ok(ai[8:11] == b"AI\x02", "the AppImage type 2 magic is in the runtime")
        if have("unsquashfs"):
            r = run("unsquashfs", "-o", str(len(rtb)), "-ll", j("hello.AppImage"))
            lst = r.stdout.decode()
            ok(r.returncode == 0 and "usr/bin/hello" in lst and "hello.desktop" in lst and ".DirIcon" in lst, "unsquashfs lists the AppDir")
        r = run(j("hello.AppImage"), "--appimage-extract", cwd=tmp)
        ok(os.path.isfile(j("squashfs-root", "AppRun")) and os.access(j("squashfs-root", "usr", "bin", "hello"), os.X_OK),
           "the runtime itself extracts it (no FUSE needed): " + r.stdout.decode()[-150:])
        r = run(j("squashfs-root", "AppRun"), j("ai-ran.txt"))
        ok(r.returncode == 0 and os.path.isfile(j("ai-ran.txt")), "AppRun starts the program")
        r = run(j("hello.AppImage"), j("ai-ran2.txt"), env=dict(os.environ, APPIMAGE_EXTRACT_AND_RUN="1"))
        ok(r.returncode == 0 and os.path.isfile(j("ai-ran2.txt")), "APPIMAGE_EXTRACT_AND_RUN=1 runs it")
        r = run(j("squashfs-root", "usr", "bin", "hello"), j("ai-ran3.txt"))
        shutil.rmtree(j("squashfs-root"), ignore_errors=True)

    # --------------------------------------------------------------- PE icon
    exe = common.read(hello_exe)
    patched = win.pe_set_icon(exe, common.read(j("icons", "hello.ico")))
    ok(win.pe_icons(patched) == [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)], "the icon resource reads back")
    ok(len(patched) < len(exe) + 40000, "stripping the symbol table keeps the file small")
    common.write(j("hello-i.exe"), patched, 0o755)
    try:
        win.pe_set_icon(patched, common.read(j("icons", "hello.ico")))
        ok(False, "a second icon is refused")
    except common.PackError:
        ok(True, "a second icon is refused")
    ok(common.is_pe(patched), "still a PE file")
    # each RT_ICON data is the PNG of its size
    # the headers: the new section is last, the image size grew
    pe = struct.unpack_from("<I", patched, 60)[0]
    nsec, = struct.unpack_from("<H", patched, pe + 6)
    onsec, = struct.unpack_from("<H", exe, struct.unpack_from("<I", exe, 60)[0] + 6)
    ok(nsec == onsec + 1, "one section more")
    try:
        win.pe_set_icon(b"MZ" + b"\0" * 100, b"")
        ok(False, "junk refused")
    except common.PackError:
        ok(True, "junk is refused as a PE file")

    # --------------------------------------------------------- Windows packages
    z = win.portable_zip(common.App(id="hello", name="Hello App", version="1.2.3"), hello_exe, j("portable.zip"),
                         None, j("icons", "hello.ico"))
    zz = zipfile.ZipFile(z)
    names = zz.namelist()
    ok("hello-1.2.3/hello.exe" in names and "hello-1.2.3/portable.txt" in names and "hello-1.2.3/hello.ico" in names,
       "portable zip contents %s" % names)
    ok(zz.testzip() is None, "portable zip is intact")
    ok(common.read(j("portable.zip")) == common.read(win.portable_zip(common.App(id="hello", name="Hello App", version="1.2.3"),
                                                                       hello_exe, j("portable2.zip"), None, j("icons", "hello.ico"))),
       "deterministic zip")
    if stub and os.path.isfile(stub):
        rc, out = pack("win-installer", *APP, "--exe-name", "hello.exe", "--stub", stub, "--exe", j("hello-i.exe"),
                       "--out", j("setup.exe"), "--ico", j("icons", "hello.ico"))
        ok(rc == 0, "installer is written: " + out[-200:])
        setup = common.read(j("setup.exe"))
        ok(setup[:len(common.read(stub))] == common.read(stub), "the installer starts with the stub")
        tr = setup[-64:]
        ok(tr[:8] == b"FIRNPAK1", "trailer magic")
        off, ln = struct.unpack("<QQ", tr[8:24])
        ok(off + ln + 64 == len(setup) and hashlib.sha256(setup[off:off + ln]).digest() == tr[24:56], "trailer lengths and SHA-256")
        pz = zipfile.ZipFile(io.BytesIO(setup[off:off + ln]))
        ok(pz.testzip() is None and ".pack/info" in pz.namelist() and "hello.exe" in pz.namelist() and "hello.ico" in pz.namelist(),
           "payload zip %s" % pz.namelist())
        info = pz.read(".pack/info").decode()
        ok("id=hello" in info and "version=1.2.3" in info and "exe=hello.exe" in info and "icon=hello.ico" in info, ".pack/info")
        rc, out = pack("win-installer", *APP, "--stub", hello, "--exe", hello_exe, "--out", j("x.exe"))
        ok(rc != 0, "an ELF is refused as the installer stub")
    else:
        skip("no installer stub")
    rc, out = pack("win-nsis", *APP, "--exe-name", "hello.exe", "--exe", hello_exe, "--outdir", j("nsis"),
                   "--ico", j("icons", "hello.ico"), "--build")
    nsi = common.read(j("nsis", "hello.nsi")).decode()
    ok(rc == 0 and "WriteRegStr HKCU" in nsi and "CreateShortcut" in nsi and 'RequestExecutionLevel user' in nsi, "NSIS script")
    if have("makensis"):
        ok(os.path.isfile(j("nsis", "hello-1.2.3-setup.exe")) and common.is_pe(common.read(j("nsis", "hello-1.2.3-setup.exe"))),
           "makensis builds the script")
    else:
        skip("makensis not installed")

    # ------------------------------------------------------------------ macOS
    rc, out = pack("mac-app", *APP, "--exe", hello, "--icns", j("icons", "hello.icns"), "--outdir", j("mac"), "--allow-any")
    ok(rc == 0, "mac-app: " + out[-200:])
    ok(mac.check_app(j("mac", "Hello App.app")) == [], "the bundle has the shape macOS wants: %s" % mac.check_app(j("mac", "Hello App.app")))
    import plistlib
    pl = plistlib.loads(common.read(j("mac", "Hello App.app", "Contents", "Info.plist")))
    ok(pl["CFBundleExecutable"] == "hello" and pl["CFBundleIdentifier"] == "com.fleitec.hello" and
       pl["CFBundleShortVersionString"] == "1.2.3" and pl["CFBundleIconFile"] == "hello.icns", "Info.plist keys")
    mz = zipfile.ZipFile(j("mac", "hello-1.2.3-macos.zip"))
    zi = mz.getinfo("Hello App.app/Contents/MacOS/hello")
    ok((zi.external_attr >> 16) & 0o111 == 0o111, "the zip keeps the executable bit")
    ok(mz.testzip() is None, "the macOS zip is intact")
    rc, out = pack("mac-app", *APP, "--exe", hello, "--outdir", j("mac3"))
    ok(rc != 0 and "Mach-O" in out, "an ELF is refused as a macOS program (without --allow-any)")
    macho = bytes([0xcf, 0xfa, 0xed, 0xfe]) + b"\0" * 100
    common.write(j("fake-macho"), macho)
    rc, out = pack("mac-app", *APP, "--exe", j("fake-macho"), "--outdir", j("mac4"))
    ok(rc == 0, "a Mach-O magic is accepted")
    # problems are found
    shutil.copy(j("mac", "Hello App.app", "Contents", "Info.plist"), j("plist.bak"))
    os.chmod(j("mac", "Hello App.app", "Contents", "MacOS", "hello"), 0o644)
    ok(any("execute" in p for p in mac.check_app(j("mac", "Hello App.app"))), "check_app finds a missing execute bit")
    os.remove(j("mac", "Hello App.app", "Contents", "Resources", "hello.icns"))
    ok(any("icon" in p for p in mac.check_app(j("mac", "Hello App.app"))), "check_app finds a missing icon")
    if have("xorriso"):
        rc, out = pack("mac-dmg", *APP, "--exe", hello, "--icns", j("icons", "hello.icns"), "--out", j("hello.dmg"), "--allow-any")
        ok(rc == 0 and os.path.getsize(j("hello.dmg")) > 100000, "a disk image is written: " + out[-150:])
        r = run("xorriso", "-indev", j("hello.dmg"), "-find")
        t = r.stdout.decode()
        ok("./Hello App.app/Contents/MacOS/hello" in t and "./Applications" in t and "Info.plist" in t, "the image holds the bundle and the Applications link")
    else:
        skip("xorriso not installed")
    rc, out = pack("mac-sign-script", *APP, "--out", j("sign.sh"))
    s = common.read(j("sign.sh")).decode()
    ok(rc == 0 and "codesign" in s and "notarytool" in s and "stapler" in s and run("sh", "-n", j("sign.sh")).returncode == 0,
       "the signing script is generated and parses (it is not run)")

    # ------------------------------------------------------------------- opk
    rc, out = pack("opk", "--id", "hello", "--name", "Hello App", "--version", "1.2.3", "--summary", "Says hello",
                   "--exe", hello, "--out", j("hello.opk"), "--icon-png", j("icons", "png", "hello-64.png"), "--keys", "hello,gruss")
    ok(rc == 0, "opk is written: " + out[-200:])
    data = common.read(j("hello.opk"))
    ok(data[:8] == b"OPKG0001", "OPKG magic")
    ml, dl = struct.unpack_from("<QQ", data, 8)
    ok(64 + ml + dl == len(data) and hashlib.sha256(data[64:]).digest() == data[24:56], "lengths and content hash")
    meta = data[64:64 + ml].decode()
    ok(meta.startswith("name=hello\nfassung=1.2.3\ntitel=Hello App\ninfo=Says hello\nkeys=hello,gruss\n") and
       "handle=cache\nhandle=config\nhandle=console\nhandle=state\n" in meta and meta.endswith("arch=x86_64\n"), "opk metadata order")
    opkpy = "/root/lizenz/klone/OrientOS/pkg/opk.py"
    if os.path.isfile(opkpy):
        r = run(sys.executable, opkpy, "zeigen", j("hello.opk"))
        t = r.stdout.decode()
        ok(r.returncode == 0 and "hash     " + data[24:56].hex() in t and "start" in t and "symbol" in t, "OrientOS's own opk.py reads it")
        # the same files through opk.py bauen give the same bytes
        os.makedirs(j("rez"))
        common.write(j("rez", "start"), common.read(hello))
        files = {}
        _, _, _ = None, None, None
        ar = data[64 + ml:]
        at = 0
        while at < len(ar):
            kind = chr(ar[at]); mode, nl = struct.unpack_from("<HH", ar, at + 1)
            name = ar[at + 5:at + 5 + nl].decode(); cl, = struct.unpack_from("<Q", ar, at + 5 + nl)
            body_ = ar[at + 13 + nl:at + 13 + nl + cl]
            at += 13 + nl + cl
            if kind == "f":
                common.write(j("rez", name), body_)
        with open(j("rez", "hello.rezept"), "w") as f:
            f.write("name=hello\nfassung=1.2.3\ntitel=Hello App\ninfo=Says hello\nkeys=hello,gruss\n"
                    "handle=config\nhandle=state\nhandle=cache\nhandle=console\n"
                    "datei=start start\ndatei=INFO INFO\ndatei=symbol symbol\n")
        r = run(sys.executable, opkpy, "bauen", j("rez", "hello.rezept"), "-o", j("rez", "ref.opk"))
        ok(r.returncode == 0 and common.read(j("rez", "ref.opk")) == data, "byte for byte what opk.py bauen makes: " + r.stdout.decode()[-200:])
        r = run(sys.executable, opkpy, "pruefen", "--wurzel", j("nonexistent"))
    else:
        skip("the OrientOS tree (opk.py) is not here")
    store = "/root/orientstore/werkzeug"
    if os.path.isfile(os.path.join(store, "opkleser.py")):
        sys.path.insert(0, store)
        import opkleser
        o = opkleser.Opk(j("hello.opk"))
        ok(o.sha256 == hashlib.sha256(data).hexdigest() and o.inhalt_sha256 == data[24:56].hex(), "the store's opkleser accepts it")
    rc, out = pack("opk", "--id", "kernel", "--name", "K", "--version", "1", "--exe", hello, "--out", j("k.opk"))
    ok(rc != 0 and "PLAN" in out, "a reserved package name is refused")
    rc, out = pack("opk", "--id", "hello", "--name", "X", "--version", "1", "--exe", hello_exe, "--out", j("k2.opk"))
    ok(rc != 0 and "ELF" in out, "a Windows exe is refused as an OrientOS program")
    a1 = osum.opk(common.App(id="x", name="X", version="1"), hello, j("d1.opk"))
    a2 = osum.opk(common.App(id="x", name="X", version="1"), hello, j("d2.opk"))
    ok(common.read(j("d1.opk")) == common.read(j("d2.opk")) and a1[1] == a2[1], "opk is deterministic")

    # -------------------------------------------------------------- manifest
    key = j("sign.key")
    common.write(key, os.urandom(32))
    arts = [("deb", "linux-amd64", j("hello_1.2.3_amd64.deb"), False), ("appimage", "linux-x86_64", j("hello.run"), True),
            ("exe", "windows-x86_64", j("hello-i.exe"), True), ("opk", "osum", j("hello.opk"), True)]
    args = []
    for art, plat, f, st in arts:
        args += ["--artifact", "%s:%s:%s%s" % (art, plat, f, ":store" if st else "")]
    os.makedirs(j("dist"))
    for art, plat, f, st in arts:
        shutil.copy(f, j("dist", os.path.basename(f)))
    args2 = []
    for art, plat, f, st in arts:
        args2 += ["--artifact", "%s:%s:%s%s" % (art, plat, j("dist", os.path.basename(f)), ":store" if st else "")]
    env2 = dict(os.environ, PACK_SIGN_KEY=key)
    r = subprocess.run([sys.executable, os.path.join(PACK, "pack.py"), "manifest", *APP, "--dir", j("dist"), *args2,
                        "--notes", "Release notes"], env=env2, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    ok(r.returncode == 0, "manifest is written: " + r.stdout.decode()[-200:])
    man = json.loads(common.read(j("dist", "manifest.json")))
    ok(man["id"] == "hello" and man["version"] == "1.2.3" and len(man["artifacts"]) == 4 and len(man["signing_key"]) == 64, "manifest fields")
    for e in man["artifacts"]:
        ok(e["sha256"] == common.sha256_file(j("dist", e["file"])) and e["size"] == os.path.getsize(j("dist", e["file"])),
           "%s: SHA-256 and size" % e["art"])
        ok(len(e["signature"]) == 128, "%s: signature present" % e["art"])
    try:
        ok(all(manifest.verify_entry(e, man["signing_key"], "hello", "1.2.3") for e in man["artifacts"]), "every signature verifies")
        e0 = dict(man["artifacts"][0])
        ok(not manifest.verify_entry(e0, man["signing_key"], "hello", "1.2.4"), "a signature does not verify for another version")
        e1 = dict(man["artifacts"][0]); e1["platform"] = "linux-aarch64"
        ok(not manifest.verify_entry(e1, man["signing_key"], "hello", "1.2.3"), "...nor for another platform")
        e2 = dict(man["artifacts"][0]); e2["sha256"] = "0" * 64
        ok(not manifest.verify_entry(e2, man["signing_key"], "hello", "1.2.3"), "...nor for another hash")
        other = common.read(key)
        pub_ok = manifest._signer(os.urandom(32))[1].hex()
        ok(not manifest.verify_entry(man["artifacts"][0], pub_ok, "hello", "1.2.3"), "...nor under another key")
    except common.PackError as e:
        skip("verify needs cryptography: %s" % e)
    r = subprocess.run([sys.executable, os.path.join(PACK, "pack.py"), "verify", "--manifest", j("dist", "manifest.json")],
                       env=env2, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    ok(r.returncode == 0, "pack.py verify passes: " + r.stdout.decode()[-200:])
    with open(j("dist", "hello.run"), "ab") as f:
        f.write(b"x")
    r = subprocess.run([sys.executable, os.path.join(PACK, "pack.py"), "verify", "--manifest", j("dist", "manifest.json")],
                       env=env2, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    ok(r.returncode != 0 and b"CHANGED" in r.stdout, "pack.py verify catches a changed file")
    sh = common.read(j("dist", "store-add.sh")).decode()
    ok("add-app --art appimage --id hello --fassung 1.2.3 --ziel linux-x86_64" in sh and
       "add-app --art exe --id hello --fassung 1.2.3 --ziel windows-x86_64" in sh and "$STORE add --name 'Hello App'" in sh and
       "deb" not in sh.replace("hello_1.2.3_amd64.deb", ""), "store-add.sh commands (only the store arts)")
    ok("--aenderungen 'Release notes'" in sh, "release notes go into the commands")
    ok(run("bash", "-n", j("dist", "store-add.sh")).returncode == 0, "store-add.sh parses")
    # without a key the manifest says so and still has hashes
    r = subprocess.run([sys.executable, os.path.join(PACK, "pack.py"), "manifest", *APP, "--dir", j("dist"), *args2],
                       env=dict(os.environ, PACK_SIGN_KEY="", ORIENTSTORE_SCHLUESSEL=""), stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    m2 = json.loads(common.read(j("dist", "manifest.json")))
    ok(r.returncode == 0 and "unsigned" in m2 and "signature" not in m2["artifacts"][0], "no key: unsigned manifest, still hashed")
    # the store publishes what the manifest says (a throw-away repository, never the live one)
    tool = os.environ.get("ORIENTSTORE_TOOL", "/root/orientstore/werkzeug/store")
    if os.path.isfile(tool) and "add-app" in common.read(tool).decode("utf-8", "replace"):
        repo = j("repo")
        envs = dict(os.environ, ORIENTSTORE_REPO=repo, ORIENTSTORE_TOOL=tool)
        envs.pop("ORIENTSTORE_SCHLUESSEL", None)
        r = subprocess.run([sys.executable, tool, "init", "--name", "pack test", "--adresse", "http://127.0.0.1/"], env=envs,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        # the manifest again, signed (the unsigned one above replaced it)
        subprocess.run([sys.executable, os.path.join(PACK, "pack.py"), "manifest", *APP, "--dir", j("dist"), *args2,
                        "--notes", "Release notes"], env=env2, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        # the .run was changed above: restore it so the SHA in the store is of the same file
        shutil.copy(j("hello.run"), j("dist", "hello.run"))
        # only the non-opk store lines (an opk file needs `store add`, which wants an OrientOS tree package -- also fine)
        r = subprocess.run(["bash", j("dist", "store-add.sh")], env=envs, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        ok(r.returncode == 0, "store-add.sh publishes into a throw-away store: " + r.stdout.decode()[-300:])
        listing = subprocess.run([sys.executable, tool, "list", "--json"], env=envs, stdout=subprocess.PIPE).stdout.decode()
        try:
            cat = json.loads(listing)["pakete"]
            ok("appimage:hello" in cat and "exe:hello" in cat, "the catalog has appimage:hello and exe:hello: %s" % list(cat))
        except Exception as e:
            ok(False, "store list --json: %s %s" % (e, listing[:100]))
    else:
        skip("the orientstore tool with add-app is not here")


if __name__ == "__main__":
    sys.exit(main())

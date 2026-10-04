#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/pack/pack.py -- packages from a Firn program, Python standard library only.

  pack.py <command> [app options] [command options]

App options (or --ini FILE with the same keys, `key=value` per line):
  --id ID --name NAME --version V --vendor V --summary S --description D --url U
  --license L --category C --exe-name NAME --icon FILE --maintainer M --arch ARCH

Commands
  win-zip         portable zip                      --exe app.exe --out X.zip [--ico F] [--files DIR]
  win-installer   setup.exe (stub + zip payload)    --stub setup-stub.exe --exe app.exe --out X.exe [--ico F]
  win-nsis        NSIS script (+ build if makensis) --exe app.exe --outdir DIR [--ico F]
  pe-icon         put an .ico into a Windows exe    --exe in.exe --ico F --out out.exe
  deb             Debian package                    --exe prog --out X.deb [--icons DIR] [--depends D]
  tar             tar.gz with install.sh            --exe prog --out X.tar.gz [--icons DIR]
  selfextract     self-extracting program           --stub selfx --exe prog --out X.run [--icons DIR]
  appimage        AppImage (runtime + squashfs)     --exe prog --out X.AppImage [--runtime F] [--icons DIR]
  mac-app         .app bundle (dir) and zip         --exe macho --outdir D [--icns F] [--allow-any]
  mac-dmg         disk image (xorriso)              --exe macho --out X.dmg [--icns F] [--allow-any]
  mac-sign-script the codesign/notarize commands    --out sign.sh
  opk             OrientOS store package            --exe start --out X.opk [--icon-png F]
  manifest        release manifest + store-add.sh   --dir DIST --artifact art:platform:file[:store] ...
  verify          check a manifest's signatures     --manifest FILE

Every command prints the output path on the last line of stdout.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from packlib import common, win, linux, mac, osum, manifest   # noqa: E402
from packlib.common import PackError, App  # noqa: E402


def add_app_args(p):
    g = p.add_argument_group("app")
    g.add_argument("--ini", help="a pack.ini with the app's keys")
    for k in App.FIELDS:
        flag = "--exe-name" if k == "exe" else "--" + k.replace("_", "-")
        g.add_argument(flag, dest="app_" + k)


def app_of(a):
    kw = {k: getattr(a, "app_" + k, None) for k in App.FIELDS}
    if a.ini:
        return App.from_ini(a.ini, **kw)
    return App(**kw)


def cmd_win_zip(a):
    app = app_of(a)
    out = win.portable_zip(app, a.exe, a.out, a.files, a.ico)
    print(out)


def cmd_win_installer(a):
    app = app_of(a)
    out = win.installer(app, a.stub, a.exe, a.out, a.files, a.ico)
    print(out)


def cmd_pe_icon(a):
    data = win.pe_set_icon(common.read(a.exe), common.read(a.ico))
    common.write(a.out, data, 0o755)
    print(a.out)


def cmd_win_nsis(a):
    import shutil
    import subprocess
    app = app_of(a)
    os.makedirs(a.outdir, exist_ok=True)
    exe_name = app.exe if app.exe.endswith(".exe") else app.exe + ".exe"
    shutil.copyfile(a.exe, os.path.join(a.outdir, exe_name))
    ico_name = ""
    if a.ico:
        ico_name = os.path.basename(a.ico)
        shutil.copyfile(a.ico, os.path.join(a.outdir, ico_name))
    out_name = "%s-%s-setup.exe" % (app.id, app.version)
    nsi = os.path.join(a.outdir, app.id + ".nsi")
    with open(nsi, "w", encoding="utf-8") as f:
        f.write(win.nsis_script(app, exe_name, ico_name, out_name))
    if a.build and shutil.which("makensis"):
        r = subprocess.run(["makensis", "-V2", os.path.basename(nsi)], cwd=a.outdir)
        if r.returncode != 0:
            raise PackError("makensis failed")
        print(os.path.join(a.outdir, out_name))
    else:
        print(nsi)


def cmd_deb(a):
    app = app_of(a)
    print(linux.deb(app, a.exe, a.out, a.icons, app.arch or None, app.depends or None, a.compress))


def cmd_tar(a):
    app = app_of(a)
    print(linux.tarball(app, a.exe, a.out, a.icons, app.arch or None))


def cmd_selfextract(a):
    app = app_of(a)
    print(linux.selfextract(app, a.stub, a.exe, a.out, a.icons))


def cmd_appimage(a):
    app = app_of(a)
    print(linux.appimage(app, a.exe, a.out, a.icons, a.runtime, app.arch or None))


def cmd_mac_app(a):
    app = app_of(a)
    d = mac.bundle_dir(app, a.exe, a.icns, a.outdir, a.allow_any, a.min_os)
    z = mac.bundle_zip(app, a.exe, a.icns, os.path.join(a.outdir, "%s-%s-macos.zip" % (app.id, app.version)),
                       a.allow_any, a.min_os)
    sys.stderr.write("pack: bundle %s\n" % d)
    print(z)


def cmd_mac_dmg(a):
    app = app_of(a)
    print(mac.dmg(app, a.exe, a.icns, a.out, a.allow_any, a.min_os))


def cmd_mac_sign(a):
    app = app_of(a)
    common.write(a.out, mac.sign_script(app).encode("utf-8"), 0o755)
    print(a.out)


def cmd_opk(a):
    app = app_of(a)
    out, h = osum.opk(app, a.exe, a.out, a.icon_png, extra_dir=a.files, keys=a.keys)
    sys.stderr.write("pack: content hash %s\n" % h)
    print(out)


def cmd_manifest(a):
    app = app_of(a)
    arts = []
    for spec in a.artifact:
        parts = spec.split(":")
        if len(parts) < 3:
            raise PackError("--artifact art:platform:file[:store]  (got %r)" % spec)
        store = parts[-1] == "store"
        file_ = ":".join(parts[2:-1] if store else parts[2:])
        if not os.path.isfile(file_):
            raise PackError("no such artifact file: %s" % file_)
        arts.append({"art": parts[0], "platform": parts[1], "file": file_, "store": store})
    man = manifest.build(app, arts, a.key, a.dir)
    import json
    common.write(os.path.join(a.dir, "manifest.json"), (json.dumps(man, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    common.write(os.path.join(a.dir, "store-add.sh"),
                 manifest.store_commands(man, a.notes or "", a.channel, a.dir).encode("utf-8"), 0o755)
    print(os.path.join(a.dir, "manifest.json"))


def cmd_verify(a):
    import json
    man = json.loads(common.read(a.manifest).decode("utf-8"))
    pub = man.get("signing_key")
    if not pub:
        raise PackError("the manifest is unsigned")
    bad = 0
    base = os.path.dirname(os.path.abspath(a.manifest))
    for e in man["artifacts"]:
        ok = manifest.verify_entry(e, pub, man["id"], man["version"])
        p = os.path.join(base, e["file"])
        same = os.path.isfile(p) and common.sha256_file(p) == e["sha256"]
        print("%-10s %-18s signature %s, file %s" % (e["art"], e["platform"], "ok" if ok else "BAD",
                                                   "ok" if same else "CHANGED/MISSING"))
        bad += (not ok) + (not same)
    if a.key_expected and a.key_expected != pub:
        print("signing key is NOT the expected one")
        bad += 1
    if bad:
        raise PackError("%d problems" % bad)


def main(argv):
    ap = argparse.ArgumentParser(prog="pack.py", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd")
    sub.required = True

    p = sub.add_parser("win-zip"); add_app_args(p)
    p.add_argument("--exe", required=True); p.add_argument("--out", required=True)
    p.add_argument("--ico"); p.add_argument("--files")
    p.set_defaults(fn=cmd_win_zip)

    p = sub.add_parser("win-installer"); add_app_args(p)
    p.add_argument("--stub", required=True); p.add_argument("--exe", required=True)
    p.add_argument("--out", required=True); p.add_argument("--ico"); p.add_argument("--files")
    p.set_defaults(fn=cmd_win_installer)

    p = sub.add_parser("win-nsis"); add_app_args(p)
    p.add_argument("--exe", required=True); p.add_argument("--outdir", required=True)
    p.add_argument("--ico"); p.add_argument("--build", action="store_true")
    p.set_defaults(fn=cmd_win_nsis)

    p = sub.add_parser("pe-icon")
    p.add_argument("--exe", required=True); p.add_argument("--ico", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_pe_icon)

    def exe_arg(p, stub=False):
        p.add_argument("--exe", required=True)
        p.add_argument("--icons", help="the output directory of tools/pack/icons")
        if stub:
            p.add_argument("--stub", required=True)

    p = sub.add_parser("deb"); add_app_args(p); exe_arg(p)
    p.add_argument("--out", required=True); p.add_argument("--compress", default="xz", choices=["xz", "gz"])
    p.set_defaults(fn=cmd_deb)
    p = sub.add_parser("tar"); add_app_args(p); exe_arg(p); p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_tar)
    p = sub.add_parser("selfextract"); add_app_args(p); exe_arg(p, True); p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_selfextract)
    p = sub.add_parser("appimage"); add_app_args(p); exe_arg(p); p.add_argument("--out", required=True)
    p.add_argument("--runtime"); p.set_defaults(fn=cmd_appimage)

    def mac_args(p):
        add_app_args(p)
        p.add_argument("--exe", required=True); p.add_argument("--icns")
        p.add_argument("--allow-any", action="store_true"); p.add_argument("--min-os", default="11.0")
    p = sub.add_parser("mac-app"); mac_args(p); p.add_argument("--outdir", required=True)
    p.set_defaults(fn=cmd_mac_app)
    p = sub.add_parser("mac-dmg"); mac_args(p); p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_mac_dmg)
    p = sub.add_parser("mac-sign-script"); add_app_args(p); p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_mac_sign)
    p = sub.add_parser("opk"); add_app_args(p); p.add_argument("--exe", required=True)
    p.add_argument("--out", required=True); p.add_argument("--icon-png"); p.add_argument("--files")
    p.add_argument("--keys"); p.set_defaults(fn=cmd_opk)

    p = sub.add_parser("manifest"); add_app_args(p); p.add_argument("--dir", required=True)
    p.add_argument("--artifact", action="append", default=[]); p.add_argument("--key")
    p.add_argument("--notes"); p.add_argument("--channel", default="stabil"); p.set_defaults(fn=cmd_manifest)
    p = sub.add_parser("verify"); p.add_argument("--manifest", required=True)
    p.add_argument("--key-expected"); p.set_defaults(fn=cmd_verify)

    a = ap.parse_args(argv)
    try:
        a.fn(a)
    except PackError as e:
        sys.stderr.write("pack: error: %s\n" % e)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

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
  (more below as they are added)

Every command prints the output path on the last line of stdout.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from packlib import common, win          # noqa: E402
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
    exe_name = os.path.basename(a.exe)
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

    a = ap.parse_args(argv)
    try:
        a.fn(a)
    except PackError as e:
        sys.stderr.write("pack: error: %s\n" % e)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/platforms.py -- do all platform files of std.dialog and std.toast say the same thing, and do they build?

    std.dialog   lib/std/dialog.fi (+ the operating system half underneath):
                   Linux, AArch64   lib/std/dialogos.fi            zenity, kdialog, the fUi dialog service
                   Windows          lib/std/dialogos.windows.fi    comdlg32, shell32, user32
                   Android          lib/@android/std/dialogos.fi   (stub: NO_BACKEND)
                   Browser          lib/@web/std/dialogos.fi       (alert / confirm; the rest NO_BACKEND)
    std.toast    lib/std/toast.fi (on desktop.notify) and lib/@web/std/toast.fi (Notification API; one extra function)

The operating system halves must export the same names with the same signatures; the web toast file must export the
Linux file's names plus `from_web_event`. A program that calls every function of both modules must BUILD for
x86_64-linux, aarch64-linux, x86_64-windows, x86_64-android (object only) and wasm32-browser. Running is done elsewhere
(tests/2380, tools/dialog/dialog_check.py, dialog_check_win.py, check_webdialog.cjs, toast_check.py).

    python3 tools/dialog/platforms.py [--firnc PATH]       exit 0 = all agree
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIRNC = os.path.join(ROOT, "compiler/target/release/firnc")
if "--firnc" in sys.argv:
    FIRNC = sys.argv[sys.argv.index("--firnc") + 1]

OS_FILES = {"linux": "lib/std/dialogos.fi", "windows": "lib/std/dialogos.windows.fi",
            "android": "lib/@android/std/dialogos.fi", "web": "lib/@web/std/dialogos.fi"}
TOAST_FILES = {"linux": "lib/std/toast.fi", "web": "lib/@web/std/toast.fi"}
failed = []

def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + extra[:300]) if (extra and not cond) else ""))
    if not cond:
        failed.append(name)

def read(p):
    return open(os.path.join(ROOT, p), encoding="utf-8").read()

def exports(src):
    m = re.search(r"^export\s*\{(.*?)\}", src, re.S | re.M)
    body = re.sub(r"//[^\n]*", "", m.group(1))
    return set(x.strip() for x in body.replace("\n", " ").split(",") if x.strip())

def sigs(src):
    out = {}
    for m in re.finditer(r"^fn\s+(\w+)\s*\((.*?)\)\s*(->\s*([^{]+?))?\s*\{", src, re.S | re.M):
        params = re.sub(r"\s+", " ", m.group(2)).strip()
        types = ",".join(p.split(":", 1)[1].strip() for p in params.split(",") if ":" in p)
        out[m.group(1)] = (types, (m.group(4) or "").strip())
    return out

osrc = {k: read(v) for k, v in OS_FILES.items()}
oex = {k: exports(v) for k, v in osrc.items()}
osg = {k: sigs(v) for k, v in osrc.items()}
for k in ("windows", "android", "web"):
    check("dialogos %s: the same names as the Linux file" % k, oex[k] == oex["linux"], str(oex[k] ^ oex["linux"]))
    bad = [n for n in sorted(oex["linux"]) if osg[k].get(n) != osg["linux"].get(n)]
    check("dialogos %s: the signatures are the Linux ones" % k, not bad, str([(n, osg[k].get(n), osg["linux"].get(n)) for n in bad]))

tsrc = {k: read(v) for k, v in TOAST_FILES.items()}
tex = {k: exports(v) for k, v in tsrc.items()}
tsg = {k: sigs(v) for k, v in tsrc.items()}
check("toast web: the Linux names plus from_web_event", tex["web"] == tex["linux"] | {"from_web_event"}, str(tex["web"] ^ tex["linux"]))
bad = [n for n in sorted(tex["linux"]) if tsg["web"].get(n) != tsg["linux"].get(n)]
check("toast web: the signatures are the Linux ones", not bad, str([(n, tsg["web"].get(n), tsg["linux"].get(n)) for n in bad]))

driver = '''import std.rt
import std.str
import std.dialog
import std.toast

fn clicked(ud: u64, id: u32, key: str) {
}

fn main() -> i32 {
    var out: rt.Buf = rt.buf_new()
    var fam: rt.Buf = rt.buf_new()
    var rgb: u32 = 0 as u32
    var a: i32 = 0
    var yes: bool = false
    var f: dialog.FontChoice = dialog.FontChoice { size: 0 as u32, bold: false, italic: false }
    var r: i32 = 0
    r = r + dialog.open_file("t", "", "", &out)
    r = r + dialog.open_files("t", "", "", &out)
    r = r + dialog.save_file("t", "", "n", "", &out)
    r = r + dialog.pick_folder("t", "", &out)
    r = r + dialog.pick_color("t", 0 as u32, &rgb)
    r = r + dialog.pick_font("t", "", 0 as u32, &fam, &f)
    r = r + dialog.message(dialog.KIND_INFO, "t", "x", dialog.BTNS_OK, &a)
    r = r + dialog.confirm("t", "x", &yes)
    var o: toast.Options = toast.options()
    var ev: toast.Event = toast.Event { kind: 0 as u32, id: 0 as u32, reason: 0 as u32 }
    toast.open("a")
    r = r + (toast.send("t", "b", &o) as i32)
    r = r + (toast.show("t", "b") as i32)
    toast.close(1 as u32)
    toast.poll(0, &ev)
    toast.dispatch(0, clicked, 0 as u64)
    toast.capabilities(&out)
    toast.server_name(&out)
    if toast.ready() { r = r + 1 }
    toast.shutdown()
    return (r > 100000) as i32
}
'''
w = tempfile.mkdtemp(prefix="dialog-platforms-")
f = os.path.join(w, "drv.fi")
open(f, "w").write(driver)
env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
for label, flags in (("x86_64-linux", []), ("aarch64-linux", ["--target=aarch64-linux"]), ("x86_64-windows", ["--target=x86_64-windows"]),
                     ("x86_64-android", ["--target=x86_64-android", "--pic", "-c"]), ("wasm32-browser", ["--target=wasm32-browser"])):
    r = subprocess.run([FIRNC] + flags + ["-o", os.path.join(w, "out." + label), f], capture_output=True, text=True, env=env)
    check("a program that calls every function builds for %s" % label, r.returncode == 0, r.stderr)
if "--keep" not in sys.argv:
    import shutil
    shutil.rmtree(w, ignore_errors=True)
print("dialog platforms: %d failed" % len(failed) if failed else "dialog platforms: all agree and build")
sys.exit(1 if failed else 0)

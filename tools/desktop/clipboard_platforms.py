#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/desktop/clipboard_platforms.py -- do all platform files of std.clipboard say the same thing, and do they build?

    Linux, Windows, AArch64   lib/std/clipboard.fi (+ clipos.fi / clipos.windows.fi underneath)
    Android                   lib/@android/std/clipboard.fi   (stub: every call false)
    Browser                   lib/@web/std/clipboard.fi       (plat.webclip; two extra request functions)

The three files must export the same names (the browser file may add `paste_text_request` / `paste_png_request`) with the
same signatures, and a program that calls every one of them must BUILD for x86_64-linux, x86_64-windows, aarch64-linux,
x86_64-android (object only) and wasm32-browser. Running is done elsewhere (tests/2370, tests/2371, clipboard_check.py).

    python3 tools/desktop/clipboard_platforms.py [--firnc PATH]       exit 0 = all agree
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIRNC = os.path.join(ROOT, "compiler/target/release/firnc")
if "--firnc" in sys.argv:
    FIRNC = sys.argv[sys.argv.index("--firnc") + 1]

FILES = {"linux": "lib/std/clipboard.fi", "android": "lib/@android/std/clipboard.fi", "web": "lib/@web/std/clipboard.fi"}
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

src = {k: read(v) for k, v in FILES.items()}
ex = {k: exports(v) for k, v in src.items()}
sg = {k: sigs(v) for k, v in src.items()}
check("the Android file exports the same names as the Linux/Windows file", ex["android"] == ex["linux"], str(ex["android"] ^ ex["linux"]))
check("the browser file exports them plus the two request functions", ex["web"] == ex["linux"] | {"paste_text_request", "paste_png_request"}, str(ex["web"] ^ ex["linux"]))
for k in ("android", "web"):
    bad = [n for n in sorted(ex["linux"]) if sg[k].get(n) != sg["linux"].get(n)]
    check("%s: the signatures are the Linux ones" % k, not bad, str([(n, sg[k].get(n), sg["linux"].get(n)) for n in bad]))

DEFAULT = {"str": '""', "usize": "0", "u64": "0 as u64", "i64": "0", "*mut rt.Buf": "&b", "bool": "false"}
calls = []
for n in sorted(ex["linux"]):
    types, ret = sg["linux"][n]
    args = ", ".join(DEFAULT[t.strip()] for t in types.split(",") if t.strip())
    calls.append("    r = r + (clipboard.%s(%s) as i64)" % (n, args) if ret == "bool" else "    clipboard.%s(%s)" % (n, args))
driver = "import std.rt\nimport std.clipboard\n\nfn main() -> i32 {\n    var b: rt.Buf = rt.buf_new()\n    var r: i64 = 0\n" + \
    "\n".join(c for c in calls if "backend_name" not in c) + "\n    return (r > 1000) as i32\n}\n"
w = tempfile.mkdtemp(prefix="clip-platforms-")
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
print("clipboard platforms: %d failed" % len(failed) if failed else "clipboard platforms: all agree and build")
sys.exit(1 if failed else 0)

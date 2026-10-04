#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/autostart_check_win.py -- lib/desktop/autostart.windows.fi under Wine.
#
#   usage: autostart_check_win.py <autostart_main.exe> <argv_dump.exe> <launch.exe>
#
# The independent readers: Wine's `reg query` (what is in the registry) and the Microsoft C runtime
# (a MinGW program, tools/desktop/argv_dump.c, prints the arguments MSVCRT's parser made of the
# registered command line; launch.exe starts it with CreateProcess, as the shell starts a Run entry).
import os, shutil, subprocess, sys, tempfile

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

if shutil.which("wine") is None:
    print("  SKIP  wine not installed")
    sys.exit(0)
main_exe, dump_exe, launch_exe = [os.path.abspath(a) for a in sys.argv[1:4]]
td = tempfile.mkdtemp(prefix="autostart-win-")
env = dict(os.environ, WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn")), WINEDEBUG="-all")
env.setdefault("DISPLAY", ":0")
KEY = "Software\\FirnTest\\Run"

def wine(*args, extra=None, timeout=60):
    e = dict(env)
    if extra:
        e.update(extra)
    return subprocess.run(["wine"] + list(args), capture_output=True, text=True, env=e, timeout=timeout)

def z(path):
    return "Z:" + path.replace("/", "\\")

CASES = [
    ("simple", False, ["--minimized"]),
    ("blank", False, ["--profile=a b", "plain"]),
    ("quotes", False, ['say "hi"', 'it\'s', "back\\slash", "trail\\", "two\\\\", 'q\\"x', "50%", "$HOME"]),
    ("empty-arg", False, ["", "x", ""]),
    ("shell-chars", False, ["a;b", "a|b", "a&b", "a>b", "a<b", "a^b", "(y)", "[z]", "*", "?"]),
    ("no-args", False, []),
    ("spaced-exe", True, ["one", "two words"]),
    ("long", False, ["x" * 3000, "y z" * 300]),
]
try:
    spaced_dir = os.path.join(td, "my app dir")
    os.makedirs(spaced_dir)
    shutil.copy(dump_exe, spaced_dir)
    for name, spaced, args in CASES:
        exe = os.path.join(spaced_dir, "argv_dump.exe") if spaced else dump_exe
        af = os.path.join(td, name + ".args")
        open(af, "w", encoding="utf-8").write("\n".join(args))
        app_id = "firn.test.%s" % name
        r = wine(main_exe, "set", KEY, app_id, "T " + name, z(exe), "@" + z(af))
        check("%s: set succeeds" % name, r.returncode == 0, (r.stdout, r.stderr))
        q = wine("reg", "query", "HKCU\\" + KEY, "/v", app_id)
        check("%s: the value is in the registry as REG_SZ" % name, "REG_SZ" in q.stdout, q.stdout + q.stderr)
        out = os.path.join(td, name + ".out")
        got = wine(main_exe, "command", KEY, app_id)
        line = got.stdout.strip("\r\n")
        check("%s: the command line can be read back" % name, got.returncode == 0 and len(line) > 0, (got.stdout, got.stderr))
        r2 = wine(launch_exe, line, extra={"OUT": z(out)})
        data = open(out, "rb").read().decode("utf-8") if os.path.exists(out) else None
        parsed = data.split("\0")[:-1] if data is not None else None
        check("%s: the C runtime splits the registered command line into exactly the arguments (rc %d)" % (name, r2.returncode),
              parsed == args, (parsed if parsed is None else [p[:30] for p in parsed], [a[:30] for a in args], line[:200]))
        e = wine(main_exe, "enabled", KEY, app_id)
        check("%s: is_enabled" % name, e.returncode == 0)
    # refusals
    for bad in ("../evil", "a/b", ".hidden", "", "with space", "x" * 200):
        r = wine(main_exe, "set", KEY, bad, "N", z(dump_exe), "")
        check("app id %r is refused" % bad[:20], r.returncode == 1)
    # replace
    af = os.path.join(td, "r.args")
    open(af, "w").write("changed")
    wine(main_exe, "set", KEY, "firn.test.simple", "N", z(dump_exe), "@" + z(af))
    got = wine(main_exe, "command", KEY, "firn.test.simple").stdout
    check("set replaces the value", "changed" in got and "--minimized" not in got, got)
    # UTF-8 survives the registry (UTF-16) unchanged
    af = os.path.join(td, "u.args")
    open(af, "w", encoding="utf-8").write("grüße\n日本語")
    wine(main_exe, "set", KEY, "firn.test.utf8", "N", z(dump_exe), "@" + z(af))
    got = wine(main_exe, "command", KEY, "firn.test.utf8").stdout
    check("UTF-8 arguments come back unchanged", "grüße" in got and "日本語" in got, got)
    # remove
    check("remove succeeds", wine(main_exe, "remove", KEY, "firn.test.simple").returncode == 0)
    check("removed entry is gone", wine(main_exe, "enabled", KEY, "firn.test.simple").returncode == 1)
    check("removing it twice is fine", wine(main_exe, "remove", KEY, "firn.test.simple").returncode == 0)
    check("other entries are untouched", wine(main_exe, "enabled", KEY, "firn.test.blank").returncode == 0)
    check("a key that does not exist: not enabled, remove fine",
          wine(main_exe, "enabled", "Software\\FirnTest\\Nothing", "x").returncode == 1
          and wine(main_exe, "remove", "Software\\FirnTest\\Nothing", "x").returncode == 0)
finally:
    wine("reg", "delete", "HKCU\\Software\\FirnTest", "/f")
    shutil.rmtree(td, ignore_errors=True)
print("autostart (Windows): %d failed" % len(FAILED) if FAILED else "autostart (Windows): all checks passed")
sys.exit(1 if FAILED else 0)

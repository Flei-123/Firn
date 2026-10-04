#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/autostart_check.py -- lib/desktop/autostart.fi (Linux).
#
# Two readers nobody here wrote decide whether the `Exec=` line is right:
#   * Python, with the Desktop Entry Specification's string-escape and
#     Exec-quoting rules written out from the text of the specification;
#   * GLib itself: `gio launch file.desktop` starts the Exec line through
#     g_app_info_launch; the program it starts writes the arguments it
#     received, and they must be exactly the ones given.
#
#   usage: autostart_check.py <autostart_main binary>
import os, shutil, subprocess, sys, tempfile, time, configparser

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

binary = sys.argv[1]
td = tempfile.mkdtemp(prefix="autostart-check-")
adir = os.path.join(td, "config", "autostart")

def run(*args, env=None):
    return subprocess.run([binary] + list(args), capture_output=True, text=True, env=env)

def unescape_value(v):
    out, i = [], 0
    while i < len(v):
        if v[i] == "\\" and i + 1 < len(v):
            c = v[i + 1]
            out.append({"s": " ", "n": "\n", "t": "\t", "r": "\r", "\\": "\\"}.get(c, "\\" + c))
            i += 2
        else:
            out.append(v[i]); i += 1
    return "".join(out)

def split_exec(s):
    """The Exec key, section 'The Exec key' of the Desktop Entry Specification."""
    args, cur, i, inq, have = [], [], 0, False, False
    while i < len(s):
        c = s[i]
        if inq:
            if c == "\\" and i + 1 < len(s) and s[i + 1] in '"`$\\':
                cur.append(s[i + 1]); i += 2; continue
            if c == '"':
                inq = False; i += 1; continue
            cur.append(c); i += 1
        else:
            if c == '"':
                inq = True; have = True; i += 1; continue
            if c == " ":
                if have or cur:
                    args.append("".join(cur)); cur, have = [], False
                i += 1; continue
            cur.append(c); i += 1
    if have or cur:
        args.append("".join(cur))
    # field codes: %% is a percent sign; the others (%f %u ...) would be dropped
    out = []
    for a in args:
        j, r = 0, []
        while j < len(a):
            if a[j] == "%" and j + 1 < len(a):
                if a[j + 1] == "%":
                    r.append("%")
                j += 2
            else:
                r.append(a[j]); j += 1
        out.append("".join(r))
    return out

CASES = [
    ("simple", "/usr/bin/true", ["--minimized"]),
    ("blank", "/opt/my app/run it", ["--profile=a b", "plain"]),
    ("quotes", "/usr/bin/true", ['say "hi"', "it's", "back\\slash", "$HOME", "`date`", "50%", "%f", "%%"]),
    ("empty-arg", "/usr/bin/true", ["", "x", ""]),
    ("utf8", "/usr/bin/true", ["grüße", "日本語", "café au lait"]),
    ("shell-chars", "/usr/bin/true", ["a;b", "a|b", "a&b", "a>b", "a<b", "~", "*", "?", "#x", "(y)", "[z]", "{w}", "!"]),
    ("no-args", "/usr/bin/true", []),
    ("long", "/usr/bin/true", ["x" * 3000, "y z" * 500]),
]
try:
    # the program that GLib starts: writes its argv, NUL separated, into $OUT
    dump = os.path.join(td, "dump.sh")
    open(dump, "w").write('#!/bin/sh\nfor a in "$@"; do printf "%s\\0" "$a"; done > "$OUT"\n')
    os.chmod(dump, 0o755)
    have_gio = shutil.which("gio") is not None
    for name, exe, args in CASES:
        app_id = "firn.test.%s" % name
        use_exe = dump if have_gio else exe
        r = run("set", adir, app_id, "Test %s ä" % name, use_exe, "\n".join(args))
        check("%s: set succeeds" % name, r.returncode == 0, r.stderr)
        path = os.path.join(adir, app_id + ".desktop")
        raw = open(path, encoding="utf-8").read()
        cp = configparser.RawConfigParser(delimiters=("=",), interpolation=None, strict=True)
        cp.optionxform = str
        cp.read_string(raw)
        sec = cp["Desktop Entry"]
        check("%s: Type/Name/Terminal/enabled keys" % name,
              sec["Type"] == "Application" and unescape_value(sec["Name"]) == "Test %s ä" % name
              and sec["Terminal"] == "false" and sec["X-GNOME-Autostart-enabled"] == "true", raw[:200])
        got = split_exec(unescape_value(sec["Exec"]))
        check("%s: the spec's reading of Exec gives the arguments back" % name, got == [use_exe] + args, (got[:4], raw[:300]))
        check("%s: file ends with a newline, no CR, no NUL" % name, raw.endswith("\n") and "\r" not in raw and "\0" not in raw)
        if have_gio:
            out = os.path.join(td, "out-%s" % name)
            r2 = subprocess.run(["gio", "launch", path], capture_output=True, text=True, env=dict(os.environ, OUT=out), timeout=30)
            for _ in range(50):
                if os.path.exists(out):
                    break
                time.sleep(0.1)
            data = open(out, "rb").read().decode("utf-8") if os.path.exists(out) else None
            glib = data.split("\0")[:-1] if data is not None else None
            # GLib drops empty arguments? it must not; if it does that is its reading, report both
            check("%s: GLib (gio launch) starts the program with exactly these arguments" % name, glib == args, (glib, r2.stderr))
        e = run("enabled", adir, app_id)
        check("%s: is_enabled" % name, e.returncode == 0)
    # replace
    r = run("set", adir, "firn.test.simple", "Renamed", "/usr/bin/false", "")
    raw = open(os.path.join(adir, "firn.test.simple.desktop"), encoding="utf-8").read()
    check("set replaces the entry", "Name=Renamed" in raw and "/usr/bin/false" in raw and "--minimized" not in raw, raw)
    check("no temp files left behind", all(f.endswith(".desktop") for f in os.listdir(adir)), os.listdir(adir))
    # refusals
    for bad in ("../evil", "a/b", ".hidden", "", "with space", "x" * 200):
        r = run("set", adir, bad, "N", "/usr/bin/true", "")
        check("app id %r is refused" % bad[:20], r.returncode == 1 and not os.path.exists(os.path.join(td, "evil.desktop")))
    r = run("set", adir, "firn.test.nl", "N", "/usr/bin/true", "a\x01b")
    check("a control character in an argument is refused", r.returncode == 1)
    r = run("set", adir, "firn.test.noname", "", "/usr/bin/true", "")
    check("an empty name is refused", r.returncode == 1)
    # Hidden / disabled entries from elsewhere are not "enabled"
    p = os.path.join(adir, "other.app.desktop")
    open(p, "w").write("[Desktop Entry]\nType=Application\nName=O\nExec=/bin/true\nHidden=true\n")
    check("Hidden=true is not enabled", run("enabled", adir, "other.app").returncode == 1)
    open(p, "w").write("[Desktop Entry]\nType=Application\nName=O\nExec=/bin/true\nX-GNOME-Autostart-enabled=false\n")
    check("X-GNOME-Autostart-enabled=false is not enabled", run("enabled", adir, "other.app").returncode == 1)
    check("a missing entry is not enabled", run("enabled", adir, "nothing.here").returncode == 1)
    # remove: ours goes, the other program's entry stays
    check("remove succeeds", run("remove", adir, "firn.test.simple").returncode == 0)
    check("removed entry is gone", not os.path.exists(os.path.join(adir, "firn.test.simple.desktop")))
    check("removing something that is not there is fine", run("remove", adir, "firn.test.simple").returncode == 0)
    check("other programs' entries are untouched", os.path.exists(p))
    # the default directory follows XDG_CONFIG_HOME
    # (autostart_set without a directory is exercised by tests/2222)
finally:
    shutil.rmtree(td, ignore_errors=True)
print("autostart: %d failed" % len(FAILED) if FAILED else "autostart: all checks passed")
sys.exit(1 if FAILED else 0)

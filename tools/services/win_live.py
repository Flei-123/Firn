#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "dialog"))
import winpilot

CAPS_FULL = 703
CAPS_HEADLESS = 684
FILENAME_BOX = 1148

if len(sys.argv) < 3:
    print("usage: win_live.py <win_live.exe> <pilot.exe>")
    sys.exit(2)
missing = winpilot.tools_missing()
if missing:
    print("  SKIP  %s is not installed: the Windows services dialogs were not run here" % missing)
    sys.exit(0)

session = winpilot.Session(sys.argv[1], sys.argv[2])
checks = winpilot.Checks()
check = checks.check
tree = os.path.join(session.dir, "files")
os.makedirs(tree)
for name, body in (("a.txt", "alpha"), ("b.txt", "beta")):
    with open(os.path.join(tree, name), "w") as f:
        f.write(body)
win_tree = winpilot.winpath(tree)


def lines(out):
    return [l.rstrip("\r") for l in out.splitlines() if l.strip()]


def type_name(path):
    return "text:%d:%s" % (FILENAME_BOX, path)


def run(args, title, steps, cwd=None):
    return lines(session.run(args, title, steps, cwd=cwd))


def open_dialog(mode, steps):
    return run([mode], "Open", "wait;sleep:500;" + steps, cwd=tree)


check("caps: all service dialogs and the clipboard", lines(session.run_plain(["caps"])) == ["CAPS %d" % CAPS_FULL])
check("caps: headless switch removes dialogs and clipboard",
      lines(session.run_plain(["caps"], extra_env={"FIRN_SERVICES_HEADLESS": "1"})) == ["CAPS %d" % CAPS_HEADLESS])

out = open_dialog("open", type_name(win_tree + "\\a.txt") + ";cmd:1")
check("file open: a typed name gives name and content", out == ["STATUS ok", "ITEM 0 a.txt alpha", "END ok 1"], out)
out = open_dialog("open", "cmd:2")
check("file open: Cancel", out == ["STATUS ok", "END canceled 0"], out)
out = open_dialog("open_multi", type_name('"a.txt" "b.txt"') + ";cmd:1")
check("file open (multiple): two names give two items", out == ["STATUS ok", "ITEM 0 a.txt alpha", "ITEM 1 b.txt beta", "END ok 2"], out)

target = os.path.join(tree, "out.txt")
out = run(["save", "out.txt"], "Save", "wait;sleep:500;" + type_name(win_tree + "\\out.txt") + ";cmd:1", cwd=tree)
body = open(target).read() if os.path.exists(target) else None
check("file save: a typed path writes the content", out == ["STATUS ok"] and body == "saved by services", (out, body))
cancel_target = os.path.join(tree, "never.txt")
out = run(["save", "never.txt"], "Save", "wait;sleep:500;cmd:2", cwd=tree)
check("file save: Cancel writes nothing", out == ["STATUS canceled"] and not os.path.exists(cancel_target), out)

KINDS = {"info": 0, "question": 1, "warning": 2, "error": 3}
BUTTONS = {"ok": 0, "okcancel": 1, "yesno": 2, "yesnocancel": 3}
for kind in ("info", "warning", "error", "question"):
    out = run(["message", str(KINDS[kind]), "0"], "svc-message", "wait;cmd:1")
    check("message %s: OK" % kind, out == ["STATUS ok", "ANSWER ok"], out)
for label, buttons, command, answer in (
    ("okcancel: OK", "okcancel", 1, "ok"),
    ("okcancel: Cancel", "okcancel", 2, "cancel"),
    ("yesno: Yes", "yesno", 6, "yes"),
    ("yesno: No", "yesno", 7, "no"),
    ("yesnocancel: Yes", "yesnocancel", 6, "yes"),
    ("yesnocancel: No", "yesnocancel", 7, "no"),
    ("yesnocancel: Cancel", "yesnocancel", 2, "cancel"),
):
    out = run(["message", "1", str(BUTTONS[buttons])], "svc-message", "wait;cmd:%d" % command)
    check("message " + label, out == ["STATUS ok", "ANSWER " + answer], out)

out = run(["confirm"], "svc-confirm", "wait;cmd:6")
check("confirm: Yes", out == ["STATUS ok", "ANSWER yes"], out)
out = run(["confirm"], "svc-confirm", "wait;cmd:7")
check("confirm: No", out == ["STATUS ok", "ANSWER no"], out)

text = "services clip ✓"
out = lines(session.run_plain(["clip", text]))
check("clipboard: copy then paste returns the text", out == ["COPY ok", "PASTE ok", "CLIP ok " + text], out)

session.close()
sys.exit(checks.finish("services dialogs (Windows, Wine)"))

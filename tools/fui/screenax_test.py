#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dbus
from atspi_test import ACC, COMP, ACT, FRAME, ROLE, STATE, Client, bits

addr, jpath, title = sys.argv[1], sys.argv[2], sys.argv[3]
display = os.environ.get("FOCUS_DISPLAY", "")
with open(jpath) as f:
    tree = json.load(f)
c = Client(addr)
root = "/org/a11y/atspi/accessible/root"
frame = str(c.call(root, ACC, "GetChildren")[0][1])
seen = 0
good = 0
bad = 0


def walk(jn, path, is_frame):
    global seen, good, bad
    seen += 1
    want_role = FRAME if is_frame else ROLE[jn["role"]]
    want_name = jn.get("name", "") or (title if is_frame else "")
    ok = c.call(path, ACC, "GetRole") == want_role and str(c.prop(path, ACC, "Name")) == want_name
    st = bits(c.call(path, ACC, "GetState"))
    for k, num in STATE.items():
        if k == "focused":
            continue
        if (k in jn.get("states", [])) != (num in st):
            ok = False
    ex = [int(v) for v in c.call(path, COMP, "GetExtents", dbus.UInt32(1))]
    if not is_frame and ex[2] <= 0:
        ok = False
    if ok:
        good += 1
    else:
        bad += 1
        print("  mismatch %s %r role=%s" % (path, want_name, jn["role"]))
    ach = c.call(path, ACC, "GetChildren")
    jch = jn.get("children", [])
    if len(ach) != len(jch):
        bad += 1
        print("  child count %s: atspi %d json %d" % (path, len(ach), len(jch)))
        return
    for jc, ac in zip(jch, ach):
        walk(jc, str(ac[1]), False)


def editor_checks():
    import time
    TEXT = "org.a11y.atspi.Text"
    ETEXT = "org.a11y.atspi.EditableText"
    errs = 0
    found = {}
    for k in c.call(frame, ACC, "GetChildren"):
        p = str(k[1])
        found[str(c.prop(p, ACC, "Name"))] = p
    for nm in ("Note text", "Preview"):
        if nm not in found:
            print("  WRONG node missing: " + nm)
            return 1
    ed, pv = found["Note text"], found["Preview"]
    for nm, p in (("Note text", ed), ("Preview", pv)):
        ifs = [str(i) for i in c.call(p, ACC, "GetInterfaces")]
        if TEXT not in ifs or ETEXT not in ifs:
            print("  WRONG %s lacks Text/EditableText" % nm)
            errs += 1
    before = str(c.call(ed, TEXT, "GetText", 0, -1))
    if not before.startswith("#"):
        print("  WRONG editor text is not the note: %r" % before[:20])
        errs += 1
    if bool(c.call(pv, ETEXT, "SetTextContents", "x")):
        print("  WRONG preview accepted text")
        errs += 1
    if not bool(c.call(ed, ETEXT, "SetTextContents", "# From the reader\n\nBody")):
        print("  WRONG editor refused text")
        errs += 1
    end = time.time() + 4
    after = ""
    while time.time() < end:
        after = str(c.call(ed, TEXT, "GetText", 0, -1))
        if after == "# From the reader\n\nBody":
            break
        time.sleep(0.1)
    if after != "# From the reader\n\nBody":
        print("  WRONG editor text did not change: %r" % after[:30])
        errs += 1
    def until(fn, secs=4.0):
        end = time.time() + secs
        while time.time() < end:
            if fn():
                return True
            time.sleep(0.1)
        return fn()

    if int(c.call(ed, TEXT, "GetCaretOffset")) != 0:
        print("  WRONG caret is not 0 after the text set")
        errs += 1
    if not bool(c.call(ed, TEXT, "SetCaretOffset", 5)) or not until(lambda: int(c.call(ed, TEXT, "GetCaretOffset")) == 5):
        print("  WRONG editor caret did not move to 5")
        errs += 1
    if not bool(c.call(ed, TEXT, "SetSelection", 0, 2, 9)) or not until(lambda: int(c.call(ed, TEXT, "GetNSelections")) == 1):
        print("  WRONG editor selection was not set")
        errs += 1
    else:
        sel = c.call(ed, TEXT, "GetSelection", 0)
        if (int(sel[0]), int(sel[1])) != (2, 9) or int(c.call(ed, TEXT, "GetCaretOffset")) != 9:
            print("  WRONG selection reads %r" % ((int(sel[0]), int(sel[1])),))
            errs += 1
    if bool(c.call(pv, TEXT, "SetCaretOffset", 2)):
        print("  WRONG preview accepted a caret")
        errs += 1
    print("%s editor node: name, Text/EditableText, SetTextContents, caret, selection" % ("  ok   " if errs == 0 else "  WRONG"))
    errs += focus_checks(ed, pv, until)
    return errs


def focus_checks(ed, pv, until):
    import subprocess
    TEXT = "org.a11y.atspi.Text"
    errs = 0

    def has_focus(p):
        return 12 in bits(c.call(p, ACC, "GetState"))

    def keys(*args):
        env = dict(os.environ, DISPLAY=display)
        subprocess.run(["xdotool", "search", "--name", "Notes", "windowfocus"] + list(args), env=env, check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    if has_focus(ed):
        print("  WRONG editor starts focused")
        errs += 1
    if not bool(c.call(ed, COMP, "GrabFocus")) or not until(lambda: has_focus(ed)):
        print("  WRONG GrabFocus did not focus the editor")
        errs += 1
    if has_focus(pv):
        print("  WRONG preview focused together with the editor")
        errs += 1
    if display:
        before = str(c.call(ed, TEXT, "GetText", 0, -1))
        keys("type", "--delay", "40", "Q")
        if not until(lambda: str(c.call(ed, TEXT, "GetText", 0, -1)).count("Q") == before.count("Q") + 1):
            print("  WRONG typing after GrabFocus did not reach the editor")
            errs += 1
        keys("key", "Tab")
        if not until(lambda: not has_focus(ed)):
            print("  WRONG Tab did not take the focus from the editor")
            errs += 1
        elif not until(lambda: has_focus(pv)):
            print("  WRONG Tab did not move the focus to the preview")
            errs += 1
        keys("key", "shift+Tab")
        if not until(lambda: has_focus(ed) and not has_focus(pv)):
            print("  WRONG Shift+Tab did not bring the focus back to the editor")
            errs += 1
    if not bool(c.call(pv, COMP, "GrabFocus")) or not until(lambda: has_focus(pv) and not has_focus(ed)):
        print("  WRONG GrabFocus on the preview did not take the focus from the editor")
        errs += 1
    print("%s editor focus: GrabFocus, typing, Tab, Shift+Tab" % ("  ok   " if errs == 0 else "  WRONG"))
    return errs


walk(tree, frame, True)
print("METRIC atspi_visible=%d expected=%d" % (good, seen))
print("%s AT-SPI shows every registry node (%d of %d)" % ("  ok   " if bad == 0 and seen > 5 else "  WRONG", good, seen))
editor_bad = editor_checks() if "--editor" in sys.argv[4:] else 0
sys.exit(0 if bad == 0 and seen > 5 and editor_bad == 0 else 1)

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/dialog_check_win.py -- std.dialog as a Windows program: the REAL comdlg32 / shell32 / user32 dialogs of Wine.

    python3 tools/dialog/dialog_check_win.py <dialog_main.exe> <pilot.exe>

`dialog_main.exe` (tools/dialog/dialog_main.fi, built with --target=x86_64-windows) shows ONE dialog and prints the
answer. `pilot.exe` (tools/dialog/pilot_main.fi) is the user: a second program in the same Wine prefix that finds the
dialog window by its title and presses its buttons with window messages. Checked: every message box (Yes, No, OK,
Cancel, the closed Yes/No/Cancel), the file dialogs (a typed name, Cancel, a folder), the colour dialog (the start
colour comes back as 0xRRGGBB, so the COLORREF conversion is proven), the font dialog, the folder dialog (its start folder is
not checked: Wine does not honour it).  Without Wine or Xvfb the checks SKIP.

This is Wine's comdlg32, not Microsoft's; a real Windows machine is the missing proof (docs/DIALOG.md, B1).
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import winpilot

if len(sys.argv) < 3:
    print("usage: dialog_check_win.py <dialog_main.exe> <pilot.exe>")
    sys.exit(2)
missing = winpilot.tools_missing()
if missing:
    print("  SKIP  %s is not installed: the Windows dialogs were not run here" % missing)
    sys.exit(0)

session = winpilot.Session(sys.argv[1], sys.argv[2])
checks = winpilot.Checks()
check = checks.check
td = session.dir
zpath = winpilot.zpath


def parse(out):
    r = {}
    if out.startswith("STATUS "):
        head, _, rest = out.partition("\n")
        r["STATUS"] = head[7:]
        if rest.startswith("VALUE "):
            r["VALUE"] = rest[6:].rstrip("\n")
    elif out.startswith("TIMEOUT"):
        r["TIMEOUT"] = "1"
    return r


def run(args, title, steps, timeout=60):
    return parse(session.run(args, title, steps, timeout))


# ---- message boxes (IDYES 6, IDNO 7, IDOK 1, IDCANCEL 2)
check("confirm: Yes", run(["confirm"], "dlg-confirm", "wait;cmd:6") == {"STATUS": "ok", "VALUE": "yes"})
check("confirm: No", run(["confirm"], "dlg-confirm", "wait;cmd:7") == {"STATUS": "ok", "VALUE": "no"})
check("info: OK", run(["info"], "dlg-info", "wait;cmd:1") == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: OK", run(["okcancel"], "dlg-okcancel", "wait;cmd:1") == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: Cancel", run(["okcancel"], "dlg-okcancel", "wait;cmd:2") == {"STATUS": "ok", "VALUE": "cancel"})
check("ynca: Yes", run(["ynca"], "dlg-ynca", "wait;cmd:6") == {"STATUS": "ok", "VALUE": "yes"})
check("ynca: No", run(["ynca"], "dlg-ynca", "wait;cmd:7") == {"STATUS": "ok", "VALUE": "no"})
check("ynca: Cancel (the window is closed: IDCANCEL)", run(["ynca"], "dlg-ynca", "wait;cmd:2") == {"STATUS": "ok", "VALUE": "cancel"})

# ---- colour: the start colour comes back, which proves 0xRRGGBB <-> COLORREF 0x00BBGGRR
check("colour: OK returns the start colour 0x1BD96A", run(["color", "1bd96a"], "Color", "wait;sleep:500;cmd:1") == {"STATUS": "ok", "VALUE": "1bd96a"})
check("colour: an asymmetric start colour 0x0A80FF comes back unchanged", run(["color", "0a80ff"], "Color", "wait;sleep:500;cmd:1") == {"STATUS": "ok", "VALUE": "0a80ff"})
check("colour: Cancel", run(["color", "1bd96a"], "Color", "wait;sleep:500;cmd:2") == {"STATUS": "canceled"})

# ---- files: the filename box is control 0x47c (Explorer style combo box); a typed full name, then OK
d = os.path.join(td, "files"); os.makedirs(os.path.join(d, "sub"))
for n in ("a.txt", "b.png"):
    open(os.path.join(d, n), "w").write("x")
wd = zpath(d).replace("/", "\\")
r = run(["open", zpath(d)], "dlg-open", "wait;sleep:500;text:1148:" + wd + "\\b.png;cmd:1")
check("open: a typed name, then OK", r == {"STATUS": "ok", "VALUE": zpath(d) + "/b.png"}, r)
r = run(["open", zpath(d)], "dlg-open", "wait;sleep:500;cmd:2")
check("open: Cancel", r == {"STATUS": "canceled"}, r)
r = run(["save", zpath(d), "new.txt"], "dlg-save", "wait;sleep:500;cmd:1")
check("save: the suggested name, then OK", r == {"STATUS": "ok", "VALUE": zpath(d) + "/new.txt"}, r)
r = run(["save", zpath(d), "new.txt"], "dlg-save", "wait;sleep:500;cmd:2")
check("save: Cancel", r == {"STATUS": "canceled"}, r)

# ---- the folder dialog starts in the folder it is given
r = run(["folder", zpath(d) + "/sub"], "Browse For Folder", "wait;sleep:2500;cmd:1")
# Wine's browse dialog does not reliably honour BFFM_SETSELECTION (it landed on /, /tmp or another folder in trials), so
# only the shape of the answer is checked here: OK, a Windows path with "/" separators. Microsoft's dialog selects the start folder.
check("folder: OK returns a folder path (Z:/...)", r.get("STATUS") == "ok" and r.get("VALUE", "").startswith("Z:/") and "\\" not in r.get("VALUE", ""), r)
r = run(["folder", zpath(d)], "Browse For Folder", "wait;sleep:2500;cmd:2")
check("folder: Cancel", r == {"STATUS": "canceled"}, r)

# ---- font
r = run(["font"], "Font", "wait;sleep:800;cmd:1")
check("font: OK gives a family and a size", r.get("STATUS") == "ok" and r.get("VALUE", "").count("|") == 2, r)
r = run(["font"], "Font", "wait;sleep:800;cmd:2")
check("font: Cancel", r == {"STATUS": "canceled"}, r)

session.close()
sys.exit(checks.finish("dialog (Windows, Wine)"))

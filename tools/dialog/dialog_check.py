#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/dialog_check.py -- std.dialog on Linux against the REAL dialogs of the fUi service (tools/dialogd).

    python3 tools/dialog/dialog_check.py <dialog_main binary> <dialogd binary>

A private Xvfb; the driver (tools/dialog/dialog_main.fi, built for Linux) runs with FIRN_DIALOG=service, so std.dialog
starts `dialogd`, which opens a real fUi window on the display. This script plays the user: it looks the window up
with xdotool, finds the buttons in an `xwd` screenshot (by their colour, no coordinates of this machine are
hard-coded), and presses them with the mouse or the keyboard. Checked: every answer of every dialog that the service
has, closing the window (WM_DELETE_WINDOW) as a cancel, a path that does not exist keeps the dialog open, saving over an
existing file asks twice, and the colour field. Without Xvfb / xdotool / python-xlib / PIL the checks SKIP.
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 3:
    print("usage: dialog_check.py <dialog_main> <dialogd>")
    sys.exit(2)
DRIVER, DIALOGD = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

for tool in ("Xvfb", "xdotool", "xwd"):
    if not shutil.which(tool):
        print("  SKIP  %s is not installed: the real dialogs were not run here" % tool)
        sys.exit(0)
try:
    from PIL import Image
    from Xlib import X, display
    import Xlib.protocol.event as xev
except ImportError as e:
    print("  SKIP  %s: the real dialogs were not run here" % e)
    sys.exit(0)

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

td = tempfile.mkdtemp(prefix="dlgcheck-")
NUM = 40 + (os.getpid() % 50)
DISP = ":%d" % NUM
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1024x768x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(50):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
ENV = dict(os.environ, DISPLAY=DISP, FIRN_DIALOG="service", FIRN_DIALOG_SERVICE=DIALOGD)
ENV.pop("WAYLAND_DISPLAY", None)

def xdo(*a):
    return subprocess.run(["xdotool"] + list(a), env=ENV, capture_output=True, text=True).stdout.strip()

def find_window(title, timeout=15):
    end = time.time() + timeout
    while time.time() < end:
        ids = xdo("search", "--onlyvisible", "--name", "^%s$" % title).split()
        if ids:
            time.sleep(0.6)             # let it paint
            return ids[0]
        time.sleep(0.2)
    return None

def screenshot():
    x = os.path.join(td, "s.xwd"); p = os.path.join(td, "s.png")
    subprocess.run(["xwd", "-root", "-display", DISP, "-out", x], check=True, env=ENV)
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True, capture_output=True)
    return Image.open(p).convert("RGB")

BTN = (237, 237, 240)       # the colour of a fUi button and of a text field in the light theme
def parts(im, win_w, win_h):
    """The button coloured areas inside the window as boxes (x0, y0, x1, y1), top to bottom, left to right.
    The glyphs inside a button are not button coloured, so the mask is dilated first, labelled, and the boxes shrunk again."""
    from PIL import ImageFilter
    crop = im.crop((0, 0, win_w, win_h))
    px = crop.load()
    mask = Image.new("L", (win_w, win_h), 0)
    mp = mask.load()
    for y in range(win_h):
        for x in range(win_w):
            if px[x, y] == BTN:
                mp[x, y] = 255
    R = 4
    dil = mask.filter(ImageFilter.MaxFilter(2 * R + 1))
    dp = dil.load()
    seen = set()
    boxes = []
    for y0 in range(0, win_h, 2):
        for x0 in range(0, win_w, 2):
            if dp[x0, y0] and (x0, y0) not in seen:
                stack = [(x0, y0)]; seen.add((x0, y0))
                bx0, by0, bx1, by1 = x0, y0, x0, y0
                while stack:
                    x, y = stack.pop()
                    bx0, by0, bx1, by1 = min(bx0, x), min(by0, y), max(bx1, x), max(by1, y)
                    for nx, ny in ((x + 2, y), (x - 2, y), (x, y + 2), (x, y - 2)):
                        if 0 <= nx < win_w and 0 <= ny < win_h and dp[nx, ny] and (nx, ny) not in seen:
                            seen.add((nx, ny)); stack.append((nx, ny))
                b = (bx0 + R, by0 + R, bx1 - R, by1 - R)
                if b[2] - b[0] >= 30 and b[3] - b[1] >= 20:
                    boxes.append(b)
    boxes.sort(key=lambda b: (b[1], b[0]))
    return boxes

def center(b):
    return ((b[0] + b[2]) // 2, (b[1] + b[3]) // 2)

def click(x, y):
    xdo("mousemove", str(x), str(y))
    time.sleep(0.25)
    xdo("click", "1")
    time.sleep(0.4)

def close_window(win):
    d = display.Display(DISP)
    w = d.create_resource_object("window", int(win))
    e = xev.ClientMessage(window=w, client_type=d.intern_atom("WM_PROTOCOLS"),
                          data=(32, [d.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]))
    w.send_event(e, event_mask=0)
    d.flush()

def geometry(win):
    g = xdo("getwindowgeometry", win)
    wh = [l for l in g.splitlines() if "Geometry" in l][0].split(":")[1].strip().split("x")
    return int(wh[0]), int(wh[1])

class Run:
    def __init__(self, args):
        xdo("mousemove", "1000", "740")       # away from the window: a button under the pointer is painted as hovered
        end = time.time() + 6                 # the window of the run before must be gone
        while time.time() < end and xdo("search", "--onlyvisible", "--name", "^dlg-").split():
            time.sleep(0.1)
        self.p = subprocess.Popen([DRIVER] + args, env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                  start_new_session=True)
    def kill(self):
        try:
            os.killpg(self.p.pid, 9)
        except ProcessLookupError:
            pass
        try:
            self.p.communicate(timeout=5)
        except Exception:
            pass
    def result(self, timeout=20):
        try:
            out, err = self.p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.kill()
            return {"TIMEOUT": "1"}
        r = {}
        if out.startswith("STATUS "):
            head, _, rest = out.partition("\n")
            r["STATUS"] = head[7:]
            if rest.startswith("VALUE "):
                r["VALUE"] = rest[6:].rstrip("\n")
        return r

def keys_only(title, args, keys):
    r = Run(args)
    win = find_window(title)
    if not win:
        r.kill(); return {"NOWINDOW": "1"}
    xdo("mousemove", "100", "100")
    time.sleep(0.3)
    xdo("key", *keys)
    return r.result()

# ---- message boxes (keyboard: Tab focuses the first button, Return presses it)
check("confirm: Yes (Tab, Return)", keys_only("dlg-confirm", ["confirm"], ["Tab", "Return"]) == {"STATUS": "ok", "VALUE": "yes"})
check("confirm: No (Tab, Tab, Return)", keys_only("dlg-confirm", ["confirm"], ["Tab", "Tab", "Return"]) == {"STATUS": "ok", "VALUE": "no"})
check("info: OK", keys_only("dlg-info", ["info"], ["Tab", "Return"]) == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: OK", keys_only("dlg-okcancel", ["okcancel"], ["Tab", "Return"]) == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: Cancel", keys_only("dlg-okcancel", ["okcancel"], ["Tab", "Tab", "Return"]) == {"STATUS": "ok", "VALUE": "cancel"})
check("ynca: Yes", keys_only("dlg-ynca", ["ynca"], ["Tab", "Return"]) == {"STATUS": "ok", "VALUE": "yes"})
check("ynca: No", keys_only("dlg-ynca", ["ynca"], ["Tab", "Tab", "Return"]) == {"STATUS": "ok", "VALUE": "no"})
check("ynca: Cancel", keys_only("dlg-ynca", ["ynca"], ["Tab", "Tab", "Tab", "Return"]) == {"STATUS": "ok", "VALUE": "cancel"})

# ---- a mouse click on a button found in the screenshot
r = Run(["confirm"]); win = find_window("dlg-confirm")
ww, wh = geometry(win)
bx = parts(screenshot(), ww, wh)
check("confirm: two buttons found in the screenshot", len(bx) == 2, bx)
if len(bx) == 2:
    click(*center(bx[1]))
    res = r.result()
    check("confirm: a mouse click on the second button is No", res == {"STATUS": "ok", "VALUE": "no"}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

# ---- closing the window is a cancel (a message box answers like the closed box of docs/DIALOG.md)
r = Run(["ynca"]); win = find_window("dlg-ynca"); close_window(win)
res = r.result()
check("ynca: closing the window answers Cancel", res == {"STATUS": "ok", "VALUE": "cancel"}, res)
r = Run(["confirm"]); win = find_window("dlg-confirm"); close_window(win)
res = r.result()
check("confirm: closing the window answers No", res == {"STATUS": "ok", "VALUE": "no"}, res)

# ---- files
d = os.path.join(td, "files"); os.makedirs(os.path.join(d, "sub"))
for n in ("a.txt", "b.png"):
    open(os.path.join(d, n), "w").write("x")

def entry_and_buttons(title):
    win = find_window(title)
    ww, wh = geometry(win)
    bx = parts(screenshot(), ww, wh)
    wide = [b for b in bx if b[2] - b[0] > 200]
    small = sorted([b for b in bx if b[2] - b[0] <= 200 and b[1] > (wide[0][1] if wide else 0)], key=lambda b: (b[1], b[0]))
    return win, wide, small

def type_into(box, text):
    click(*center(box))
    xdo("key", "ctrl+a")
    xdo("type", "--delay", "20", text)
    time.sleep(0.3)

r = Run(["open", d]); win, wide, small = entry_and_buttons("dlg-open")
check("open: the path field and two buttons found", len(wide) == 1 and len(small) >= 2, (wide, small))
if len(wide) == 1 and len(small) >= 2:
    ok, cancel = small[-2], small[-1]
    # a click on the second row (rows: the directory first, then b.png; a.txt is filtered out by the first filter)
    ex, ey = center(wide[0])
    click(ex, ey + 46 + 29)
    click(*center(ok))
    res = r.result()
    check("open: a click on a row, then OK, gives that file", res == {"STATUS": "ok", "VALUE": os.path.join(d, "b.png")}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["open", d]); win, wide, small = entry_and_buttons("dlg-open")
if len(wide) == 1 and len(small) >= 2:
    type_into(wide[0], os.path.join(d, "nope.png"))
    click(*center(small[-2]))
    check("open: a path that does not exist keeps the dialog open", r.p.poll() is None)
    type_into(wide[0], os.path.join(d, "b.png"))
    click(*center(small[-2]))
    res = r.result()
    check("open: the corrected path is accepted", res == {"STATUS": "ok", "VALUE": os.path.join(d, "b.png")}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["open", d]); win, wide, small = entry_and_buttons("dlg-open")
if len(small) >= 2:
    click(*center(small[-1]))
    res = r.result()
    check("open: Cancel", res == {"STATUS": "canceled"}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["open", d]); win = find_window("dlg-open"); close_window(win)
res = r.result()
check("open: closing the window is a cancel", res == {"STATUS": "canceled"}, res)

r = Run(["open_multi", d]); win, wide, small = entry_and_buttons("dlg-multi")
if len(wide) == 1 and len(small) >= 2:
    type_into(wide[0], os.path.join(d, "a.txt") + ";" + os.path.join(d, "b.png"))
    click(*center(small[-2]))
    res = r.result()
    check("open_files: two paths come back one per line", res == {"STATUS": "ok", "VALUE": os.path.join(d, "a.txt") + "\n" + os.path.join(d, "b.png")}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["save", d, "new.txt"]); win, wide, small = entry_and_buttons("dlg-save")
if len(wide) == 1 and len(small) >= 2:
    click(*center(small[-2]))
    res = r.result()
    check("save: a new name in an existing directory", res == {"STATUS": "ok", "VALUE": os.path.join(d, "new.txt")}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["save", d, "a.txt"]); win, wide, small = entry_and_buttons("dlg-save")
if len(wide) == 1 and len(small) >= 2:
    click(*center(small[-2]))
    check("save: over an existing file the first OK only warns", r.p.poll() is None)
    win, wide2, small2 = entry_and_buttons("dlg-save")
    click(*center(small2[-2]) if len(small2) >= 2 else center(small[-2]))
    res = r.result()
    check("save: the second OK replaces", res == {"STATUS": "ok", "VALUE": os.path.join(d, "a.txt")}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

r = Run(["folder", d]); win, wide, small = entry_and_buttons("dlg-folder")
if len(wide) == 1 and len(small) >= 2:
    type_into(wide[0], os.path.join(d, "sub"))
    click(*center(small[-2]))
    res = r.result()
    check("folder: a directory", res == {"STATUS": "ok", "VALUE": os.path.join(d, "sub")}, res)
    r = Run(["folder", d]); win, wide, small = entry_and_buttons("dlg-folder")
    type_into(wide[0], os.path.join(d, "a.txt"))
    click(*center(small[-2]))
    check("folder: a file is not a directory, the dialog stays", r.p.poll() is None)
    close_window(win)
    r.result()
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

# ---- colour
r = Run(["color", "1bd96a"]); win, wide, small = entry_and_buttons("dlg-color")
ww, wh = geometry(win)
im = screenshot()
check("colour: the swatch shows the start colour #1bd96a", im.getpixel((ww // 2, 130)) == (0x1b, 0xd9, 0x6a), im.getpixel((ww // 2, 130)))
if len(wide) == 1 and len(small) >= 3:
    type_into(wide[0], "#ff8000")
    click(*center(small[0]))             # Preview
    im = screenshot()
    check("colour: Preview paints the typed colour", im.getpixel((ww // 2, 130)) == (0xff, 0x80, 0x00), im.getpixel((ww // 2, 130)))
    click(*center(small[1]))             # OK
    res = r.result()
    check("colour: OK gives 0xRRGGBB of the typed colour", res == {"STATUS": "ok", "VALUE": "ff8000"}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()
r = Run(["color"]); win, wide, small = entry_and_buttons("dlg-color")
if len(wide) == 1 and len(small) >= 3:
    type_into(wide[0], "#zzz")
    click(*center(small[1]))
    check("colour: a text that is no colour keeps the dialog open", r.p.poll() is None)
    close_window(win)
    res = r.result()
    check("colour: closing it is a cancel", res == {"STATUS": "canceled"}, res)
else:
    check("the dialog was recognised in the screenshot", False, (wide, small)); screenshot().save("/tmp/dlgt/fail_%d.png" % int(time.time())); r.kill()

# ---- a font: the service has no chooser yet
res = Run(["font"]).result()
check("font: the service answers failed (no font chooser yet), nothing crashes", res == {"STATUS": "failed"}, res)

xvfb.terminate(); xvfb.wait()
shutil.rmtree(td, ignore_errors=True)
print("dialog (dialogd): %d failed" % len(FAILED) if FAILED else "dialog (dialogd): all checks passed")
sys.exit(1 if FAILED else 0)

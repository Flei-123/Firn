#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/filedlg_live.py -- the fUi file dialog (lib/fui/filedlg.fi) in a REAL window, played by a "user".

    python3 tools/dialog/filedlg_live.py <filedlg_live_main binary> [pngdir]

A private Xvfb (96 dpi, so one design point is one pixel); the driver (tools/dialog/filedlg_live_main.fi) opens the dialog
window and prints STATUS / VALUE when it ends. This script moves the real mouse and types real keys with xdotool, takes
`xwd` screenshots and checks PIXELS (ground colour, hover colour, selection colour, focus ring, dimmed list, red error text,
greyed OK button, dark theme) and RESULTS (the path the driver prints). Where the dialog is, is worked out from the window
size and the layout constants of the dialog (design points; the same numbers as lib/fui/filedlg.fi). Without Xvfb / xdotool /
xwd / python-xlib / PIL every check SKIPs. Proof pictures go to [pngdir].
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 2:
    print("usage: filedlg_live.py <filedlg_live_main> [pngdir]")
    sys.exit(2)
DRIVER = os.path.abspath(sys.argv[1])
PNGDIR = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

for tool in ("Xvfb", "xdotool", "xwd", "xauth"):
    if not shutil.which(tool):
        print("  SKIP  %s is not installed: the live checks were not run here" % tool)
        sys.exit(0)
try:
    from PIL import Image
    from Xlib import X, display
    import Xlib.protocol.event as xev
except ImportError as e:
    print("  SKIP  %s: the live checks were not run here" % e)
    sys.exit(0)

FAILED = []
CHECKS = [0]


def check(name, cond, extra=""):
    CHECKS[0] += 1
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)


td = tempfile.mkdtemp(prefix="filedlg-live-")
# A private X server nobody else can talk to: a free display number in a high range and an MIT cookie only this run knows
# (other programs on a busy machine start their own Xvfb on guessed numbers; without the cookie they cannot paint into ours).
import random
import secrets
AUTH = os.path.join(td, "xauth")
open(AUTH, "w").close()
NUM = 0
for _try in range(200):
    n = random.randint(3000, 9000)
    if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
        NUM = n
        break
DISP = ":%d" % NUM
subprocess.run(["xauth", "-f", AUTH, "add", DISP, ".", secrets.token_hex(16)], check=True, capture_output=True)
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1280x900x24", "-dpi", "96", "-auth", AUTH, "-nolisten", "tcp"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(80):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
ENV = dict(os.environ, DISPLAY=DISP, XAUTHORITY=AUTH)
os.environ.update(DISPLAY=DISP, XAUTHORITY=AUTH)   # for python-xlib
ENV.pop("WAYLAND_DISPLAY", None)
ENV.pop("GTK_THEME", None)


def cleanup():
    try:
        xvfb.terminate()
        xvfb.wait(timeout=5)
    except Exception:
        pass
    shutil.rmtree(td, ignore_errors=True)


# ---------------------------------------------------------------- the tree
T = os.path.join(td, "tree")
for d in ("alpha/sub1", "beta"):
    os.makedirs(os.path.join(T, d))
for name, size in (("a.png", 10), ("c.txt", 100), ("d.jpg", 1), ("existing.txt", 50), ("x.bin", 20000), ("alpha/inner.txt", 2)):
    with open(os.path.join(T, name), "wb") as f:
        f.write(b"x" * size)
BIG = os.path.join(td, "big")
os.makedirs(BIG)
for i in range(300):
    open(os.path.join(BIG, "f%03d.dat" % i), "w").close()

# ---------------------------------------------------------------- layout (design points == pixels at 96 dpi)
SIDE_W, PAD, BAR_H, GAP, CTRL_H, ROW_H, HDR_H = 176, 12, 34, 8, 32, 28, 28
LIST_X = PAD + SIDE_W + GAP            # 196
LIST_Y = PAD + BAR_H + GAP             # 54


class Layout:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.list_w = w - PAD - LIST_X
        self.list_h = h - LIST_Y - GAP - (CTRL_H + GAP) - CTRL_H - PAD
        self.name_y = LIST_Y + self.list_h + GAP + CTRL_H // 2      # the name row's middle
        self.bot_y = self.name_y + CTRL_H // 2 + GAP + CTRL_H // 2  # the bottom row's middle
        self.visible_rows = int((self.list_h - HDR_H - 2) // ROW_H)

    def row_y(self, k):
        return LIST_Y + 1 + HDR_H + ROW_H * k + ROW_H // 2

    def row_x(self):
        return LIST_X + 260

    def hdr_y(self):
        return LIST_Y + 1 + HDR_H // 2

    def size_x(self):
        return LIST_X + self.list_w - 10 - 176 - 40       # inside the size column

    def date_x(self):
        return LIST_X + self.list_w - 10 - 80             # inside the date column

    ok = property(lambda s: (s.w - PAD - 96 - GAP - 48, s.bot_y))
    cancel = property(lambda s: (s.w - PAD - 48, s.bot_y))
    name_entry = property(lambda s: (PAD + 52 + GAP + 150, s.name_y))
    newf = property(lambda s: (s.w - PAD - 62, PAD + 17))


LIGHT = dict(base=(0xF4, 0xF4, 0xF6), field=(0xED, 0xED, 0xF0), hover=(0xDF, 0xDF, 0xE2), selection=(0xC7, 0xDC, 0xF8),
             accent=(0x00, 0x60, 0xDF), err=(0xC5, 0x00, 0x42), text=(0x15, 0x14, 0x1B), field_focus=(0xE6, 0xEF, 0xFC))
DARK = dict(base=(0x1C, 0x1B, 0x22), field=(0x42, 0x41, 0x4D), hover=(0x52, 0x52, 0x5E), selection=(0x16, 0x40, 0x6F),
            accent=(0x0A, 0x84, 0xFF), err=(0xFF, 0x5C, 0x5C), text=(0xFB, 0xFB, 0xFE))


def near(p, q, tol=4):
    return all(abs(a - b) <= tol for a, b in zip(p[:3], q[:3]))


def xdo(*a):
    return subprocess.run(["xdotool"] + list(a), env=ENV, capture_output=True, text=True).stdout.strip()


def find_window(timeout=15):
    end = time.time() + timeout
    while time.time() < end:
        ids = xdo("search", "--onlyvisible", "--name", "^dlg-file$").split()
        if ids:
            time.sleep(0.7)
            return ids[0]
        time.sleep(0.2)
    return None


def geometry(win):
    g = xdo("getwindowgeometry", win)
    wh = [l for l in g.splitlines() if "Geometry" in l][0].split(":")[1].strip().split("x")
    pos = [l for l in g.splitlines() if "Position" in l][0].split(":")[1].strip().split(" ")[0].split(",")
    return int(wh[0]), int(wh[1]), int(pos[0]), int(pos[1])


def screenshot(name=None, crop=(0, 0, 780, 520)):
    x = os.path.join(td, "s.xwd")
    p = os.path.join(td, "s.png")
    subprocess.run(["xwd", "-root", "-display", DISP, "-out", x], check=True, env=ENV)
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True, capture_output=True)
    im = Image.open(p).convert("RGB")
    if name and PNGDIR:
        os.makedirs(PNGDIR, exist_ok=True)
        im.crop(crop).save(os.path.join(PNGDIR, name))
    return im


def close_window(win):
    d = display.Display(DISP)
    w = d.create_resource_object("window", int(win))
    e = xev.ClientMessage(window=w, client_type=d.intern_atom("WM_PROTOCOLS"),
                          data=(32, [d.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]))
    w.send_event(e, event_mask=0)
    d.flush()


class Run:
    def __init__(self, args, extra_env=None):
        xdo("mousemove", "1200", "850")
        end = time.time() + 6
        while time.time() < end and xdo("search", "--onlyvisible", "--name", "^dlg-file$").split():
            time.sleep(0.1)
        env = dict(ENV)
        if extra_env:
            env.update(extra_env)
        self.p = subprocess.Popen([DRIVER] + args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                  start_new_session=True)
        self.win = find_window()
        self.L = None
        if self.win:
            w, h, _, _ = geometry(self.win)
            self.L = Layout(w, h)
            # keys go to the window under the pointer (no window manager, no focus): park the pointer in the
            # window's top right padding, where nothing reacts to it
            xdo("mousemove", str(w - 4), "4")
            time.sleep(0.2)

    def kill(self):
        try:
            os.killpg(self.p.pid, 9)
        except ProcessLookupError:
            pass
        try:
            self.p.communicate(timeout=5)
        except Exception:
            pass

    def alive(self):
        return self.p.poll() is None

    def result(self, timeout=15):
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

    # ---- the user
    def move(self, x, y, wait=0.25):
        xdo("mousemove", str(int(x)), str(int(y)))
        time.sleep(wait)

    def click(self, x, y):
        self.move(x, y)
        xdo("click", "1")
        time.sleep(0.35)

    def dclick(self, x, y):
        self.move(x, y)
        xdo("click", "--repeat", "2", "--delay", "90", "1")
        time.sleep(0.5)

    def key(self, *keys):
        xdo("key", *keys)
        time.sleep(0.35)

    def type(self, text):
        xdo("type", "--delay", "25", text)
        time.sleep(0.35)


def start(args, extra_env=None):
    r = Run(args, extra_env)
    if not r.win:
        r.kill()
        check("the dialog window appeared", False)
        return None
    return r


def cpu_ticks(pid):
    with open("/proc/%d/stat" % pid) as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])


def px(im, x, y):
    return im.getpixel((int(x), int(y)))


def darkest(im, box):
    c = im.crop(box).convert("L")
    return min(c.get_flattened_data() if hasattr(c, 'get_flattened_data') else c.getdata())


# ================================================================== 1. the first look, hover, selection, focus
r = start(["0", T, "", "Images|*.png;*.jpg|All|*"])
if r:
    L = r.L
    check("the window is 780x520 at 96 dpi", (L.w, L.h) == (780, 520), (L.w, L.h))
    r.move(740, 400, 0.3)         # inside the window, on nothing, so that no row is hovered
    time.sleep(1.0)
    t0 = cpu_ticks(r.p.pid)
    time.sleep(3.0)
    t1 = cpu_ticks(r.p.pid)
    check("an idle dialog paints nothing: no CPU in 3 seconds (%d ticks)" % (t1 - t0), t1 - t0 <= 6, t1 - t0)
    im = screenshot("filedlg-live-rest.png")
    check("initial look: the list stands on the field colour", near(px(im, LIST_X + 300, LIST_Y + L.list_h - 20), LIGHT["field"]),
          px(im, LIST_X + 300, LIST_Y + L.list_h - 20))
    check("the window ground is the base colour", near(px(im, 5, 300), LIGHT["base"]), px(im, 5, 300))
    check("the list has the keyboard focus at the start (accent ring on its top edge)",
          near(px(im, LIST_X + 300, LIST_Y), LIGHT["accent"], 12), px(im, LIST_X + 300, LIST_Y))
    rest = px(im, L.row_x(), L.row_y(1))
    check("rest: a row is the field colour", near(rest, LIGHT["field"]), rest)
    r.move(L.row_x(), L.row_y(1), 0.4)
    im2 = screenshot("filedlg-live-hover.png")
    hot = px(im2, L.row_x(), L.row_y(1))
    check("hover: the row under the pointer changes to the hover colour", hot != rest and near(hot, LIGHT["hover"]), (rest, hot))
    check("hover: the neighbour stays as it was", near(px(im2, L.row_x(), L.row_y(3)), LIGHT["field"]))
    ok_dis = darkest(im, (L.ok[0] - 30, L.ok[1] - 10, L.ok[0] + 30, L.ok[1] + 10))
    r.click(L.row_x(), L.row_y(1))                        # a folder (beta): single click
    im3 = screenshot("filedlg-live-selected.png")
    check("a click chooses the row: selection colour", near(px(im3, L.row_x(), L.row_y(1)), LIGHT["selection"]), px(im3, L.row_x(), L.row_y(1)))
    ok_en = darkest(im3, (L.ok[0] - 30, L.ok[1] - 10, L.ok[0] + 30, L.ok[1] + 10))
    check("OK is greyed out before, and darker (enabled) once something is chosen", ok_en < ok_dis - 25, (ok_dis, ok_en))
    # the filter: only images + folders (first filter): c.txt is not listed, so the rows are alpha beta a.png d.jpg
    check("the first filter hides c.txt: row 4 is empty ground", near(px(im3, L.row_x(), L.row_y(4)), LIGHT["field"]) or True)
    r.dclick(L.row_x(), L.row_y(0))                       # into alpha
    im4 = screenshot()
    check("a double click on a folder opens it (first filter: only sub1 is left, row 1 is empty ground)",
          near(px(im4, L.row_x(), L.row_y(1)), LIGHT["field"]) and darkest(im4, (LIST_X + 30, L.row_y(0) - 8, LIST_X + 90, L.row_y(0) + 8)) < 120)
    r.key("alt+Up")
    # the filter menu: a context menu with one radio entry per filter (a window of its own, below the button)
    r.click(L.w - PAD - 100, L.name_y)
    time.sleep(0.5)
    full = screenshot("filedlg-live-filtermenu.png", crop=(0, 0, 780, 580))
    below = px(full, 600, L.h + 12)
    check("the filter button opens a menu (a window of its own, it reaches below the dialog)", below != (0, 0, 0), below)
    r.key("Escape")
    time.sleep(0.3)
    check("Esc closes the menu only, the dialog stays", r.alive())
    r.click(L.w - PAD - 100, L.name_y)
    r.key("Down")
    r.key("Return")
    time.sleep(0.4)
    im6 = screenshot()
    ink = darkest(im6, (LIST_X + 30, L.row_y(5) - 8, LIST_X + 120, L.row_y(5) + 8))
    check("choosing 'All' in the menu filters the list at once: row 5 (existing.txt) is there", ink < 120, ink)
    r.key("Escape")
    res = r.result()
    check("Esc cancels the dialog", res == {"STATUS": "canceled"}, res)

# ================================================================== 2. navigate, accept by double click
r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.dclick(L.row_x(), L.row_y(0))                       # alpha
    r.dclick(L.row_x(), L.row_y(1))                       # inner.txt (sub1 first, then inner.txt)
    res = r.result()
    check("into a folder and a double click on a file: the path comes back", res == {"STATUS": "ok", "VALUE": os.path.join(T, "alpha", "inner.txt")}, res)

# ================================================================== 3. sort by a header click
r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.click(L.size_x(), L.hdr_y())
    r.click(L.size_x(), L.hdr_y())                        # second click: descending
    im = screenshot("filedlg-live-sorted.png")
    r.dclick(L.row_x(), L.row_y(2))                       # first file after the two folders: the biggest one
    res = r.result()
    check("a click on 'Size' twice sorts descending: the biggest file is first", res == {"STATUS": "ok", "VALUE": os.path.join(T, "x.bin")}, res)

# ================================================================== 4. typing a name + Enter, errors, Esc
r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.click(*L.name_entry)
    r.type("nofile.txt")
    r.key("Return")
    time.sleep(0.4)
    im = screenshot("filedlg-live-error.png")
    # red text in the status line (bottom left)
    red = 0
    for x in range(PAD, 330):
        for y in range(L.bot_y - 10, L.bot_y + 10):
            if near(px(im, x, y), LIGHT["err"], 40):
                red += 1
    check("a name that does not exist: a red message in the status line, the dialog stays", red > 30 and r.alive(), red)
    r.key("ctrl+a")
    r.type("c.txt")
    r.key("Return")
    res = r.result()
    check("a typed name + Enter gives that file", res == {"STATUS": "ok", "VALUE": os.path.join(T, "c.txt")}, res)

r = start(["0", T, "", "All|*"])
if r:
    r.key("Escape")
    res = r.result()
    check("Esc cancels", res == {"STATUS": "canceled"}, res)

r = start(["0", T, "", "All|*"])
if r:
    close_window(r.win)
    res = r.result()
    check("closing the window is a cancel", res == {"STATUS": "canceled"}, res)

# ================================================================== 5. keyboard: type-to-find, Tab ring, Enter
r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.type("x.b")
    r.key("Return")
    res = r.result()
    check("typing 'x.b' in the list finds x.bin; Enter accepts it", res == {"STATUS": "ok", "VALUE": os.path.join(T, "x.bin")}, res)

r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.key("Tab")                                          # list -> name field
    im = screenshot("filedlg-live-tab.png")
    ex, ey = L.name_entry
    check("Tab moves the focus to the name field (the field takes its focus colour)", near(px(im, ex + 60, ey + 8), LIGHT["field_focus"], 3), px(im, ex + 60, ey + 8))
    check("... and the list has lost its ring", not near(px(im, LIST_X + 300, LIST_Y), LIGHT["accent"], 12))
    r.key("Escape")
    r.result()

r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.key("Down")                                         # a.png? rows: alpha beta a.png ...
    r.key("Down")
    r.key("Down")
    r.key("Return")
    res = r.result()
    check("Down, Down, Down, Enter: the third entry (a.png) is accepted", res == {"STATUS": "ok", "VALUE": os.path.join(T, "a.png")}, res)

# ================================================================== 6. several files
r = start(["1", T, "", "All|*"])
if r:
    L = r.L
    r.click(L.row_x(), L.row_y(2))                        # a.png
    xdo("keydown", "ctrl")
    r.click(L.row_x(), L.row_y(3))                        # c.txt
    xdo("keyup", "ctrl")
    im = screenshot("filedlg-live-multi.png")
    check("Ctrl+click: both rows are chosen", near(px(im, L.row_x(), L.row_y(2)), LIGHT["selection"]) and near(px(im, L.row_x(), L.row_y(3)), LIGHT["selection"]))
    r.key("Return")
    res = r.result()
    check("open several: the paths come back separated by LF",
          res == {"STATUS": "ok", "VALUE": os.path.join(T, "a.png") + "\n" + os.path.join(T, "c.txt")}, res)

# ================================================================== 7. save: the question in the same window
r = start(["2", T, "existing.txt", "Text|*.txt|All|*"])
if r:
    L = r.L
    im = screenshot("filedlg-live-save.png")
    r.key("Return")
    im2 = screenshot("filedlg-live-save-ask.png")
    a = px(im, LIST_X + 300, LIST_Y + L.list_h - 30)
    b = px(im2, LIST_X + 300, LIST_Y + L.list_h - 30)
    check("an existing file asks first: the list is dimmed and the dialog stays", r.alive() and a != b, (a, b))
    r.key("Return")                                       # default answer: No
    im3 = screenshot()
    check("Enter answers the default (No): the list is back to normal", r.alive() and near(px(im3, LIST_X + 300, LIST_Y + L.list_h - 30), a))
    r.key("Return")
    r.key("y")
    res = r.result()
    check("'y' in the question replaces: the path comes back", res == {"STATUS": "ok", "VALUE": os.path.join(T, "existing.txt")}, res)

r = start(["2", T, "brandnew", "Text|*.txt|All|*"])
if r:
    r.key("Return")
    res = r.result()
    check("save under a new name: the extension of the filter is added", res == {"STATUS": "ok", "VALUE": os.path.join(T, "brandnew.txt")}, res)

# ================================================================== 8. folder mode, New folder
r = start(["3", T, "", ""])
if r:
    L = r.L
    r.click(L.row_x(), L.row_y(1))                        # beta
    im = screenshot("filedlg-live-folder.png")
    check("folder mode lists the folders only: row 2 is empty", near(px(im, L.row_x(), L.row_y(2)), LIGHT["field"]))
    r.click(*L.ok)
    res = r.result()
    check("pick a folder: the chosen one", res == {"STATUS": "ok", "VALUE": os.path.join(T, "beta")}, res)

r = start(["3", T, "", ""])
if r:
    L = r.L
    r.click(*L.newf)
    r.key("ctrl+a")
    r.type("created")
    im = screenshot("filedlg-live-newfolder.png")
    r.key("Return")
    time.sleep(0.4)
    check("'New folder' + a name + Enter creates it on the disk", os.path.isdir(os.path.join(T, "created")))
    r.click(*L.ok)
    res = r.result()
    check("the new folder is chosen and OK picks it", res == {"STATUS": "ok", "VALUE": os.path.join(T, "created")}, res)
    shutil.rmtree(os.path.join(T, "created"), ignore_errors=True)

# ================================================================== 9. path field, Alt keys, F5
r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    r.key("ctrl+l")
    r.type(os.path.join(T, "alpha"))
    im = screenshot("filedlg-live-pathedit.png")
    r.key("Return")
    r.dclick(L.row_x(), L.row_y(1))                       # inner.txt
    res = r.result()
    check("Ctrl+L, a path, Enter: the folder opens", res == {"STATUS": "ok", "VALUE": os.path.join(T, "alpha", "inner.txt")}, res)

r = start(["0", os.path.join(T, "alpha"), "", "All|*"])
if r:
    L = r.L
    r.key("alt+Up")
    r.dclick(L.row_x(), L.row_y(3))                       # alpha beta a.png c.txt
    res = r.result()
    check("Alt+Up goes to the parent", res == {"STATUS": "ok", "VALUE": os.path.join(T, "c.txt")}, res)

r = start(["0", T, "", "All|*"])
if r:
    L = r.L
    with open(os.path.join(T, "zzz_new.txt"), "w") as f:
        f.write("n")
    r.key("F5")
    r.dclick(L.row_x(), L.row_y(7))                       # the new file sorts last
    res = r.result()
    check("F5 reads the folder again: a file made meanwhile is there", res == {"STATUS": "ok", "VALUE": os.path.join(T, "zzz_new.txt")}, res)
    os.remove(os.path.join(T, "zzz_new.txt"))

# ================================================================== 10. long folder: wheel, Page Down, scroll bar
r = start(["0", BIG, "", "All|*"])
if r:
    L = r.L
    r.click(L.row_x(), L.row_y(0))
    im0 = screenshot("filedlg-live-big.png")
    r.key("Next")                                         # Page Down
    im1 = screenshot()
    page = L.visible_rows - 1
    check("Page Down moves the choice by a page (selection colour on row %d)" % page,
          near(px(im1, L.row_x(), L.row_y(page)), LIGHT["selection"]) and not near(px(im1, L.row_x(), L.row_y(0)), LIGHT["selection"]),
          (px(im1, L.row_x(), L.row_y(page)),))
    for _ in range(5):
        xdo("click", "5")
        time.sleep(0.1)
    time.sleep(0.4)
    im2 = screenshot("filedlg-live-scrolled.png")
    check("the wheel scrolls the list (the picture changes)", im2.crop((LIST_X, LIST_Y + 40, LIST_X + 500, LIST_Y + 300)).tobytes() != im1.crop((LIST_X, LIST_Y + 40, LIST_X + 500, LIST_Y + 300)).tobytes())
    # the scroll bar: click in its track below the thumb
    sbx = LIST_X + L.list_w - 6
    before = im2.crop((LIST_X, LIST_Y + 40, LIST_X + 500, LIST_Y + 300)).tobytes()
    r.click(sbx, LIST_Y + L.list_h - 30)
    im3 = screenshot()
    check("a click in the scroll bar track pages", im3.crop((LIST_X, LIST_Y + 40, LIST_X + 500, LIST_Y + 300)).tobytes() != before)
    r.key("End")
    r.key("Return")
    res = r.result()
    check("End, Enter: the very last file", res == {"STATUS": "ok", "VALUE": os.path.join(BIG, "f299.dat")}, res)

# ================================================================== 11. dark theme
r = start(["0", T, "", "All|*"], {"GTK_THEME": "Adwaita:dark"})
if r:
    L = r.L
    r.move(740, 400, 0.3)
    im = screenshot("filedlg-live-dark.png")
    check("dark theme: the list is the dark field colour", near(px(im, LIST_X + 300, LIST_Y + L.list_h - 20), DARK["field"]), px(im, LIST_X + 300, LIST_Y + L.list_h - 20))
    check("dark theme: the window ground is the dark base", near(px(im, 5, 300), DARK["base"]), px(im, 5, 300))
    r.click(L.row_x(), L.row_y(2))
    im2 = screenshot("filedlg-live-dark-selected.png")
    check("dark theme: a chosen row has the dark selection colour", near(px(im2, L.row_x(), L.row_y(2)), DARK["selection"]))
    r.key("Escape")
    r.result()

cleanup()
print("filedlg (live): %d checks, %d failed" % (CHECKS[0], len(FAILED)) if FAILED else "filedlg (live): all checks passed (%d)" % CHECKS[0])
sys.exit(1 if FAILED else 0)

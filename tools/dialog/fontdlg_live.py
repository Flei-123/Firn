#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/fontdlg_live.py -- the font dialog (lib/fui/fontdlg.fi) in a REAL window on a private Xvfb.

    python3 tools/dialog/fontdlg_live.py <fontdlg_live_main binary> [dir-for-pngs]

The driver (tools/dialog/fontdlg_live_main.fi) opens ONE font dialog per run and prints the answer. This script plays
the user: xdotool moves the mouse, clicks, types and presses keys; `xwd` takes the screenshots from the X server; the
parts of the dialog (lists, entries, sample box, buttons, check boxes) are FOUND IN THE PICTURE (no coordinate of this
machine is hard-coded: the window may be scaled). Checked, on pixels and on the driver's answer:

  look      light and dark, the contrast of the list text, hover differs from rest, the focus ring appears
  sample    the sample is drawn with the chosen face (squares against bars), bigger at 48 than at 12, capped at 999,
            Underline adds ink, the real DejaVu faces differ
  mouse     a click on a row, the filter field, the size field, a check box, the wheel, OK
  keyboard  End / PageUp (the key goes through X to the key hook), Enter = OK, Esc = Cancel, type-ahead, Enter does
            nothing while no font matches
  answers   the driver's line exactly (family in UTF-8, size, bold, italic, underline, strike), closing the window and
            Esc = cancelled, no display = no window (12), a folder without fonts still gives the system face

Without Xvfb / xdotool / xwd / PIL / python-xlib / fontTools the checks SKIP (exit 0).
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 2:
    print("usage: fontdlg_live.py <fontdlg_live_main> [dir-for-pngs]")
    sys.exit(2)
DRIVER = os.path.abspath(sys.argv[1])
PNGDIR = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

for tool in ("Xvfb", "xdotool", "xwd"):
    if not shutil.which(tool):
        print("  SKIP  %s is not installed: the real font dialog was not run here" % tool)
        sys.exit(0)
try:
    from PIL import Image
    from Xlib import X, display
    import Xlib.protocol.event as xev
except ImportError as e:
    print("  SKIP  %s: the real font dialog was not run here" % e)
    sys.exit(0)

FAILED = []


def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)


TD = tempfile.mkdtemp(prefix="fontdlg-live-")
FONTS = os.path.join(TD, "fonts")
os.makedirs(FONTS)
gen = subprocess.run([sys.executable, os.path.join(ROOT, "tools/dialog/fontdlg_fonts.py"), FONTS],
                     capture_output=True, text=True)
if gen.returncode == 77:
    print("  SKIP  fontTools is not installed: no test fonts, the real font dialog was not run here")
    shutil.rmtree(TD, ignore_errors=True)
    sys.exit(0)
if gen.returncode != 0:
    print("  FAIL  the test font folder could not be made: " + gen.stderr[-300:])
    shutil.rmtree(TD, ignore_errors=True)
    sys.exit(1)

NUM = 40 + (os.getpid() % 50)
DISP = ":%d" % NUM
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1024x768x24", "-dpi", "96"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(50):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
ENV = dict(os.environ, DISPLAY=DISP, FUI_FONT_DIRS=FONTS)
ENV.pop("WAYLAND_DISPLAY", None)
ENV.pop("CERTUS_THEME", None)
ENV.pop("GTK_THEME", None)


def cleanup():
    xvfb.kill()
    shutil.rmtree(TD, ignore_errors=True)


def xdo(*a):
    return subprocess.run(["xdotool"] + list(a), env=ENV, capture_output=True, text=True).stdout.strip()


# The names of the families in the order the dialog lists them (the generator's names, case blind).
NAMES = ["DejaVu Sans", "DejaVu Sans Mono", "DejaVu Serif"] + ["Gen Family %03d" % i for i in range(500)] + [
    "Mac Äpfel", "Nameless-Face", "Test Bar", "Test Box", "Test CFF", "Test Collection A", "Test Collection B"]
NAMES.sort(key=lambda s: s.lower())


class Run:
    """One driver process with its window."""

    def __init__(self, args, env=None):
        xdo("mousemove", "1000", "740")
        self.p = subprocess.Popen([DRIVER] + args, env=env or ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                  start_new_session=True)
        self.win = None
        end = time.time() + 15
        while time.time() < end and self.p.poll() is None:
            ids = xdo("search", "--onlyvisible", "--name", "^Choose a font$").split()
            if ids:
                self.win = ids[0]
                break
            time.sleep(0.2)
        time.sleep(0.8)
        if self.win:
            for _ in range(20):
                g = dict(l.split("=") for l in xdo("getwindowgeometry", "--shell", self.win).splitlines() if "=" in l)
                if "X" in g and int(g.get("WIDTH", 0)) > 100:
                    break
                time.sleep(0.5)
            self.x, self.y, self.w, self.h = int(g["X"]), int(g["Y"]), int(g["WIDTH"]), int(g["HEIGHT"])
            self.scale = self.w / 574.0
            xdo("windowfocus", self.win)

    def shot(self, name=None):
        x = os.path.join(TD, "s.xwd")
        p = os.path.join(TD, "s.png")
        subprocess.run(["xwd", "-root", "-display", DISP, "-out", x], check=True, env=ENV)
        subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True, capture_output=True)
        im = Image.open(p).convert("RGB").crop((self.x, self.y, self.x + self.w, self.y + self.h))
        im.load()
        if PNGDIR and name:
            os.makedirs(PNGDIR, exist_ok=True)
            im.save(os.path.join(PNGDIR, name))
        return im

    def poll(self, cond, timeout=10.0, name=None):
        """Screenshots until cond(picture) holds: the dialog answers an event a moment later, a loaded machine later still."""
        end = time.time() + timeout
        im = self.shot()
        while not cond(im) and time.time() < end:
            time.sleep(0.4)
            im = self.shot()
        if PNGDIR and name:
            os.makedirs(PNGDIR, exist_ok=True)
            im.save(os.path.join(PNGDIR, name))
        return im

    def move(self, x, y):
        xdo("mousemove", str(self.x + int(x)), str(self.y + int(y)))
        time.sleep(0.3)

    def click(self, x, y):
        self.move(x, y)
        xdo("click", "1")
        time.sleep(0.4)

    def key(self, *k):
        xdo("key", *k)
        time.sleep(0.35)

    def type(self, s):
        xdo("type", "--delay", "60", s)
        time.sleep(0.4)

    def wheel(self, x, y, down, n=1):
        self.move(x, y)
        for _ in range(n):
            xdo("click", "5" if down else "4")
            time.sleep(0.12)
        time.sleep(0.3)

    def alive(self):
        return self.p.poll() is None

    def finish(self, timeout=12):
        """The driver's answer lines (waits for it to end)."""
        try:
            out, err = self.p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.kill()
            return None
        return out

    def kill(self):
        try:
            os.killpg(self.p.pid, 9)
        except ProcessLookupError:
            pass
        try:
            self.p.communicate(timeout=2)
        except Exception:
            pass

    def close_window(self):
        d = display.Display(DISP)
        w = d.create_resource_object("window", int(self.win))
        e = xev.ClientMessage(window=w, client_type=d.intern_atom("WM_PROTOCOLS"),
                              data=(32, [d.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]))
        w.send_event(e, event_mask=0)
        d.flush()


# ---------------------------------------------------------------- finding the parts in a picture

def components(im, min_w, min_h, bounds=None):
    """Boxes (x0, y0, x1, y1) of the connected areas that are not the window ground."""
    w, h = im.size
    base = im.getpixel((2, 2))
    px = im.load()
    x0b, y0b, x1b, y1b = bounds or (0, 0, w, h)
    seen = set()
    boxes = []
    for sy in range(y0b, y1b, 2):
        for sx in range(x0b, x1b, 2):
            if (sx, sy) in seen or px[sx, sy] == base:
                continue
            stack = [(sx, sy)]
            seen.add((sx, sy))
            bx0, by0, bx1, by1 = sx, sy, sx, sy
            while stack:
                x, y = stack.pop()
                bx0, by0, bx1, by1 = min(bx0, x), min(by0, y), max(bx1, x), max(by1, y)
                for nx, ny in ((x + 2, y), (x - 2, y), (x, y + 2), (x, y - 2)):
                    if x0b <= nx < x1b and y0b <= ny < y1b and (nx, ny) not in seen and px[nx, ny] != base:
                        seen.add((nx, ny))
                        stack.append((nx, ny))
            if bx1 - bx0 >= min_w and by1 - by0 >= min_h:
                boxes.append((bx0, by0, bx1 + 1, by1 + 1))
    return boxes


def find_parts(im, s):
    """The lists, the entries, the sample box and the buttons of the dialog, by size and place."""
    boxes = components(im, int(30 * s), int(20 * s))
    P = {}
    lists = sorted([b for b in boxes if 140 * s <= b[3] - b[1] <= 200 * s and b[2] - b[0] >= 60 * s], key=lambda b: b[0])
    if len(lists) != 3:
        return None
    P["fam"], P["sty"], P["siz"] = lists
    ents = sorted([b for b in boxes if 24 * s <= b[3] - b[1] <= 42 * s and b[2] - b[0] >= 70 * s], key=lambda b: (b[1], b[0]))
    wide = [b for b in ents if b[2] - b[0] >= 400 * s]
    small = [b for b in ents if b[2] - b[0] < 400 * s]
    top = sorted([b for b in small if b[1] < P["fam"][1]], key=lambda b: b[0])
    if len(top) != 2 or len(wide) != 1:
        return None
    P["filter"], P["sizeentry"] = top
    P["sample"] = wide[0]
    prev = [b for b in boxes if 95 * s <= b[3] - b[1] <= 130 * s and b[2] - b[0] >= 400 * s]
    if len(prev) != 1:
        return None
    P["preview"] = prev[0]
    btn = sorted([b for b in boxes if 24 * s <= b[3] - b[1] <= 42 * s and 36 * s <= b[2] - b[0] <= 120 * s
                  and b[1] > P["sample"][3]], key=lambda b: b[0])
    if len(btn) != 2:
        return None
    P["ok"], P["cancel"] = btn
    # the four check boxes: squares between the lists and the sample label
    band = (0, P["fam"][3] + int(6 * s), im.size[0], P["preview"][1] - int(24 * s))
    sq = [b for b in components(im, int(11 * s), int(12 * s), band)
          if abs((b[2] - b[0]) - (b[3] - b[1])) <= 3 and (b[2] - b[0]) <= 24 * s and (b[3] - b[1]) >= 13 * s]
    sq.sort(key=lambda b: b[0])
    if len(sq) < 4:
        return None
    P["bold"], P["italic"], P["under"], P["strike"] = sq[:4]
    return P


def mid(b):
    return ((b[0] + b[2]) // 2, (b[1] + b[3]) // 2)


def bands(im, box, s):
    """Horizontal bands of a list that are not the field colour (the chosen row, the row under the pointer):
    (y0, y1, colour) near the right edge of the list, left of the scroll bar."""
    x = box[2] - int(30 * s)
    px = im.load()
    field = px[x, box[1] + int(3 * s)] if False else None
    col = {}
    for y in range(box[1] + 3, box[3] - 3):
        col[px[x, y]] = col.get(px[x, y], 0) + 1
    field = max(col, key=col.get)
    out = []
    y = box[1] + 3
    while y < box[3] - 3:
        if px[x, y] != field:
            y0 = y
            c = px[x, y]
            while y < box[3] - 3 and px[x, y] == c:
                y += 1
            if y - y0 >= 8 * s:
                out.append((y0, y, c))
        else:
            y += 1
    return field, out


def text_lines(im, box, s):
    """How many lines of text a list shows: runs of rows that hold pixels unlike the list ground (left part, no bands)."""
    px = im.load()
    x0, x1 = box[0] + int(6 * s), box[2] - int(26 * s)
    ground = px[x1, box[1] + int(3 * s)] if False else None
    cols = {}
    for y in range(box[1] + 4, box[3] - 4, 3):
        for x in range(x0, x1, 3):
            cols[px[x, y]] = cols.get(px[x, y], 0) + 1
    ground = max(cols, key=cols.get)
    runs = 0
    inrun = False
    for y in range(box[1] + 4, box[3] - 4):
        dark = any(lum(px[x, y]) < 0.25 and px[x, y] != ground for x in range(x0, x1))
        if dark and not inrun:
            runs += 1
        inrun = dark
    return runs


def ink(im, box, s):
    """The number of pixels in the sample box that are not its ground."""
    px = im.load()
    x0, y0, x1, y1 = box[0] + int(5 * s), box[1] + int(5 * s), box[2] - int(5 * s), box[3] - int(5 * s)
    g = px[x0 + 1, y0 + 1]
    n = 0
    xmin = ymin = 10 ** 9
    xmax = ymax = -1
    for y in range(y0, y1):
        for x in range(x0, x1):
            if px[x, y] != g:
                n += 1
                xmin, xmax, ymin, ymax = min(xmin, x), max(xmax, x), min(ymin, y), max(ymax, y)
    return n, (xmin, ymin, xmax, ymax)


def lum(c):
    def ch(v):
        v /= 255.0
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    return 0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])


def contrast(a, b):
    la, lb = lum(a), lum(b)
    if la < lb:
        la, lb = lb, la
    return (la + 0.05) / (lb + 0.05)


def same_image(a, b, box):
    return a.crop(box).tobytes() == b.crop(box).tobytes()


# ================================================================ R1: the long walk, light
print("== 1. light: look, the sample, the mouse, OK")
r = Run(["Test Bar", "12"])
check("the window opens", r.win is not None and r.alive())
if r.win:
    s = r.scale
    im0 = r.shot("live-rest.png")
    P = find_parts(im0, s)
    check("the lists, entries, sample box, check boxes and buttons are found in the picture", P is not None, P)
    if P:
        field, bd = bands(im0, P["fam"], s)
        check("the family list shows its chosen row", len(bd) == 1, bd)
        check("... and the style list its one row", len(bands(im0, P["sty"], s)[1]) == 1)
        check("... and the size list the chosen 12", len(bands(im0, P["siz"], s)[1]) == 1)
        base = im0.getpixel((2, 2))
        check("the window ground is a light colour", lum(base) > 0.5, base)
        # the text of a list on its ground
        px = im0.load()
        fb = P["fam"]
        cols = {}
        for y in range(fb[1] + 4, fb[3] - 4):
            for x in range(fb[0] + 8, fb[0] + int(120 * s)):
                cols[px[x, y]] = cols.get(px[x, y], 0) + 1
        ground = max(cols, key=cols.get)
        darkest = min(cols, key=lambda c: lum(c))
        check("light: the list text has a contrast of at least 4.5 : 1 on its ground", contrast(darkest, ground) >= 4.5,
              contrast(darkest, ground))
        ink0, bb0 = ink(im0, P["preview"], s)
        check("the sample shows ink (Test Bar: thin bars)", ink0 > 50, ink0)
        # hover
        sel = bd[0]
        rowh = round(24 * s)
        hy = sel[1] + rowh // 2
        hx = fb[0] + int(40 * s)
        r.move(hx, hy)
        im1 = r.poll(lambda im: len(bands(im, fb, s)[1]) == 2, name="live-hover.png")
        f1, bd1 = bands(im1, fb, s)
        check("hover paints the row under the pointer in a colour of its own", len(bd1) == 2 and any(
            b[2] != sel[2] and b[2] != f1 for b in bd1), bd1)
        px_x = fb[2] - int(30 * s)
        check("... and the same row is the plain field colour at rest", im0.getpixel((px_x, hy)) == f1 and
              im1.getpixel((px_x, hy)) != f1, (im0.getpixel((px_x, hy)), im1.getpixel((px_x, hy)), f1))
        # click that row: Test Box
        r.click(hx, hy)
        im2 = r.poll(lambda im: ink(im, P["preview"], s)[0] > ink0 * 3, name="live-focus.png")
        ink2, bb2 = ink(im2, P["preview"], s)
        check("a click on the row below chooses Test Box: the sample is drawn with ITS face (squares carry far more ink "
              "than bars)", ink2 > ink0 * 3, (ink0, ink2))
        top0 = [im0.getpixel((fb[0] + int(60 * s), fb[1] + 1))]
        top2 = [im2.getpixel((fb[0] + int(60 * s), fb[1] + 1))]
        check("the focus ring appears around the list that was clicked", top0 != top2, (top0, top2))
        check("the style list of Test Box shows five lines of text (Regular, Italic, Bold, Bold Italic, Light)",
              text_lines(im2, P["sty"], s) == 5, text_lines(im2, P["sty"], s))
        check("the style list of Test Bar showed one", text_lines(im0, P["sty"], s) == 1, text_lines(im0, P["sty"], s))
        # the filter field
        r.click(*mid(P["filter"]))
        r.type("test b")
        im3 = r.poll(lambda im: text_lines(im, fb, s) == 2, name="live-filter.png")
        f3, bd3 = bands(im3, fb, s)
        check("typing 'test b' into the filter narrows the family list to two lines (Test Bar, Test Box)",
              text_lines(im3, fb, s) == 2, text_lines(im3, fb, s))
        check("... the chosen row (Test Box) is the second one", len(bd3) >= 1 and abs(bd3[0][0] - (fb[1] + int(2 * s) + rowh)) <= 3 * s,
              bd3)
        # choose Test Bar by the first row, then Test Box by the second
        r.click(fb[0] + int(40 * s), fb[1] + int(2 * s) + int(0.5 * rowh))
        imb = r.poll(lambda im: ink(im, P["preview"], s)[0] < ink0 * 2)
        check("its first row is Test Bar (thin bars again)", ink(imb, P["preview"], s)[0] < ink0 * 2, ink(imb, P["preview"], s)[0])
        r.click(fb[0] + int(40 * s), fb[1] + int(2 * s) + int(1.5 * rowh))
        im3b = r.poll(lambda im: ink(im, P["preview"], s)[0] > ink0 * 3)
        ink3, bb3 = ink(im3b, P["preview"], s)
        check("... and its second row is Test Box", ink3 > ink0 * 3, (ink0, ink3))
        # the size field
        r.click(*mid(P["sizeentry"]))
        r.key("ctrl+a")
        r.type("48")
        im4 = r.poll(lambda im: ink(im, P["preview"], s)[0] > ink3 * 5, name="live-size48.png")
        ink4, bb4 = ink(im4, P["preview"], s)
        check("size 48 typed into the size field: the sample is much bigger than at 12", ink4 > ink3 * 5, (ink3, ink4))
        check("... and its height grows (the ink box of 48 is at least 2x the one of 12)",
              (bb4[3] - bb4[1]) > (bb3[3] - bb3[1]) * 2, (bb3, bb4))
        # Bold
        c0 = im4.getpixel(mid(P["bold"]))
        r.click(*mid(P["bold"]))
        im5 = r.poll(lambda im: im.getpixel(mid(P["bold"])) != c0, name="live-bold.png")
        check("the Bold check box turns on (its box changes colour)", im5.getpixel(mid(P["bold"])) != c0)
        sty0 = bands(im4, P["sty"], s)[1]
        sty5 = bands(im5, P["sty"], s)[1]
        check("... and the style list moves its chosen row from Regular down to Bold (two rows)",
              sty0 and sty5 and sty5[0][0] - sty0[0][0] >= int(1.5 * rowh), (sty0, sty5))
        # Underline
        r.click(*mid(P["under"]))
        im6 = r.poll(lambda im: ink(im, P["preview"], s)[0] > ink4 + 100, name="live-underline.png")
        ink6, bb6 = ink(im6, P["preview"], s)
        check("Underline adds a line below the text", ink6 > ink4 + 100 and bb6[3] > bb4[3], (ink4, ink6, bb4, bb6))
        # OK
        r.click(*mid(P["ok"]))
        out = r.finish()
        check("OK: the driver answers exactly", out == "STATUS ok\nVALUE family=Test Box size=48 bold=1 italic=0 underline=1 "
              "strike=0\n", out)
    else:
        r.kill()
else:
    r.kill()

# ================================================================ R2: dark, Esc
print("== 2. dark, Esc")
env = dict(ENV, CERTUS_THEME="dark")
r = Run(["DejaVu Sans", "14"], env)
check("the dark window opens", r.win is not None)
if r.win:
    s = r.scale
    im = r.shot("live-dark.png")
    P = find_parts(im, s)
    check("the parts are found (dark)", P is not None)
    base = im.getpixel((2, 2))
    check("the window ground is dark", lum(base) < 0.1, base)
    if P:
        px = im.load()
        fb = P["fam"]
        cols = {}
        for y in range(fb[1] + 4, fb[3] - 4):
            for x in range(fb[0] + 8, fb[0] + int(120 * s)):
                cols[px[x, y]] = cols.get(px[x, y], 0) + 1
        ground = max(cols, key=cols.get)
        lightest = max(cols, key=lambda c: lum(c))
        check("dark: the list text has a contrast of at least 4.5 : 1 on its ground", contrast(lightest, ground) >= 4.5,
              contrast(lightest, ground))
        check("dark: the sample box is drawn in the dark field colour, not in a light one", lum(px[P["preview"][0] + 6, P[
            "preview"][1] + 6]) < 0.3)
        ink_d, bb = ink(im, P["preview"], s)
        check("dark: the sample draws ink (DejaVu Sans)", ink_d > 200, ink_d)
    r.key("Escape")
    out = r.finish()
    check("Esc: the driver says cancelled", out == "STATUS canceled\n", out)

# ================================================================ R3: Enter
print("== 3. Enter = OK, close = cancel, no display")
r = Run(["Test Box", "20"])
if r.win:
    r.key("Return")
    out = r.finish()
    check("Enter without touching anything: OK with what the dialog was given", out == "STATUS ok\nVALUE family=Test Box size=20 "
          "bold=0 italic=0 underline=0 strike=0\n", out)
else:
    check("window", False)
r = Run(["Test Box", "20"])
if r.win:
    r.close_window()
    out = r.finish()
    check("closing the window (WM_DELETE_WINDOW) = cancelled", out == "STATUS canceled\n", out)
envn = dict(ENV)
envn.pop("DISPLAY")
p = subprocess.run([DRIVER, "Test Box", "20"], env=envn, capture_output=True, text=True, timeout=20)
check("no display: no window (the driver reports it, nothing hangs)", p.stdout == "STATUS nowindow\n", p.stdout)

# ================================================================ R4: keys
print("== 4. keyboard")
r = Run(["Test Bar", "12"])
if r.win:
    s = r.scale
    im = r.shot()
    P = find_parts(im, s)
    if P:
        fb = P["fam"]
        field, bd = bands(im, fb, s)
        # a click on the chosen row gives the list the focus (and keeps the family)
        r.click(fb[0] + int(40 * s), (bd[0][0] + bd[0][1]) // 2)
        r.key("End")
        r.key("Prior")
        rowh = round(24 * s)
        inner = round(172 * s) - 2 * max(1, round(2 * s))      # the list's design height less its frame
        page = max(1, int(inner / rowh) - 1)
        want = NAMES[len(NAMES) - 1 - page]
        r.key("Return")
        out = r.finish()
        check("End, then PageUp (a page up through the real X key), Enter: " + want, out is not None and (
            "family=%s " % want) in out, out)
    else:
        r.kill()
        check("parts", False)
r = Run(["Test Box", "12"])
if r.win:
    s = r.scale
    im = r.shot()
    P = find_parts(im, s)
    if P:
        fb = P["fam"]
        field, bd = bands(im, fb, s)
        r.click(fb[0] + int(40 * s), (bd[0][0] + bd[0][1]) // 2)
        r.type("Test C")
        r.key("Return")
        out = r.finish()
        check("type-ahead: 'Test C' in the list jumps to the first family that starts with it (Test CFF)", out is not None and
              "family=Test CFF " in out, out)
    else:
        r.kill()
        check("parts", False)
r = Run(["Test Box", "12"])
if r.win:
    s = r.scale
    im = r.shot()
    P = find_parts(im, s)
    if P:
        r.click(*mid(P["filter"]))
        r.type("zzzz")
        im2 = r.shot("live-nomatch.png")
        r.key("Return")
        time.sleep(0.6)
        check("a filter that matches nothing: Enter does nothing, the dialog stays open", r.alive())
        r.key("Escape")
        out = r.finish()
        check("... and Esc still cancels", out == "STATUS canceled\n", out)
    else:
        r.kill()
        check("parts", False)

# ================================================================ R5: the wheel and the real faces
print("== 5. wheel, real faces, 999")
r = Run(["DejaVu Serif", "14"])
if r.win:
    s = r.scale
    im0 = r.shot()
    P = find_parts(im0, s)
    if P:
        ink_serif, bbs = ink(im0, P["preview"], s)
        fb = P["fam"]
        # DejaVu Sans is the first row: scroll the list to the top, click it
        r.wheel(fb[0] + int(40 * s), fb[1] + int(40 * s), False, 3)
        im1 = r.poll(lambda im: not same_image(im0, im, fb), name="live-wheel.png")
        check("the wheel scrolls the list under the pointer (the list picture changed)", not same_image(im0, im1, fb))
        r.click(fb[0] + int(40 * s), fb[1] + int(2 * s) + int(0.5 * round(24 * s)))
        im2 = r.poll(lambda im: not same_image(im0, im, P["preview"]))
        ink_sans, bbsans = ink(im2, P["preview"], s)
        check("DejaVu Sans and DejaVu Serif draw differently in the sample (the real faces)", ink_sans != ink_serif and
              not same_image(im0, im2, P["preview"]), (ink_serif, ink_sans))
        check("DejaVu Sans (as the test folder has it) has two faces: the style list shows two lines (Regular, Bold)",
              text_lines(im2, P["sty"], s) == 2, text_lines(im2, P["sty"], s))
        # 999
        r.click(*mid(P["sizeentry"]))
        r.key("ctrl+a")
        r.type("5000")
        im3 = r.poll(lambda im: ink(im, P["preview"], s)[0] > 1000, name="live-size999.png")
        ink9, bb9 = ink(im3, P["preview"], s)
        pv = P["preview"]
        check("5000 is clamped to 999 and the sample stays inside its box (capped, clipped)",
              bb9[1] >= pv[1] + 2 and bb9[3] <= pv[3] - 2 and bb9[0] >= pv[0] + 2 and bb9[2] <= pv[2] - 2 and ink9 > 1000, bb9)
        r.key("Return")
        out = r.finish()
        check("... and the answer says 999 for the family that was chosen", out is not None and "size=999" in out and
              "family=DejaVu Sans " in out, out)
    else:
        r.kill()
        check("parts", False)

# ================================================================ R6: no fonts
print("== 6. a folder without fonts")
env = dict(ENV, FUI_FONT_DIRS="/nonexistent-font-folder")
r = Run(["Whatever", "12"], env)
check("the dialog opens", r.win is not None)
if r.win:
    s = r.scale
    im = r.shot("live-nofonts.png")
    P = find_parts(im, s)
    check("the parts are found", P is not None)
    if P:
        check("the family list has exactly one row (the system face), chosen", len(bands(im, P["fam"], s)[1]) == 1)
        ink_n, bb = ink(im, P["preview"], s)
        check("the sample is drawn", ink_n > 100, ink_n)
        r.click(*mid(P["ok"]))
        out = r.finish()
        check("OK still works: the system face", out is not None and "family=DejaVu Sans " in out, out)
    else:
        r.kill()

cleanup()
print()
if FAILED:
    print("FONTDLG LIVE FAILED (%d): %s" % (len(FAILED), "; ".join(FAILED)))
    sys.exit(1)
print("FONTDLG LIVE PASSED")

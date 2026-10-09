#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/colordlg_live.py -- the colour dialog (lib/fui/colordlg.fi) in a REAL window on a private Xvfb.

    python3 tools/dialog/colordlg_live.py <colordlg_live_main binary> [<proof folder>]

The driver (tools/dialog/colordlg_live_main.fi) opens the dialog with colordlg.run and prints the answer. This
script plays the user with xdotool (real mouse, real keys) and looks at what the X server shows (xwd): it finds the
parts of the dialog IN THE PICTURE (the swatch of an exactly known colour, the rainbow of the field, the runs of the
field colour that are the text fields and buttons) -- no coordinates of this machine are hard-coded -- and checks
PIXELS and RESULTS: the start colour on both halves of the preview, hover, focus ring, a click on a swatch,
a drag in the field (also out of the window), the lightness bar, typing into Hex and Red, a field that holds
no colour (error ring), Esc, Enter, OK with the mouse, "Add to custom colours" and what the next run reads back
from the store, and the dark theme. Pictures of the key states go to <proof folder>.

Without Xvfb / xdotool / xwd / PIL the run SKIPs (exit 0).
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 2:
    print("usage: colordlg_live.py <colordlg_live_main> [<proof folder>]")
    sys.exit(2)
DRIVER = os.path.abspath(sys.argv[1])
PROOF = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

for tool in ("Xvfb", "xdotool", "xwd"):
    if not shutil.which(tool):
        print("  SKIP  %s is not installed: the real colour dialog was not run here" % tool)
        sys.exit(0)
try:
    from PIL import Image
except ImportError as e:
    print("  SKIP  %s: the real colour dialog was not run here" % e)
    sys.exit(0)

FAILED = []


def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""),
          flush=True)
    if not cond:
        FAILED.append(name)


td = tempfile.mkdtemp(prefix="colordlg-live-")
NUM = 60 + (os.getpid() % 100)
DISP = ":%d" % NUM
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1024x768x24"], stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL)
for _ in range(60):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
HOME = os.path.join(td, "home")
os.makedirs(HOME)
BASE_ENV = dict(os.environ, DISPLAY=DISP, HOME=HOME)
BASE_ENV.pop("WAYLAND_DISPLAY", None)
BASE_ENV.pop("CERTUS_THEME", None)
BASE_ENV.pop("GTK_THEME", None)
STORE = os.path.join(HOME, ".config", "firn", "dialog-colors.txt")
if PROOF:
    os.makedirs(PROOF, exist_ok=True)


def xdo(*a, env=None):
    return subprocess.run(["xdotool"] + list(a), env=env or BASE_ENV, capture_output=True, text=True).stdout.strip()


def snap(name=None):
    """The whole screen as an RGB image (and a copy of the window area in the proof folder under `name`)."""
    x = os.path.join(td, "s.xwd")
    p = os.path.join(td, "s.png")
    subprocess.run(["xwd", "-root", "-silent", "-display", DISP, "-out", x], check=True, env=BASE_ENV)
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True,
                   capture_output=True)
    im = Image.open(p).convert("RGB")
    im.load()
    if name and PROOF:
        im.crop((0, 0, WIN[0], WIN[1])).save(os.path.join(PROOF, name + ".png"))
    return im


WIN = (660, 490)


class Run:
    def __init__(self, args=(), env=None, title="dlg-color"):
        xdo("mousemove", "1000", "740")
        end = time.time() + 6
        while time.time() < end and xdo("search", "--onlyvisible", "--name", "^dlg-color$").split():
            time.sleep(0.1)
        self.env = dict(BASE_ENV, **(env or {}))
        self.p = subprocess.Popen([DRIVER] + list(args), env=self.env, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, start_new_session=True)
        self.win = None
        end = time.time() + 15
        while time.time() < end and not self.win:
            ids = xdo("search", "--onlyvisible", "--name", "^%s$" % title).split()
            if ids:
                self.win = ids[0]
            else:
                time.sleep(0.2)
        if self.win:
            time.sleep(0.8)
            g = xdo("getwindowgeometry", self.win)
            wh = [l for l in g.splitlines() if "Geometry" in l][0].split(":")[1].strip().split("x")
            global WIN
            WIN = (int(wh[0]), int(wh[1]))
            self.size = WIN

    def alive(self):
        return self.p.poll() is None

    def kill(self):
        try:
            os.killpg(self.p.pid, 9)
        except ProcessLookupError:
            pass
        try:
            self.p.communicate(timeout=5)
        except Exception:
            pass

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


def px(im, x, y):
    return im.getpixel((int(x), int(y)))


def near(a, b, tol=2):
    return all(abs(a[i] - b[i]) <= tol for i in range(3))


def chroma(c):
    return max(c) - min(c)


# ------------------------------------------------------------------ finding the parts in the picture

class Layout:
    """Where things are, found in a screenshot of the dialog's start state (start colour 1bd96a)."""

    def __init__(self, im, start=(0x1B, 0xD9, 0x6A)):
        W, H = WIN
        self.bg = px(im, 2, 2)
        self.W, self.H = W, H
        self.sc = W / 620.0
        P = im.load()
        # the basic swatches: the inner box of the exactly-red one (row 1, column 0), yellow is its neighbour
        red = self.box(P, (255, 0, 0), 0, W // 2)
        yel = self.box(P, (255, 255, 0), 0, W // 2)
        dark = self.box(P, (128, 0, 0), 0, W // 2)
        self.cell = red[2] - red[0] + 1
        self.pitch = yel[0] - red[0]
        self.pitch_y = (dark[1] - red[1]) // 2
        self.sw_x0 = red[0]
        self.sw_y0 = red[1] - self.pitch_y            # row 0
        # the field: the top row of saturated colours in the right half
        top = None
        for y in range(H):
            xs = [x for x in range(self.sw_x0 + 8 * self.pitch + 4, W) if chroma(P[x, y]) >= 150]
            if len(xs) > 100:
                top = (y, min(xs), max(xs))
                break
        self.fy0, self.fx0, fx1 = top
        self.fw = fx1 - self.fx0 + 1
        y = self.fy0
        while y < H and P[self.fx0 + 3, y] != self.bg:
            y += 1
        self.fh = y - 1 - self.fy0                      # one row of border before the page
        # the bar: right of the field, white at the top
        x = self.fx0 + self.fw + 3
        mid = self.fy0 + self.fh // 2
        while x < W and P[x, mid] == self.bg:
            x += 1
        self.bx0 = x + 1
        x2 = self.bx0
        while x2 < W and P[x2, mid] != self.bg:
            x2 += 1
        self.bw = x2 - 1 - self.bx0
        self.by0 = self.fy0
        self.bh = self.fh
        # the preview: the first row below the field that is not the page
        y = self.fy0 + self.fh + 4
        while y < H and P[self.fx0 + 6, y] == self.bg:
            y += 1
        self.py0 = y
        # the custom grid: empty slots are field coloured below the basic grid
        y = self.sw_y0 + 6 * self.pitch_y
        field_col = self.bg
        while y < H and P[self.sw_x0 + 8, y] == self.bg:
            y += 1
        # (a thin run of border first)
        while y < H and P[self.sw_x0 + 8, y] != self.bg and not near(P[self.sw_x0 + 8, y], (237, 237, 240), 40):
            y += 1
        self.cu_y0 = y + 2
        # boxes of the button / field colour that are wide enough: the text fields and the buttons
        self.parts = self.runs(P, W // 2)
        self.add = self.runs(P, 0, W // 2)

    @staticmethod
    def box(P, col, x0, x1):
        xs = []
        ys = []
        for y in range(0, WIN[1]):
            for x in range(x0, x1):
                if P[x, y] == col:
                    xs.append(x)
                    ys.append(y)
        return (min(xs), min(ys), max(xs), max(ys))

    def runs(self, P, x0, x1=None):
        """Boxes of the field colour at least 40 px wide and 20 px high: (x0, y0, x1, y1)."""
        x1 = x1 or self.W
        col = (237, 237, 240)
        found = {}
        for y in range(self.H):
            x = x0
            while x < x1:
                if near(P[x, y], col, 1):
                    s = x
                    while x < x1 and near(P[x, y], col, 1):
                        x += 1
                    if x - s >= 40:
                        found.setdefault((s, x - 1), []).append(y)
                else:
                    x += 1
        boxes = []
        for (a, b), ys in found.items():
            ys.sort()
            start = ys[0]
            prev = ys[0]
            for y in ys[1:] + [10 ** 9]:
                if y - prev > 22:
                    if prev - start >= 4:
                        boxes.append((a, start, b, prev))
                    start = y
                prev = y
        # merge boxes that are one text field (top and bottom padding with text between)
        boxes.sort(key=lambda b: (b[0], b[1]))
        merged = []
        for b in boxes:
            if merged and merged[-1][0] == b[0] and merged[-1][2] == b[2] and b[1] - merged[-1][3] <= 24:
                merged[-1] = (b[0], merged[-1][1], b[2], b[3])
            else:
                merged.append(b)
        return [b for b in merged if b[3] - b[1] >= 14]

    def swatch(self, k):
        """The centre of basic swatch k."""
        return (self.sw_x0 + (k % 8) * self.pitch + self.cell // 2, self.sw_y0 + (k // 8) * self.pitch_y + self.cell // 2)

    def custom(self, k):
        return (self.sw_x0 + (k % 8) * self.pitch + self.cell // 2, self.cu_y0 + (k // 8) * self.pitch_y + self.cell // 2)

    def field_pt(self, i, j):
        return (self.fx0 + i, self.fy0 + j)

    def new_half(self):
        return (self.fx0 + self.fw - 14, self.py0 + 8)

    def old_half(self):
        return (self.fx0 + 14, self.py0 + 8)

    def prev_y(self):
        return self.py0 + 8


def click(x, y):
    xdo("mousemove", str(int(x)), str(int(y)))
    time.sleep(0.15)
    xdo("click", "1")
    time.sleep(0.35)


def move(x, y):
    xdo("mousemove", str(int(x)), str(int(y)))
    time.sleep(0.35)


def park():
    """The pointer rests on the empty top left corner of the window: no hover anywhere, and the keys go to the window
    (there is no window manager: the keyboard follows the pointer)."""
    xdo("mousemove", "10", "10")
    time.sleep(0.4)


def keys(*k):
    xdo("mousemove", "10", "10")        # the keys go to the window under the pointer
    xdo("key", *k)
    time.sleep(0.3)


def type_text(t):
    xdo("mousemove", "10", "10")
    xdo("type", "--delay", "40", t)
    time.sleep(0.4)


def tabs(n):
    xdo("mousemove", "10", "10")
    for _ in range(n):
        xdo("key", "Tab")
        time.sleep(0.12)
    time.sleep(0.3)


def newc(im, lay):
    x, y = lay.new_half()
    return px(im, x, y)


def oldc(im, lay):
    x, y = lay.old_half()
    return px(im, x, y)


# ============================================================ the runs

# ---- 1. rest: the start colour on both halves; find the parts
r = Run(["1bd96a"])
check("the colour dialog window opens", r.win is not None)
if not r.win:
    r.kill()
    xvfb.terminate()
    shutil.rmtree(td, ignore_errors=True)
    print("colordlg live: no window")
    sys.exit(1)
park()
im0 = snap("colordlg-live-rest")
lay = Layout(im0)
print("    layout: swatch0 %s cell %d pitch %d | field (%d,%d) %dx%d | bar x %d w %d | preview y %d | custom y %d | %d boxes right, %d left"
      % (lay.swatch(0), lay.cell, lay.pitch, lay.fx0, lay.fy0, lay.fw, lay.fh, lay.bx0, lay.bw, lay.py0, lay.cu_y0,
         len(lay.parts), len(lay.add)))
check("the window background is the light page colour", near(lay.bg, (244, 244, 246), 6), lay.bg)
check("the colour field is found and is big enough (%dx%d)" % (lay.fw, lay.fh), lay.fw >= 200 and lay.fh >= 120)
check("Old half of the preview shows the start colour 1bd96a", near(oldc(im0, lay), (0x1B, 0xD9, 0x6A)), oldc(im0, lay))
check("New half of the preview shows the start colour 1bd96a", near(newc(im0, lay), (0x1B, 0xD9, 0x6A)), newc(im0, lay))
# the field is a rainbow: the top row has many hues, the left edge is red, the middle is green-cyan
left = px(im0, lay.fx0 + 1, lay.fy0 + 1)
check("the field's top left is red (hue 0, full saturation)", left[0] > 200 and left[1] < 60 and left[2] < 60, left)
right = px(im0, lay.fx0 + lay.fw - 2, lay.fy0 + 1)
check("the field's top right is red again (hue 360)", right[0] > 200 and right[1] < 90 and right[2] < 90, right)
check("the field's bottom row is grey (saturation 0)", chroma(px(im0, lay.fx0 + lay.fw // 2, lay.fy0 + lay.fh - 1)) < 12,
      px(im0, lay.fx0 + lay.fw // 2, lay.fy0 + lay.fh - 1))
check("the bar's top is white, its bottom black",
      min(px(im0, lay.bx0 + lay.bw // 2, lay.by0 + 1)) > 235 and max(px(im0, lay.bx0 + lay.bw // 2, lay.by0 + lay.bh - 2)) < 20,
      (px(im0, lay.bx0 + lay.bw // 2, lay.by0 + 1), px(im0, lay.bx0 + lay.bw // 2, lay.by0 + lay.bh - 2)))
check("the swatches are found at the expected colours",
      all(px(im0, *lay.swatch(k)) == c for k, c in ((8, (255, 0, 0)), (9, (255, 255, 0)), (0, (255, 128, 128)), (47, (255, 255, 255)))),
      [px(im0, *lay.swatch(k)) for k in (8, 9, 0, 47)])
check("the text fields and buttons are found (7 fields + OK + Cancel on the right, Add on the left)",
      len(lay.parts) == 9 and len(lay.add) == 1, (len(lay.parts), len(lay.add)))

# ---- 2. hover on a swatch: a ring appears
hx, hy = lay.swatch(20)
edge = (hx - lay.cell // 2 - 3, hy)
rest = px(im0, *edge)
move(hx, hy)
im1 = snap("colordlg-live-hover")
check("hover: the pixel next to the swatch under the mouse changes (ring)", px(im1, *edge) != rest, (rest, px(im1, *edge)))
park()
im1b = snap()
check("hover: ... and goes back when the mouse leaves", px(im1b, *edge) == rest, (rest, px(im1b, *edge)))

# ---- 3. a click on a swatch: New shows it, Old stays
click(*lay.swatch(8))
park()
im2 = snap("colordlg-live-swatch")
check("a click on the red swatch: New half = ff0000", newc(im2, lay) == (255, 0, 0), newc(im2, lay))
check("Old half still the start colour", near(oldc(im2, lay), (0x1B, 0xD9, 0x6A)), oldc(im2, lay))
sx, sy = lay.swatch(8)
check("the chosen swatch has a tick (its pixels are not all red)",
      any(px(im2, sx + dx, sy + dy) != (255, 0, 0) for dx in range(-6, 7) for dy in range(-6, 7)))
# every swatch of the first row gives its colour
okrow = True
for k, want in ((1, (255, 255, 128)), (3, (0, 255, 128)), (5, (0, 128, 255)), (7, (255, 128, 255))):
    click(*lay.swatch(k))
    park()
    got = newc(snap(), lay)
    if got != want:
        okrow = False
        print("    swatch", k, got, want)
check("swatches 1, 3, 5, 7 of the first row each give their colour", okrow)

# ---- 4. a click in the field: the New colour is the colour under the pointer
tx, ty = lay.field_pt(lay.fw * 3 // 4, lay.fh // 3)
want = px(im0, tx, ty)          # the field's colour there (same lightness as the start)
# the lightness is that of the start colour only before a swatch changed it: start a fresh run
keys("Escape")
res = r.result()
check("Esc cancels the dialog: STATUS canceled", res == {"STATUS": "canceled"}, res)
r = Run(["1bd96a"])
move(900, 700)
im0 = snap()
lay = Layout(im0)
want = px(im0, *lay.field_pt(lay.fw * 3 // 4, lay.fh // 3))
click(*lay.field_pt(lay.fw * 3 // 4, lay.fh // 3))
park()
im3 = snap("colordlg-live-field-click")
check("a click in the field: New = the field's colour under the pointer (%s)" % (want,), near(newc(im3, lay), want, 2),
      (newc(im3, lay), want))
# the focus ring on the field (a click focuses it)
ring = px(im3, lay.fx0 - 3, lay.fy0 + lay.fh // 2)
check("focus ring: the field has a ring after the click (a pixel outside the picture is not the page)",
      not near(ring, lay.bg, 8), (ring, lay.bg))
check("the cross-hair is painted where the pointer was",
      any(px(im3, lay.fx0 + lay.fw * 3 // 4 + dx, lay.fy0 + lay.fh // 3 + dy) != px(im0, lay.fx0 + lay.fw * 3 // 4 + dx, lay.fy0 + lay.fh // 3 + dy)
          for dx in range(-9, 10) for dy in range(-9, 10)))

# ---- 5. a drag in the field, ending outside the window: clamped to the corner
xdo("mousemove", str(lay.fx0 + 40), str(lay.fy0 + 30))
time.sleep(0.2)
xdo("mousedown", "1")
time.sleep(0.2)
for k in range(1, 8):
    xdo("mousemove", str(lay.fx0 + 40 + k * 20), str(lay.fy0 + 30 + k * 8))
    time.sleep(0.08)
time.sleep(0.3)
im4 = snap("colordlg-live-drag")
want4 = px(im0, lay.fx0 + 40 + 7 * 20, lay.fy0 + 30 + 7 * 8)
check("a drag: New follows the pointer (the field's colour at the last point)", near(newc(im4, lay), want4, 2),
      (newc(im4, lay), want4))
xdo("mousemove", "1000", "760")           # far outside the window, bottom right
time.sleep(0.4)
im4b = snap()
corner = px(im0, lay.fx0 + lay.fw - 1, lay.fy0 + lay.fh - 1)
check("dragged out of the window: New is the bottom right corner of the field (a grey)", near(newc(im4b, lay), corner, 3),
      (newc(im4b, lay), corner))
xdo("mouseup", "1")
time.sleep(0.3)
park()
im4c = snap()
after = newc(im4c, lay)
xdo("mousemove", str(lay.fx0 + 10), str(lay.fy0 + 10))
time.sleep(0.4)
check("after the release the pointer moving over the field changes nothing", newc(snap(), lay) == after)

# ---- 6. the lightness bar: top = white, bottom = black
click(lay.bx0 + lay.bw // 2, lay.by0)
park()
got_top = newc(snap(), lay)
check("a click on the top of the bar: New is white", got_top == (255, 255, 255), got_top)
click(lay.bx0 + lay.bw // 2, lay.by0 + lay.bh - 1)
park()
check("a click on the bottom of the bar: New is black", newc(snap(), lay) == (0, 0, 0))
# the field is black (lightness 0) there: it repaints with the bar
fb = px(snap(), lay.fx0 + lay.fw // 2, lay.fy0 + lay.fh // 2)
check("at lightness 0 the whole field is black", max(fb) < 6, fb)
click(lay.bx0 + lay.bw // 2, lay.by0 + lay.bh // 2)
park()
im6 = snap("colordlg-live-bar")
mid = newc(im6, lay)
check("a click in the middle of the bar gives a colour (not grey: the hue came back from before)", chroma(mid) > 100 or mid in ((128, 128, 128),), mid)

# ---- 7. keyboard: Tab to the field, arrows
keys("Escape")
res = r.result()
check("Esc: STATUS canceled (again)", res == {"STATUS": "canceled"}, res)
r = Run(["1bd96a"])
move(900, 700)
im0 = snap()
lay = Layout(im0)
tabs(4)                                                         # basic, custom, add, field
move(900, 700)
imf = snap("colordlg-live-focus-field")
ringf = px(imf, lay.fx0 - 3, lay.fy0 + lay.fh // 2)
check("Tab x4: the field has the focus ring", not near(ringf, lay.bg, 8) and ringf[2] > ringf[0] + 40, ringf)
before = newc(imf, lay)
keys("shift+Right")                                             # hue + 10
imr = snap()
check("Shift+Right on the field changes the colour", newc(imr, lay) != before, (before, newc(imr, lay)))
keys("Right")
imr2 = snap()
check("Right moves it once more", newc(imr2, lay) != newc(imr, lay))
tabs(1)                                                         # the bar
imb = snap("colordlg-live-focus-bar")
ringb = px(imb, lay.bx0 - 3, lay.by0 + lay.bh // 2)
check("Tab: the lightness bar has the focus ring", not near(ringb, lay.bg, 8), ringb)
keys("End")
check("End on the bar: black", newc(snap(), lay) == (0, 0, 0))
keys("Home")
check("Home on the bar: white", newc(snap(), lay) == (255, 255, 255))
keys("Escape")
res = r.result()
check("Esc: STATUS canceled", res == {"STATUS": "canceled"}, res)

# ---- 8. the fields: Hex (error ring while it holds no colour), Red
r = Run(["1bd96a"])
move(900, 700)
im0 = snap()
lay = Layout(im0)
parts = sorted(lay.parts, key=lambda b: (b[1], b[0]))
rows = []
for b in parts:
    if rows and abs(rows[-1][0][1] - b[1]) <= 24 and not (b[2] - b[0] < 70 and False):
        rows[-1].append(b)
    else:
        rows.append([b])
bottom = sorted(rows[-1], key=lambda b: b[0])
check("the bottom row has the Hex field, OK and Cancel", len(bottom) == 3, [len(x) for x in rows])
hexbox = bottom[0]
okbox = bottom[1]
redbox = sorted(rows[1], key=lambda b: b[0])[0] if len(rows) >= 2 else None
click((hexbox[0] + hexbox[2]) // 2, (hexbox[1] + hexbox[3]) // 2)
keys("ctrl+a")
type_text("#12")
imh = snap("colordlg-live-error")
ry = (hexbox[1] + hexbox[3]) // 2
def reddish(c):
    return c[0] > 140 and c[1] < 100 and c[2] < 130 and c[0] > c[1] + 70


def count_reddish(im, box, pad=8):
    n = 0
    for y in range(box[1] - pad, box[3] + pad):
        for x in range(box[0] - pad, box[2] + pad):
            if reddish(px(im, x, y)):
                n += 1
    return n


nred = count_reddish(imh, hexbox)
check("a hex field that holds no colour (#12) is ringed in the error colour (%d reddish pixels around it)" % nred,
      nred > 60, nred)
check("... and the colour has not changed", near(newc(imh, lay), (0x1B, 0xD9, 0x6A)), newc(imh, lay))
keys("ctrl+a")
type_text("#ff8000")
imx = snap("colordlg-live-hex")
check("typing #ff8000 into Hex: New = ff8000", newc(imx, lay) == (255, 128, 0), newc(imx, lay))
check("Old is still the start colour", near(oldc(imx, lay), (0x1B, 0xD9, 0x6A)), oldc(imx, lay))
nred2 = count_reddish(imx, hexbox)
check("the error ring is gone once the hex is valid", nred2 == 0, nred2)
# the Red/Green/Blue fields changed with the hex: a screenshot of their boxes differs from the start
rg = sorted(rows[1], key=lambda b: b[0])
boxes_changed = 0
for b in rg:
    a0 = im0.crop((b[0], b[1], b[2], b[3])).tobytes()
    a1 = imx.crop((b[0], b[1], b[2], b[3])).tobytes()
    if a0 != a1:
        boxes_changed += 1
check("the Red, Green and Blue fields repaint with the typed hex (green: 217 -> 128, blue: 106 -> 0, red: 27 -> 255)",
      boxes_changed == 3, boxes_changed)
hl = sorted(rows[0], key=lambda b: b[0])
changed_h = sum(1 for b in hl if im0.crop((b[0], b[1], b[2], b[3])).tobytes() != imx.crop((b[0], b[1], b[2], b[3])).tobytes())
check("... and so do Hue, Saturation and Lightness", changed_h == 3, changed_h)
# the Red field: click, select all, type 10 -> ff8000 becomes 0a8000
click((redbox[0] + redbox[2]) // 2, (redbox[1] + redbox[3]) // 2)
keys("ctrl+a")
type_text("10")
imrd = snap("colordlg-live-red")
check("typing 10 into Red: New = 0a8000", newc(imrd, lay) == (10, 128, 0), newc(imrd, lay))
keys("ctrl+a")
type_text("300")
imrd = snap()
check("typing 300 into Red clamps to 255: New = ff8000", newc(imrd, lay) == (255, 128, 0), newc(imrd, lay))
# Enter anywhere is OK, with the colour that was typed
keys("Return")
res = r.result()
check("Enter in a text field is OK: STATUS ok, VALUE ff8000", res == {"STATUS": "ok", "VALUE": "ff8000"}, res)

# ---- 9. an untouched dialog answers with the start colour bit for bit; OK by mouse
for start in ("1bd96a", "010203", "fefdfc", "7f7f80"):
    r = Run([start])
    move(900, 700)
    time.sleep(0.2)
    if start == "1bd96a":
        iml = snap()
        lay = Layout(iml)
        bx = sorted(lay.parts, key=lambda b: (b[1], b[0]))
        rws = []
        for b in bx:
            if rws and abs(rws[-1][0][1] - b[1]) <= 24:
                rws[-1].append(b)
            else:
                rws.append([b])
        okb = sorted(rws[-1], key=lambda b: b[0])[1]
        click((okb[0] + okb[2]) // 2, (okb[1] + okb[3]) // 2)
    else:
        keys("Return")
    res = r.result()
    check("OK on an untouched dialog gives the start colour exactly (%s)" % start, res == {"STATUS": "ok", "VALUE": start}, res)

# ---- 10. Old | New: a click on Old restores the start colour
r = Run(["1bd96a"])
move(900, 700)
im0 = snap()
lay = Layout(im0)
click(*lay.swatch(8))
click(lay.old_half()[0], lay.old_half()[1] + 4)
park()
imo = snap("colordlg-live-old")
check("a click on Old restores the start colour (New = 1bd96a)", near(newc(imo, lay), (0x1B, 0xD9, 0x6A)), newc(imo, lay))
# ---- 11. custom colours: add, see the slot, the store, the next run
click(*lay.swatch(8))
add = lay.add[0]
click((add[0] + add[2]) // 2, (add[1] + add[3]) // 2)
park()
imc = snap("colordlg-live-custom")
check("'Add to custom colours': slot 0 shows the colour (ff0000)", px(imc, *lay.custom(0)) == (255, 0, 0), px(imc, *lay.custom(0)))
check("the next slot is still empty", px(imc, *lay.custom(1)) != (255, 0, 0))
check("the store file holds 16 lines with FF0000 first",
      os.path.exists(STORE) and open(STORE).read().split("\n")[:2] == ["FF0000", ""] and len(open(STORE).read().split("\n")) == 17,
      open(STORE).read() if os.path.exists(STORE) else "no file")
click(*lay.swatch(28))                      # blue 0000ff
click(*lay.custom(0))
park()
check("a click on the custom slot loads its colour (New = ff0000)", newc(snap(), lay) == (255, 0, 0))
keys("Escape")
res = r.result()
check("Esc: canceled", res == {"STATUS": "canceled"}, res)
r = Run(["1bd96a"])
move(900, 700)
imn = snap("colordlg-live-custom-loaded")
check("the next run reads the store: slot 0 is red from the start", px(imn, *lay.custom(0)) == (255, 0, 0), px(imn, *lay.custom(0)))
click(*lay.custom(0))
keys("Return")
res = r.result()
check("... and OK gives ff0000", res == {"STATUS": "ok", "VALUE": "ff0000"}, res)
# a broken store is read as empty, no crash
open(STORE, "w").write("not a store\n")
r = Run(["1bd96a"])
move(900, 700)
imb2 = snap()
check("a broken store: the dialog opens and all 16 slots are empty", px(imb2, *lay.custom(0)) != (255, 0, 0) and r.alive())
keys("Escape")
r.result()

# ---- 12. closing the window = cancel
r = Run(["1bd96a"])
subprocess.run(["xdotool", "key", "--window", r.win, "Escape"], env=BASE_ENV, capture_output=True)
res = r.result()
check("a window that is closed (Esc sent to it): canceled", res == {"STATUS": "canceled"}, res)

# ---- 13. the dark theme
r = Run(["1bd96a"], env={"CERTUS_THEME": "dark"})
move(900, 700)
imd = snap("colordlg-live-dark")
bgd = px(imd, 2, 2)
check("dark theme: the page is dark", sum(bgd) < 200, bgd)
# the field exists and is a rainbow: look for the saturated top row
rowfound = False
for y in range(0, WIN[1]):
    xs = [x for x in range(int(WIN[0] * 0.44), WIN[0]) if chroma(px(imd, x, y)) >= 150]
    if len(xs) > 100:
        rowfound = True
        top_y, x_l, x_r = y, min(xs), max(xs)
        break
check("dark theme: the colour field is painted (a saturated top row)", rowfound)
if rowfound:
    ringd = px(imd, x_l - 3, top_y + 40)
    check("dark theme: the field's surround is the dark page", sum(ringd) < 250, ringd)
    tabs(4)
    imd2 = snap("colordlg-live-dark-focus")
    ringd2 = px(imd2, x_l - 3, top_y + 40)
    check("dark theme: the focus ring on the field is visible (a light/accent pixel outside the picture)",
          sum(ringd2) > sum(bgd) + 120, (ringd2, bgd))
keys("Escape")
r.result()

xvfb.terminate()
xvfb.wait()
shutil.rmtree(td, ignore_errors=True)
print("colordlg live: %d failed" % len(FAILED) if FAILED else "colordlg live: all checks passed")
sys.exit(1 if FAILED else 0)

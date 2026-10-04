#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/kitlive.py -- THE LAUNCHER KIT IN A REAL WINDOW ON A REAL X SERVER.
#
# tools/fui/kit_main.fi proves every part against the canvas in memory. This
# run proves the same program the way a user meets it: examples/fui/launcher_kit
# runs on its own Xvfb, is operated from OUTSIDE with xdotool (mouse and keys),
# and what is checked is what the SERVER shows (xwd), pixel by pixel, plus the
# CPU the process used while it was idle (/proc/<pid>/stat) -- the demo asks
# for frames only while something moves.
#
#     python3 tools/fui/kitlive.py <launcher_kit program> <proof folder>
#
# Without Xvfb / xdotool / xwd on the machine: "SKIP" with the reason, exit 0 --
# a missing X server is no error of the kit. Every other deviation: "WRONG",
# exit 1.
import os, shutil, subprocess, sys, time

PROG = sys.argv[1]
BELEG = sys.argv[2]
os.makedirs(BELEG, exist_ok=True)
HERE = os.path.dirname(os.path.abspath(__file__))
for tool in ("Xvfb", "xdotool", "xwd"):
    if shutil.which(tool) is None:
        print("SKIP: no %s -- no real window in this run" % tool)
        sys.exit(0)
try:
    from PIL import Image
except Exception:
    print("SKIP: no PIL (python3-pil) -- the pictures cannot be read")
    sys.exit(0)

bad = 0
total = 0


def chk(text, ok, got="", want=""):
    global bad, total
    total += 1
    if not ok:
        bad += 1
    print("  %-72s%s %s %s" % (text, "OK   " if ok else "WRONG", got, want), flush=True)


DISP = ":%d" % (100 + os.getpid() % 800)
env = dict(os.environ, DISPLAY=DISP)
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1100x720x24"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1.0)
app = None


def finish(code):
    if app is not None:
        app.terminate()
        try:
            app.wait(3)
        except Exception:
            app.kill()
    xvfb.terminate()
    try:
        xvfb.wait(3)
    except Exception:
        xvfb.kill()
    sys.exit(code)


def xdo(*args):
    subprocess.run(["xdotool"] + list(args), env=env, check=False)


def shot(name):
    xwd = os.path.join(BELEG, name + ".xwd")
    with open(xwd, "wb") as f:
        subprocess.run(["xwd", "-root", "-silent", "-display", DISP], stdout=f, check=True)
    png = os.path.join(BELEG, name + ".png")
    subprocess.run([sys.executable, os.path.join(HERE, "xwd2png.py"), xwd, png],
                   check=True, stdout=subprocess.DEVNULL)
    os.remove(xwd)
    return Image.open(png).convert("RGB")


def rgb(h):
    return ((h >> 16) & 255, (h >> 8) & 255, h & 255)


def near(a, b, tol=3):
    return all(abs(x - y) <= tol for x, y in zip(a, b))


def count(im, box, col, tol=2):
    n = 0
    x0, y0, x1, y1 = box
    px = im.load()
    for y in range(y0, y1):
        for x in range(x0, x1):
            if near(px[x, y], col, tol):
                n += 1
    return n


def ink(im, box):
    # pixels that differ clearly from the box's corner pixel (text on a plain ground)
    x0, y0, x1, y1 = box
    px = im.load()
    bg = px[x0, y0]
    lb = sum(bg) / 3.0
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            if abs(sum(px[x, y]) / 3.0 - lb) > 80:
                n += 1
    return n


def diff(a, b, box):
    # pixels that differ clearly between two screenshots
    x0, y0, x1, y1 = box
    pa = a.load()
    pb = b.load()
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            if sum(abs(p - q) for p, q in zip(pa[x, y], pb[x, y])) > 60:
                n += 1
    return n


def cpu_ticks(pid):
    with open("/proc/%d/stat" % pid) as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])


print("== the launcher kit in a real window (Xvfb %s) ==" % DISP)
app = subprocess.Popen([PROG], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
time.sleep(1.8)
if app.poll() is not None:
    chk("the program is running", False, "exit %s" % app.returncode, app.stderr.read().decode()[:200])
    finish(1)
info = subprocess.run(["xwininfo", "-root", "-tree", "-display", DISP], capture_output=True, text=True).stdout
chk("a window with the program's title exists", "fUi launcher kit" in info or "launcher" in info.lower(), "", "")

im = shot("kit-live-1-home")
base = im.getpixel((600, 600))
dark = base[0] < 128
print("  (the window is %s)" % ("dark" if dark else "light"))
if dark:
    BASE, SURF, RAISED, ACCENT = rgb(0x1C1B22), rgb(0x2B2A33), rgb(0x37373F), rgb(0x1BD96A)
else:
    BASE, SURF, RAISED, ACCENT = rgb(0xF4F4F6), rgb(0xF9F9FB), rgb(0xFFFFFF), rgb(0x1BD96A)
chk("the sidebar has the surface colour", near(im.getpixel((100, 600)), SURF), im.getpixel((100, 600)), SURF)
chk("the page has the base colour", near(im.getpixel((600, 600)), BASE), im.getpixel((600, 600)), BASE)
chk("the primary button (Play) is the launcher accent", count(im, (278, 86, 360, 116), ACCENT) > 1500)
chk("the active nav item has the accent mark on the left edge", count(im, (0, 50, 6, 80), ACCENT, 40) > 40)
pr = im.getpixel((400, 341))
chk("the failed progress bar (64 %) is filled in the error colour (red)", pr[0] > 180 and pr[1] < 140 and pr[2] < 140, pr, "reddish")
pt = im.getpixel((900, 341))
chk("and its track is the button colour right of the fill", pt != pr and abs(pt[0] - pt[1]) < 20, pt, "grey")

# hover on Play: a different colour
xdo("mousemove", "300", "101")
time.sleep(0.5)
im2 = shot("kit-live-2-hover")
chk("hovering the primary button changes its colour", not near(im2.getpixel((283, 90)), im.getpixel((283, 90)), 2),
    im2.getpixel((283, 90)), im.getpixel((283, 90)))

# the Mods page: the tab bar, the tiles
xdo("mousemove", "60", "111", "click", "1")
time.sleep(0.7)
im3 = shot("kit-live-3-mods")
chk("the Mods page shows the accent underline of the active tab", count(im3, (262, 40, 390, 48), ACCENT, 40) > 60)
chk("the first tile has its avatar (hue #7AA2F7)", count(im3, (330, 160, 430, 230), rgb(0x7AA2F7)) > 1500)
chk("the picture tile shows its own gradient (green to blue)", count(im3, (520, 340, 720, 460), (40, 200, 120), 40) > 500)

# hover a tile: the accent ring
xdo("mousemove", "380", "430")
time.sleep(0.5)
im4 = shot("kit-live-4-tile-hover")
chk("a hovered tile gets the accent ring", count(im4, (264, 336, 494, 534), ACCENT, 60) > 300)

# click the tile: selected + a toast bottom right
xdo("click", "1")
time.sleep(0.9)
im5 = shot("kit-live-5-tile-click")
chk("a clicked tile shows the check badge (accent disc)", count(im5, (440, 345, 490, 380), ACCENT, 40) > 150)
chk("a toast stands at the bottom right (the raised surface)", count(im5, (604, 600, 984, 652), RAISED) > 8000)

# the search field: / focuses, keys type, Escape clears
xdo("mousemove", "400", "250")
xdo("key", "slash")
time.sleep(0.4)
im5b = shot("kit-live-5b-search-focused")
for ch in ("s", "o", "d"):
    xdo("key", ch)
    time.sleep(0.15)
time.sleep(0.4)
im6 = shot("kit-live-6-search")
BOX_TEXT = (310, 66, 520, 92)
chk("typing replaces the placeholder: the field's text area changes", diff(im5b, im6, BOX_TEXT) > 100, diff(im5b, im6, BOX_TEXT), "> 100")
chk("the clear button (an X) appears at the right end of the field", ink(im6, (600, 62, 640, 96)) > 10 and ink(im5b, (600, 62, 640, 96)) < 3)
xdo("key", "Escape")
time.sleep(0.4)
im7 = shot("kit-live-7-cleared")
chk("Escape clears the field: the placeholder is back, the X is gone",
    diff(im5b, im7, BOX_TEXT) < 25 and ink(im7, (600, 62, 640, 96)) < 3, diff(im5b, im7, BOX_TEXT), "< 25")

# the dialog: open from Settings, the scrim, the focus trap, Escape
xdo("mousemove", "80", "240", "click", "1")
time.sleep(0.5)
im8 = shot("kit-live-8-settings")
xdo("mousemove", "330", "322", "click", "1")
time.sleep(0.8)
im9 = shot("kit-live-9-dialog")
chk("the dialog's scrim covers the page (corner colour changed)", not near(im9.getpixel((5, 5)), im8.getpixel((5, 5)), 2) or not near(im9.getpixel((990, 600)), im8.getpixel((990, 600)), 2))
chk("the dialog's panel is the surface colour", near(im9.getpixel((300, 432)), SURF, 4), im9.getpixel((300, 432)), SURF)
xdo("key", "Tab")
time.sleep(0.4)
im10 = shot("kit-live-10-focus1")
xdo("key", "Tab")
time.sleep(0.4)
im11 = shot("kit-live-11-focus2")
xdo("key", "Tab")
time.sleep(0.4)
im12 = shot("kit-live-12-focus-wrapped")
chk("Tab moves the focus ring between the two buttons", im10.tobytes() != im11.tobytes())
chk("the third Tab wraps to the first button (same picture as after the first Tab)",
    im12.tobytes() == im10.tobytes() or count(im12, (0, 0, 1000, 666), ACCENT, 8) == count(im10, (0, 0, 1000, 666), ACCENT, 8))
xdo("key", "Escape")
time.sleep(0.8)
im13 = shot("kit-live-13-closed")
chk("Escape closes the dialog (the corner has the page colour again)", near(im13.getpixel((5, 5)), im8.getpixel((5, 5)), 2), im13.getpixel((5, 5)), im8.getpixel((5, 5)))

# the Markdown page and its scrolling
xdo("mousemove", "60", "157", "click", "1")
time.sleep(0.7)
im14 = shot("kit-live-14-docs")
chk("the Markdown page shows its heading", ink(im14, (270, 20, 700, 70)) > 300, ink(im14, (270, 20, 700, 70)), "> 300")
xdo("mousemove", "600", "400", "click", "5", "click", "5", "click", "5")
time.sleep(0.6)
im15 = shot("kit-live-15-docs-scrolled")
chk("the wheel scrolls the Markdown page", im15.tobytes() != im14.tobytes())

# idle: no CPU when nothing moves (toasts have expired after 5 s)
xdo("mousemove", "1050", "700")
time.sleep(6.0)
t0 = cpu_ticks(app.pid)
time.sleep(2.0)
t1 = cpu_ticks(app.pid)
chk("an idle window uses (almost) no CPU: <= 10 ticks in 2 s", t1 - t0 <= 10, t1 - t0, "<= 10")

print("%d checks, %d wrong" % (total, bad))
print("KIT LIVE PASSED" if bad == 0 else "KIT LIVE FAILED")
finish(0 if bad == 0 else 1)

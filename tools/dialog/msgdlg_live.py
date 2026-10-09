#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/msgdlg_live.py -- the fUi message box (lib/fui/msgdlg.fi) in a REAL window.

    python3 tools/dialog/msgdlg_live.py <msgdlg_live binary> [directory for the pictures]

The driver (tools/dialog/msgdlg_live_main.fi) shows ONE message box per run and prints `RESULT <rc> <answer>`
(rc 0 = a button decided, 1 = dismissed, 12 = no window). This script plays the user on a private Xvfb with xdotool
(mouse, keys) and takes xwd screenshots; what it checks is PIXELS and RESULTS, not that "a window exists":

  * every kind (information, question, warning, error) in the light and the dark set: the window size, the
    icon in the token colour of its kind (and in no other), the card and the strip, the default button filled
    with the accent (No on a warning that asks Yes / No), text ink;
  * Enter, Esc, Tab / Right / Left, Alt + letter, the click on every button, closing the window: the answers;
  * hover (the button's colour changes), the focus ring (the accent on the edge), Ctrl+C (the clipboard holds
    "title\\ntext"), a 4,000 character text that scrolls (wheel, End);
  * no display: rc 12; FUI_AUDIT=1: the audit says 0 unnamed.

Without Xvfb / xdotool / xwd / PIL / python-xlib the checks SKIP.
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 2:
    print("usage: msgdlg_live.py <msgdlg_live binary> [picture directory]")
    sys.exit(2)
DRIVER = os.path.abspath(sys.argv[1])
PICS = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

for tool in ("Xvfb", "xdotool", "xwd"):
    if not shutil.which(tool):
        print("  SKIP  %s is not installed: the message box was not run in a real window here" % tool)
        sys.exit(0)
try:
    from PIL import Image
    from Xlib import X, display
    import Xlib.protocol.event as xev
except ImportError as e:
    print("  SKIP  %s: the message box was not run in a real window here" % e)
    sys.exit(0)

FAILED = []
PASSED = [0]


def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""), flush=True)
    if cond:
        PASSED[0] += 1
    else:
        FAILED.append(name)


td = tempfile.mkdtemp(prefix="msgdlg-live-")
NUM = 300 + (os.getpid() % 600)
DISP = ":%d" % NUM
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1280x800x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(50):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
ENV_LIGHT = dict(os.environ, DISPLAY=DISP)
for k in ("WAYLAND_DISPLAY", "GTK_THEME", "FUI_AUDIT", "FIRN_CLIPBOARD"):
    ENV_LIGHT.pop(k, None)
ENV_DARK = dict(ENV_LIGHT, GTK_THEME="Adwaita:dark")
ENV = ENV_LIGHT


def cleanup(code):
    xvfb.terminate()
    try:
        xvfb.wait(3)
    except Exception:
        xvfb.kill()
    shutil.rmtree(td, ignore_errors=True)
    sys.exit(code)


def xdo(*a):
    return subprocess.run(["xdotool"] + list(a), env=ENV_LIGHT, capture_output=True, text=True).stdout.strip()


def shot():
    x = os.path.join(td, "s.xwd")
    p = os.path.join(td, "s.png")
    subprocess.run(["xwd", "-root", "-silent", "-display", DISP, "-out", x], check=True)
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True, capture_output=True)
    return Image.open(p).convert("RGB")


# ---- the colours of the two built-in sets (lib/fui/theme.fi), 0xRRGGBB
LIGHT = dict(base=0xF4F4F6, raised=0xFFFFFF, button=0xEDEDF0, hover=0xDFDFE2, accent=0x0060DF, warning=0x715100,
             error=0xC50042, text=0x15141B)
DARK = dict(base=0x1C1B22, raised=0x37373F, button=0x42414D, hover=0x52525E, accent=0x0A84FF, warning=0xFFBD4F,
            error=0xFF5C5C, text=0xFBFBFE)


def rgb(v):
    return ((v >> 16) & 255, (v >> 8) & 255, v & 255)


def kind_colour(pal, kind):
    return rgb(pal["warning"] if kind == 2 else pal["error"] if kind == 3 else pal["accent"])


class Win:
    """The dialog's window: its id, position and size on the screen."""

    def __init__(self, wid):
        self.id = wid
        g = xdo("getwindowgeometry", wid)
        pos = [l for l in g.splitlines() if "Position" in l][0].split(":")[1].split("(")[0].strip().split(",")
        wh = [l for l in g.splitlines() if "Geometry" in l][0].split(":")[1].strip().split("x")
        self.x, self.y = int(pos[0]), int(pos[1])
        self.w, self.h = int(wh[0]), int(wh[1])

    def crop(self, im):
        return im.crop((self.x, self.y, self.x + self.w, self.y + self.h))

    def abs(self, x, y):
        return self.x + x, self.y + y


class Run:
    """One message box: the driver process and its window."""

    def __init__(self, kind, buttons, title, text, env=None, args_extra=None):
        xdo("mousemove", "1200", "780")             # away: a button under the pointer is painted as hovered
        end = time.time() + 6                       # the window of the run before must be gone
        while time.time() < end and xdo("search", "--onlyvisible", "--name", "^msgdlg-live").split():
            time.sleep(0.1)
        self.title = title
        self.p = subprocess.Popen([DRIVER, str(kind), str(buttons), title, text], env=env or ENV_LIGHT,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
        self.win = None
        end = time.time() + 15
        while time.time() < end and self.win is None:
            ids = xdo("search", "--onlyvisible", "--name", "^%s$" % title).split()
            if ids:
                time.sleep(0.9)                      # let it paint (the focus, the settle frame)
                try:
                    self.win = Win(ids[-1])
                except (IndexError, ValueError):     # a window that closed between the search and the question
                    time.sleep(0.2)
            else:
                time.sleep(0.15)

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
        """(rc, answer, stderr) from the driver's `RESULT rc answer`, or None when it does not end."""
        try:
            out, err = self.p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.kill()
            return None
        for line in out.splitlines():
            if line.startswith("SIZE "):
                self.size = tuple(int(v) for v in line.split()[1:3])
            if line.startswith("RESULT "):
                f = line.split()
                return int(f[1]), int(f[2]), err
        return ("noline", out, err)

    def focus(self):
        xdo("mousemove", *[str(v) for v in self.win.abs(self.win.w // 2, 30)])
        xdo("windowfocus", self.win.id)
        time.sleep(0.25)

    def shot(self):
        return self.win.crop(shot())

    def close_window(self):
        d = display.Display(DISP)
        w = d.create_resource_object("window", int(self.win.id))
        e = xev.ClientMessage(window=w, client_type=d.intern_atom("WM_PROTOCOLS"),
                              data=(32, [d.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]))
        w.send_event(e, event_mask=0)
        d.flush()


def buttons_of(im, pal, w, h):
    """The buttons of the strip as (x0, x1) boxes left to right: the runs of non-strip colour on a row inside
    the buttons and above the label."""
    y = h - 40
    px = im.load()
    base = rgb(pal["base"])
    runs = []
    start = None
    for x in range(w):
        on = px[x, y] != base
        if on and start is None:
            start = x
        if not on and start is not None:
            if x - start >= 40:
                runs.append((start, x - 1))
            start = None
    if start is not None and w - start >= 40:
        runs.append((start, w - 1))
    return runs


def count_colour(im, box, colour, tol=0):
    x0, y0, x1, y1 = box
    px = im.load()
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            p = px[x, y]
            if abs(p[0] - colour[0]) <= tol and abs(p[1] - colour[1]) <= tol and abs(p[2] - colour[2]) <= tol:
                n += 1
    return n


def diff_boxes(a, b, box, thr=40):
    x0, y0, x1, y1 = box
    pa, pb = a.load(), b.load()
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            if sum(abs(p - q) for p, q in zip(pa[x, y], pb[x, y])) > thr:
                n += 1
    return n


def pic(im, name):
    if PICS:
        os.makedirs(PICS, exist_ok=True)
        im.save(os.path.join(PICS, name))


TWO = "Delete 3 files from the project?\nThis cannot be undone."
NAMES = ["info", "question", "warning", "error"]

print("== the message box in a real window (Xvfb %s) ==" % DISP, flush=True)

# ---------------------------------------------------------------- 1. the four kinds, light and dark
for mode, pal, env in (("light", LIGHT, ENV_LIGHT), ("dark", DARK, ENV_DARK)):
    for kind in range(4):
        r = Run(kind, 3, "msgdlg-live", TWO, env)
        if r.win is None:
            check("%s %s: the window appears" % (NAMES[kind], mode), False)
            r.kill()
            continue
        w = r.win
        im = r.shot()
        pic(im, "live-%s-yesnocancel-%s.png" % (NAMES[kind], mode))
        check("%s %s: the window is 360..560 x 130..200 (%dx%d)" % (NAMES[kind], mode, w.w, w.h),
              360 <= w.w <= 560 and 130 <= w.h <= 200)
        fill = kind_colour(pal, kind)
        n = count_colour(im, (24, 24, 56, 56), fill)
        check("%s %s: the icon is painted in the theme token colour" % (NAMES[kind], mode), n > 300, n)
        foreign = sum(count_colour(im, (24, 24, 56, 56), kind_colour(pal, o)) for o in range(4)
                      if kind_colour(pal, o) != fill)
        check("%s %s: and in no other kind's colour" % (NAMES[kind], mode), foreign == 0, foreign)
        px = im.load()
        check("%s %s: the card is the raised surface, the strip the base" % (NAMES[kind], mode),
              px[4, 4] == rgb(pal["raised"]) and px[4, w.h - 4] == rgb(pal["base"]))
        bx = buttons_of(im, pal, w.w, w.h)
        check("%s %s: three buttons, right-aligned (24 from the edge), one width" % (NAMES[kind], mode),
              len(bx) == 3 and abs(bx[2][1] - (w.w - 24)) <= 2 and len({b[1] - b[0] for b in bx}) <= 2, bx)
        if len(bx) == 3:
            default = 1 if kind >= 2 else 0
            acc = rgb(pal["accent"])
            filled = [count_colour(im, (b[0] + 6, w.h - 44, b[0] + 14, w.h - 20), acc) for b in bx]
            check("%s %s: only the default button (%s) is filled with the accent" % (
                NAMES[kind], mode, "No" if default else "Yes"),
                  filled[default] > 40 and sum(1 for i, v in enumerate(filled) if i != default and v > 0) == 0, filled)
        ink = sum(1 for x in range(60, w.w - 24) for y in range(24, 56) if px[x, y] != rgb(pal["raised"]))
        check("%s %s: the text is painted" % (NAMES[kind], mode), ink > 300, ink)
        r.focus()
        xdo("key", "Escape")
        res = r.result()
        check("%s %s: Esc dismisses (rc 1, answer Cancel)" % (NAMES[kind], mode), res is not None and res[:2] == (1, 2), res)
        check("%s %s: nothing on stderr" % (NAMES[kind], mode), res is not None and res[2] == "", res)

# ---------------------------------------------------------------- 2. the keyboard and the mouse
ENV = ENV_LIGHT


def result_of(r):
    res = r.result()
    return res[:2] if res and res[0] != "noline" else None


def keys(kind, buttons, *ks, text="Proceed?", title="msgdlg-live"):
    r = Run(kind, buttons, title, text)
    if r.win is None:
        r.kill()
        return None
    r.focus()
    for k in ks:
        xdo("key", k)
        time.sleep(0.2)
    return result_of(r)


# Enter = the default button (No on a warning / error that asks Yes / No)
check("Enter: question Yes/No/Cancel -> Yes", keys(1, 3, "Return") == (0, 3))
check("Enter: information OK -> OK", keys(0, 0, "Return") == (0, 1))
check("Enter: question OK/Cancel -> OK", keys(1, 1, "Return") == (0, 1))
check("Enter: warning Yes/No -> No (the safe one)", keys(2, 2, "Return") == (0, 4))
check("Enter: error Yes/No/Cancel -> No", keys(3, 3, "Return") == (0, 4))
# Esc = the close rule
check("Esc: OK only -> OK (dismissed)", keys(0, 0, "Escape") == (1, 1))
check("Esc: OK/Cancel -> Cancel", keys(1, 1, "Escape") == (1, 2))
check("Esc: Yes/No -> No", keys(1, 2, "Escape") == (1, 4))
check("Esc: Yes/No/Cancel -> Cancel", keys(1, 3, "Escape") == (1, 2))
# Alt + the underlined letter
check("Alt+Y: Yes", keys(1, 3, "alt+y") == (0, 3))
check("Alt+N: No", keys(1, 3, "alt+n") == (0, 4))
check("Alt+C: Cancel", keys(1, 3, "alt+c") == (0, 2))
check("Alt+O: OK (OK/Cancel)", keys(1, 1, "alt+o") == (0, 1))
check("Alt+C: Cancel (OK/Cancel)", keys(1, 1, "alt+c") == (0, 2))
check("Alt+Y / Alt+N on Yes/No", keys(1, 2, "alt+y") == (0, 3) and keys(1, 2, "alt+n") == (0, 4))
# Tab / Right / Left move the focus, Enter presses
check("Tab, Enter: second button (No)", keys(1, 3, "Tab", "Return") == (0, 4))
check("Tab, Tab, Enter: third button (Cancel)", keys(1, 3, "Tab", "Tab", "Return") == (0, 2))
check("Right, Enter: second button", keys(1, 3, "Right", "Return") == (0, 4))
check("Right, Right, Left, Enter: second button", keys(1, 3, "Right", "Right", "Left", "Return") == (0, 4))
check("Left from the first button wraps to the last (Cancel)", keys(1, 3, "Left", "Return") == (0, 2))
check("shift+Tab from the first wraps to the last", keys(1, 3, "shift+Tab", "Return") == (0, 2))
check("the space bar presses the focused button", keys(1, 3, "Right", "space") == (0, 4))
# the window's close button
for buttons, want in ((0, (1, 1)), (1, (1, 2)), (2, (1, 4)), (3, (1, 2))):
    r = Run(1, buttons, "msgdlg-live", "Proceed?")
    if r.win is None:
        check("closing the window, buttons %d" % buttons, False)
        r.kill()
        continue
    r.close_window()
    check("closing the window answers like Esc (buttons %d: %s)" % (buttons, want), result_of(r) == want)

# the mouse: a click on every button, found in the screenshot
for buttons, labels in ((0, ["OK"]), (1, ["OK", "Cancel"]), (2, ["Yes", "No"]), (3, ["Yes", "No", "Cancel"])):
    want_ans = {"OK": 1, "Cancel": 2, "Yes": 3, "No": 4}
    for i, lab in enumerate(labels):
        r = Run(1, buttons, "msgdlg-live", "Proceed?")
        if r.win is None:
            check("click %s of %d" % (lab, buttons), False)
            r.kill()
            continue
        bx = buttons_of(r.shot(), LIGHT, r.win.w, r.win.h)
        if len(bx) != len(labels):
            check("click %s of %d: the buttons are found" % (lab, buttons), False, bx)
            r.kill()
            continue
        cx = (bx[i][0] + bx[i][1]) // 2
        xdo("mousemove", *[str(v) for v in r.win.abs(cx, r.win.h - 28)])
        time.sleep(0.3)
        xdo("click", "1")
        got = result_of(r)
        check("a click on %s (%d buttons) answers it" % (lab, buttons), got == (0, want_ans[lab]), (got, bx, r.win.w, r.win.h))

# ---------------------------------------------------------------- 3. hover, the focus ring, the pressed colour
r = Run(1, 3, "msgdlg-live", TWO)
if r.win is not None:
    w = r.win
    rest = r.shot()
    pic(rest, "live-question-rest.png")
    bx = buttons_of(rest, LIGHT, w.w, w.h)
    check("hover: three buttons found", len(bx) == 3, bx)
    if len(bx) == 3:
        no = bx[1]
        probe = (no[0] + 8, w.h - 30)
        check("a plain button rests in the button colour", rest.getpixel(probe) == rgb(LIGHT["button"]), rest.getpixel(probe))
        xdo("mousemove", *[str(v) for v in w.abs((no[0] + no[1]) // 2, w.h - 28)])
        time.sleep(0.7)
        hov = r.shot()
        pic(hov, "live-question-hover.png")
        check("hovered: the hover colour", hov.getpixel(probe) == rgb(LIGHT["hover"]), hov.getpixel(probe))
        yes = bx[0]
        check("the default button does not change while another is hovered",
              hov.getpixel((yes[0] + 8, w.h - 30)) == rgb(LIGHT["accent"]))
        xdo("mousemove", *[str(v) for v in w.abs((yes[0] + yes[1]) // 2, w.h - 28)])
        time.sleep(0.7)
        hy = r.shot()
        check("hovering the default button changes its colour", hy.getpixel((yes[0] + 8, w.h - 30)) != rgb(LIGHT["accent"]),
              hy.getpixel((yes[0] + 8, w.h - 30)))
        xdo("mousemove", *[str(v) for v in w.abs(w.w // 2, 20)])
        r.focus()
        xdo("key", "Tab")
        time.sleep(0.6)
        foc = r.shot()
        pic(foc, "live-question-focus.png")
        check("Tab: the focus ring (the accent, 2 px) is on the second button",
              foc.getpixel((no[0] + 1, w.h - 28)) == rgb(LIGHT["accent"]) and foc.getpixel((no[0] + 4, w.h - 28)) != rgb(LIGHT["accent"]),
              (foc.getpixel((no[0] + 1, w.h - 28)), foc.getpixel((no[0] + 4, w.h - 28))))
        check("... and it left the first button's inner ring",
              diff_boxes(rest, foc, (yes[0], w.h - 44, yes[1], w.h - 12)) > 20)
        xdo("key", "Escape")
    res = r.result()
    check("the hover / focus run ended with Esc", res is not None and res[:2] == (1, 2), res)
else:
    check("the hover run starts", False)
    r.kill()

# ---------------------------------------------------------------- 4. Ctrl+C puts "title\ntext" on the clipboard
def clipboard_text(timeout=3.0):
    d = display.Display(DISP)
    root = d.screen().root
    w = root.create_window(0, 0, 1, 1, 0, d.screen().root_depth, X.InputOutput, X.CopyFromParent)
    clip = d.intern_atom("CLIPBOARD")
    utf8 = d.intern_atom("UTF8_STRING")
    prop = d.intern_atom("MSGDLG_TEST_SEL")
    w.convert_selection(clip, utf8, prop, X.CurrentTime)
    d.flush()
    end = time.time() + timeout
    while time.time() < end:
        while d.pending_events():
            e = d.next_event()
            if e.type == X.SelectionNotify:
                if e.property == X.NONE:
                    return None
                r = w.get_full_property(prop, X.AnyPropertyType)
                return bytes(r.value).decode("utf-8", "replace") if r is not None else None
        time.sleep(0.05)
    return None


r = Run(1, 3, "msgdlg-live", "Line one\nLine two")
if r.win is not None:
    r.focus()
    xdo("key", "ctrl+c")
    got = None
    for _ in range(12):                         # the dialog answers for 400 ms after the key
        got = clipboard_text(0.15)
        if got is not None:
            break
    check("Ctrl+C: the clipboard holds title, a line break and the text", got == "msgdlg-live\nLine one\nLine two", repr(got))
    check("Ctrl+C does not end the dialog", r.p.poll() is None)
    xdo("key", "Return")
    check("... and Enter then answers Yes", result_of(r) == (0, 3))
else:
    check("the clipboard run starts", False)
    r.kill()

# ---------------------------------------------------------------- 5. a long text scrolls
LONG = "\n".join("Line %d: the quick brown fox jumps over the lazy dog." % i for i in range(1, 61))
r = Run(2, 0, "msgdlg-live", LONG)
if r.win is not None:
    w = r.win
    top = r.shot()
    pic(top, "live-long-top.png")
    check("a 60 line text: the window is not taller than 60 %% of the screen (%d of 800)" % w.h, w.h <= 480, w.h)
    r.focus()
    xdo("mousemove", *[str(v) for v in w.abs(w.w // 2, 80)])
    time.sleep(0.2)
    xdo("click", "5")
    xdo("click", "5")
    xdo("click", "5")
    time.sleep(0.6)
    mid = r.shot()
    check("the wheel scrolls the text", diff_boxes(top, mid, (60, 24, w.w - 40, 270)) > 800)
    xdo("key", "End")
    time.sleep(0.6)
    end = r.shot()
    pic(end, "live-long-end.png")
    check("End scrolls on to the last line", diff_boxes(mid, end, (60, 24, w.w - 40, 270)) > 800)
    xdo("key", "Home")
    time.sleep(0.6)
    check("Home goes back to the top", diff_boxes(top, r.shot(), (60, 24, w.w - 40, 270)) < 20)
    xdo("key", "Escape")
    res = r.result()
    check("Esc still dismisses while scrolled", res is not None and res[:2] == (1, 1), res)
else:
    check("the long text run starts", False)
    r.kill()

# ---------------------------------------------------------------- 6. odd texts do not break it
for name, text in (("empty text", ""), ("umlauts", "ÄÖÜ äöü ß € é"),
                   ("CJK", "你好，世界。\nこれはテスト"),
                   ("emoji", "Done \U0001F600 \U0001F44D"), ("a very long word", "W" * 700)):
    r = Run(0, 0, "msgdlg-live", text)
    if r.win is None:
        check("%s: the window appears" % name, False)
        r.kill()
        continue
    im = r.shot()
    pic(im, "live-odd-%s.png" % name.replace(" ", "-"))
    r.focus()
    xdo("key", "Return")
    check("%s: shown and answered" % name, result_of(r) == (0, 1))

# ---------------------------------------------------------------- 7. the driver's other ways out
env = dict(ENV_LIGHT)
env.pop("DISPLAY")
p = subprocess.run([DRIVER, "1", "3", "t", "x"], env=env, capture_output=True, text=True, timeout=20)
check("no display: rc 12, the answer untouched", "RESULT 12 0" in p.stdout, (p.stdout, p.stderr))
env = dict(ENV_LIGHT, FUI_AUDIT="1")
p = subprocess.run([DRIVER, "1", "3", "t", "x"], env=env, capture_output=True, text=True, timeout=20)
check("FUI_AUDIT: the audit says 0 unnamed", "0 unnamed" in p.stdout, p.stdout)
check("... and no window was opened", xdo("search", "--onlyvisible", "--name", "^t$") == "")

print("%d checks OK, %d failed" % (PASSED[0], len(FAILED)))
if FAILED:
    print("MSGDLG LIVE FAILED")
    cleanup(1)
print("MSGDLG LIVE PASSED")
cleanup(0)

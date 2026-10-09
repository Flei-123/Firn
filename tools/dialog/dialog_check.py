#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/dialog_check.py -- std.dialog on Linux against the REAL dialogs of the fUi service (tools/dialogd).

    python3 tools/dialog/dialog_check.py <dialog_main binary> <dialogd binary> [pngdir]

A private Xvfb; the driver (tools/dialog/dialog_main.fi, built for Linux) runs with FIRN_DIALOG=service, so std.dialog
starts `dialogd`, which opens the REAL fUi dialogs (lib/fui/msgdlg, filedlg, colordlg, fontdlg). This script plays the
user with xdotool (keys and mouse; there is no window manager, so the keys go to the window under the pointer, which is
parked inside it), and checks the ANSWER that comes back through the whole chain: std.dialog -> line protocol -> dialogd ->
dialog -> line protocol -> std.dialog. The look and every interaction of each dialog is checked by its own live test
(tools/dialog/{msg,file,color,font}dlg_live.py); this one proves the wiring. A screenshot of each dialog goes to [pngdir].
Without Xvfb / xdotool / xwd / python-xlib / PIL the checks SKIP.
"""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 3:
    print("usage: dialog_check.py <dialog_main> <dialogd> [pngdir]")
    sys.exit(2)
DRIVER, DIALOGD = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
PNGDIR = os.path.abspath(sys.argv[3]) if len(sys.argv) > 3 else None
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
COUNT = [0]


def check(name, cond, extra=""):
    COUNT[0] += 1
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)


td = tempfile.mkdtemp(prefix="dlgcheck-")
if PNGDIR:
    os.makedirs(PNGDIR, exist_ok=True)
NUM = 300 + (os.getpid() % 500)
DISP = ":%d" % NUM
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1280x900x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(50):
    if os.path.exists("/tmp/.X11-unix/X%d" % NUM):
        break
    time.sleep(0.1)
ENV = dict(os.environ, DISPLAY=DISP, FIRN_DIALOG="service", FIRN_DIALOG_SERVICE=DIALOGD,
           HOME=td, XDG_CONFIG_HOME=os.path.join(td, "cfg"))
ENV.pop("WAYLAND_DISPLAY", None)
ENV.pop("CERTUS_THEME", None)


def xdo(*a):
    return subprocess.run(["xdotool"] + list(a), env=ENV, capture_output=True, text=True).stdout.strip()


def find_window(title, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        ids = xdo("search", "--onlyvisible", "--name", "^%s$" % title).split()
        if ids:
            time.sleep(0.8)             # let it paint
            return ids[0]
        time.sleep(0.2)
    return None


def geometry(win):
    g = xdo("getwindowgeometry", win)
    wh = [l for l in g.splitlines() if "Geometry" in l][0].split(":")[1].strip().split("x")
    return int(wh[0]), int(wh[1])


def snap(name):
    if not PNGDIR:
        return
    x = os.path.join(td, "s.xwd"); p = os.path.join(td, "s.png")
    subprocess.run(["xwd", "-root", "-display", DISP, "-out", x], check=True, env=ENV)
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/fui/xwd2png.py"), x, p], check=True, capture_output=True)
    shutil.copy(p, os.path.join(PNGDIR, name))


def close_window(win):
    d = display.Display(DISP)
    w = d.create_resource_object("window", int(win))
    e = xev.ClientMessage(window=w, client_type=d.intern_atom("WM_PROTOCOLS"),
                          data=(32, [d.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]))
    w.send_event(e, event_mask=0)
    d.flush()


class Run:
    def __init__(self, title, args):
        xdo("mousemove", "1270", "890")
        end = time.time() + 8           # the window of the run before must be gone
        while time.time() < end and xdo("search", "--onlyvisible", "--name", "^dlg-").split():
            time.sleep(0.1)
        self.p = subprocess.Popen([DRIVER] + args, env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                  start_new_session=True)
        self.win = find_window(title)
        if self.win:
            ww, wh = geometry(self.win)
            xdo("mousemove", "%d" % (ww // 2), "%d" % 6)    # inside the window, on the empty top strip
            time.sleep(0.3)

    def key(self, *k):
        xdo("key", *k)
        time.sleep(0.35)

    def type(self, text):
        xdo("type", "--delay", "40", text)
        time.sleep(0.4)

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

    def result(self, timeout=25):
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


def play(title, args, keys=None, typed=None, picture=None, close=False):
    """Open one dialog through std.dialog, optionally take a picture, then press keys / type / close; the driver's answer."""
    r = Run(title, args)
    if not r.win:
        r.kill()
        return {"NOWINDOW": "1"}
    if picture:
        snap(picture)
    if typed:
        r.type(typed)
    if close:
        close_window(r.win)
    for k in (keys or []):
        r.key(*k)
    return r.result()


# ---- message boxes (msgdlg): the default button on Enter, the Alt mnemonics, Esc and the window cross
check("confirm: Enter takes the default (Yes)", play("dlg-confirm", ["confirm"], [("Return",)], picture="confirm.png") == {"STATUS": "ok", "VALUE": "yes"})
check("confirm: Alt+N is No", play("dlg-confirm", ["confirm"], [("alt+n",)]) == {"STATUS": "ok", "VALUE": "no"})
check("confirm: Esc is No", play("dlg-confirm", ["confirm"], [("Escape",)]) == {"STATUS": "ok", "VALUE": "no"})
check("confirm: closing the window is No", play("dlg-confirm", ["confirm"], close=True) == {"STATUS": "ok", "VALUE": "no"})
check("info: Enter is OK", play("dlg-info", ["info"], [("Return",)], picture="info.png") == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: Enter is OK", play("dlg-okcancel", ["okcancel"], [("Return",)], picture="okcancel.png") == {"STATUS": "ok", "VALUE": "ok"})
check("okcancel: Alt+C is Cancel", play("dlg-okcancel", ["okcancel"], [("alt+c",)]) == {"STATUS": "ok", "VALUE": "cancel"})
check("okcancel: Esc is Cancel", play("dlg-okcancel", ["okcancel"], [("Escape",)]) == {"STATUS": "ok", "VALUE": "cancel"})
check("ynca: Alt+Y is Yes", play("dlg-ynca", ["ynca"], [("alt+y",)], picture="ynca.png") == {"STATUS": "ok", "VALUE": "yes"})
check("ynca: Alt+N is No", play("dlg-ynca", ["ynca"], [("alt+n",)]) == {"STATUS": "ok", "VALUE": "no"})
check("ynca: Alt+C is Cancel", play("dlg-ynca", ["ynca"], [("alt+c",)]) == {"STATUS": "ok", "VALUE": "cancel"})
check("ynca: closing the window is Cancel", play("dlg-ynca", ["ynca"], close=True) == {"STATUS": "ok", "VALUE": "cancel"})

# ---- files (filedlg): a folder with real content
d = os.path.join(td, "files")
os.makedirs(os.path.join(d, "sub"))
for n in ("a.txt", "b.png", "c.jpg"):
    open(os.path.join(d, n), "w").write("x")

check("open: typing 'b' finds b.png, Enter accepts it",
      play("dlg-open", ["open", d], [("Return",)], typed="b", picture="open.png") == {"STATUS": "ok", "VALUE": os.path.join(d, "b.png")})
check("open: Esc is a cancel", play("dlg-open", ["open", d], [("Escape",)]) == {"STATUS": "canceled"})
check("open: closing the window is a cancel", play("dlg-open", ["open", d], close=True) == {"STATUS": "canceled"})
check("open: a filter that hides a.txt (the first filter is pictures only): 'a' finds nothing, Enter does not accept a.txt",
      play("dlg-open", ["open", d], [("Return",), ("Escape",)], typed="a") == {"STATUS": "canceled"})
check("open_files: two paths come back one per line (Ctrl+A, Enter)",
      play("dlg-multi", ["open_multi", d], [("ctrl+a",), ("Return",)], picture="open_multi.png").get("VALUE", "").split("\n").count(os.path.join(d, "a.txt")) == 1)
check("save: the name field starts with the proposed name",
      play("dlg-save", ["save", d, "new.txt"], [("Return",)], picture="save.png") == {"STATUS": "ok", "VALUE": os.path.join(d, "new.txt")})
check("save: over an existing file Enter asks, 'y' replaces",
      play("dlg-save", ["save", d, "a.txt"], [("Return",), ("y",)]) == {"STATUS": "ok", "VALUE": os.path.join(d, "a.txt")})
check("save: over an existing file Enter asks, the default answer (Enter) is No and the dialog stays open",
      (lambda r: (r.key("Return"), r.key("Return"), r.alive(), r.kill())[2])(Run("dlg-save", ["save", d, "a.txt"])))
check("folder: Enter with nothing marked takes the opened folder",
      play("dlg-folder", ["folder", d], [("Return",)], picture="folder.png") == {"STATUS": "ok", "VALUE": d})

# ---- colour (colordlg)
check("colour: OK without touching anything gives the start colour back, bit for bit",
      play("dlg-color", ["color"], [("Return",)], picture="color.png") == {"STATUS": "ok", "VALUE": "1bd96a"})
check("colour: another start colour comes back unchanged", play("dlg-color", ["color", "ff8000"], [("Return",)]) == {"STATUS": "ok", "VALUE": "ff8000"})
check("colour: Esc is a cancel", play("dlg-color", ["color"], [("Escape",)]) == {"STATUS": "canceled"})
check("colour: closing the window is a cancel", play("dlg-color", ["color"], close=True) == {"STATUS": "canceled"})

# ---- font (fontdlg)
res = play("dlg-font", ["font"], [("Return",)], picture="font.png")
check("font: OK answers a family and the size 12 that was asked for", res.get("STATUS") == "ok" and "|12|" in res.get("VALUE", "") and not res["VALUE"].startswith("|"), res)
check("font: Esc is a cancel", play("dlg-font", ["font"], [("Escape",)]) == {"STATUS": "canceled"})

xvfb.terminate()
xvfb.wait()
shutil.rmtree(td, ignore_errors=True)
print("dialog (dialogd): %d of %d failed" % (len(FAILED), COUNT[0]) if FAILED else "dialog (dialogd): all %d checks passed" % COUNT[0])
sys.exit(1 if FAILED else 0)

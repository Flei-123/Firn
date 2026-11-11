#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import x11drv

try:
    from PIL import Image, ImageChops
except ImportError:
    print("SKIP: python3-PIL missing")
    sys.exit(0)

PROGRAM = sys.argv[1]
PROOF = sys.argv[2]
WINDOW_TITLE = "Terminal"
SCREEN_W = 1400
SCREEN_H = 900
SHELL = "/bin/sh"
CURSOR_MIN_GREEN = 180
CURSOR_MAX_RED = 120
IDLE_TICKS_MAX = 5
CAL_COL = 40
BLINK_PROBES = 12
CAL_ROW = 22
CAL_FIRST_ROW = 2
FKEY_CODES = ["^[OP", "^[OQ", "^[OR", "^[OS", "^[[15~", "^[[17~", "^[[18~", "^[[19~", "^[[20~", "^[[21~", "^[[23~", "^[[24~"]
KEY_CASES = [
    ("F1", "^[OP"), ("F2", "^[OQ"), ("F3", "^[OR"), ("F4", "^[OS"), ("F5", "^[[15~"), ("F6", "^[[17~"),
    ("F7", "^[[18~"), ("F8", "^[[19~"), ("F9", "^[[20~"), ("F10", "^[[21~"), ("F11", "^[[23~"), ("F12", "^[[24~"),
    ("Insert", "^[[2~"), ("Delete", "^[[3~"), ("Home", "^[[H"), ("End", "^[[F"), ("Prior", "^[[5~"),
    ("Next", "^[[6~"), ("Up", "^[[A"), ("Down", "^[[B"), ("Left", "^[[D"), ("Right", "^[[C"),
    ("ctrl+Left", "^[[1;5D"), ("shift+F1", "^[[1;2P"), ("alt+x", "^[x"),
]

errors = 0
count = 0
tmp = tempfile.mkdtemp(prefix="termx11.")
os.makedirs(PROOF, exist_ok=True)


def chk(name, ok, got="", want=""):
    global errors, count
    count += 1
    if not ok:
        errors += 1
    status = "ok" if ok else "WRONG got %r want %r" % (got, want)
    print("  %-52s%s" % (name, status), flush=True)


def path(name):
    return os.path.join(tmp, name)


def read(name, seconds=4.0):
    def get():
        try:
            return open(path(name)).read()
        except OSError:
            return ""
    return x11drv.wait_for(get, seconds) or ""


class Session:
    def __init__(self, srv, extra):
        self.srv = srv
        log = open(path("app.log"), "w")
        self.proc = subprocess.Popen([PROGRAM, "--shell", SHELL] + extra, env=srv.env, stdout=log, stderr=log)
        self.wid = srv.find_window(WINDOW_TITLE)
        srv.focus(self.wid)
        time.sleep(0.5)
        self.cell = None

    def shot(self, name):
        return self.srv.shot(self.wid, os.path.join(PROOF, name + ".png"))

    def cursor_box(self, name):
        im = self.shot(name)
        xs, ys = [], []
        for x in range(im.width):
            for y in range(im.height):
                r, g, b = im.getpixel((x, y))
                if g >= CURSOR_MIN_GREEN and r <= CURSOR_MAX_RED:
                    xs.append(x)
                    ys.append(y)
        if not xs:
            return None
        return min(xs), min(ys), max(xs) - min(xs) + 1, max(ys) - min(ys) + 1

    def park_cursor(self, col, row):
        self.srv.line("printf '\\033[2J\\033[%d;%dH'; read x" % (row + 1, col + 1))
        time.sleep(0.4)
        box = self.cursor_box("calibrate")
        self.srv.key("Return")
        return box

    def calibrate(self):
        a = self.park_cursor(0, CAL_FIRST_ROW)
        b = self.park_cursor(CAL_COL, CAL_ROW)
        if a is None or b is None:
            return None
        cw = (b[0] - a[0]) / CAL_COL
        ch = (b[1] - a[1]) / (CAL_ROW - CAL_FIRST_ROW)
        self.cell = (a[0], a[1] - CAL_FIRST_ROW * ch, cw, ch)
        return self.cell

    def center(self, col, row):
        ox, oy, cw, ch = self.cell
        return int(ox + (col + 0.5) * cw), int(oy + (row + 0.5) * ch)

    def edge(self, col, row, frac):
        ox, oy, cw, ch = self.cell
        return int(ox + (col + frac) * cw), int(oy + (row + 0.5) * ch)

    def stop(self):
        self.proc.terminate()
        self.proc.wait()


def same(a, b, box=None):
    if box:
        a, b = a.crop(box), b.crop(box)
    return ImageChops.difference(a, b).getbbox() is None


def check_typing(s):
    s.srv.line("echo hi > %s" % path("o1"))
    chk("typing: echo hi reaches the shell", read("o1") == "hi\n", read("o1"), "hi\n")
    s.srv.line("echo $$ > %s" % path("shellpid"))
    s.srv.line("ls / > %s" % path("ls"))
    chk("typing: ls lists the root", "etc" in read("ls").split(), read("ls")[:40], "etc in listing")
    im = s.shot("typing")
    colors = len(im.getcolors(maxcolors=1 << 20) or [])
    chk("screenshot is not blank", colors > 8, colors, "> 8 colors")


def check_keys(s):
    s.srv.line("cat -v > %s" % path("keys"))
    for key, _ in KEY_CASES:
        s.srv.key(key, "Return")
    time.sleep(0.3)
    s.srv.key("ctrl+d")
    got = read("keys").split("\n")
    for i, (key, want) in enumerate(KEY_CASES):
        have = got[i] if i < len(got) else ""
        chk("key %s" % key, have == want, have, want)


def check_resize(s):
    s.srv.line("stty size > %s" % path("size0"))
    rows0, cols0 = [int(v) for v in read("size0").split()]
    s.srv.resize(s.wid, 700, 420)
    time.sleep(1.0)
    s.srv.line("stty size > %s" % path("size1"))
    rows1, cols1 = [int(v) for v in read("size1").split()]
    chk("resize smaller: rows and cols shrink", rows1 < rows0 and cols1 < cols0, (rows1, cols1), "< %s" % ((rows0, cols0),))
    s.srv.resize(s.wid, 1000, 700)
    time.sleep(1.0)
    s.srv.line("stty size > %s" % path("size2"))
    rows2, cols2 = [int(v) for v in read("size2").split()]
    chk("resize larger: rows and cols grow", rows2 > rows0 and cols2 > cols0, (rows2, cols2), "> %s" % ((rows0, cols0),))
    s.shot("resized")


def prepare_row_text(s):
    cell = s.calibrate()
    s.srv.line("printf '\\033[2J\\033[3;1HSELECTME1234'; read x")
    time.sleep(0.5)
    return cell


def check_selection_and_copy(s):
    cell = prepare_row_text(s)
    chk("cell grid calibrated from the cursor", cell is not None and cell[2] > 3 and cell[3] > 8, cell, "cw>3 ch>8")
    if cell is None:
        return
    x, y = s.center(3, 2)
    s.srv.click(x, y, repeat=2)
    s.srv.key("ctrl+shift+c")
    time.sleep(0.3)
    chk("double click selects a word, Ctrl+Shift+C copies", s.srv.get_clipboard() == "SELECTME1234",
        s.srv.get_clipboard(), "SELECTME1234")
    s.shot("selection")
    s.srv.click(x, y, repeat=3)
    s.srv.key("ctrl+shift+c")
    time.sleep(0.3)
    chk("triple click selects the line", s.srv.get_clipboard().strip() == "SELECTME1234", s.srv.get_clipboard(), "SELECTME1234")
    x0, y0 = s.edge(0, 2, 0.1)
    x1, y1 = s.edge(5, 2, 0.5)
    s.srv.drag(x0, y0, x1, y1)
    s.srv.key("ctrl+shift+c")
    time.sleep(0.3)
    chk("mouse drag selects the dragged cells", s.srv.get_clipboard() == "SELECT", s.srv.get_clipboard(), "SELECT")
    s.srv.key("Return")


def check_paste(s):
    s.srv.line("cat > %s" % path("pasted"))
    s.srv.set_clipboard("PASTE-ME-OK\n")
    s.srv.key("ctrl+shift+v")
    time.sleep(0.5)
    s.srv.key("ctrl+d")
    chk("Ctrl+Shift+V pastes the clipboard", read("pasted") == "PASTE-ME-OK\n", read("pasted"), "PASTE-ME-OK\n")
    s.srv.line("printf '\\033[?2004h'; cat -v > %s" % path("bracketed"))
    s.srv.key("ctrl+shift+v")
    time.sleep(0.5)
    s.srv.key("ctrl+d", "ctrl+d")
    s.srv.line("printf '\\033[?2004l'")
    want = "^[[200~PASTE-ME-OK\n^[[201~"
    chk("paste is bracketed when the program asks", read("bracketed") == want, read("bracketed"), want)


def check_mouse_reports(s):
    s.srv.line("printf '\\033[?1000h\\033[?1006h'; cat -v > %s" % path("mouse"))
    time.sleep(0.5)
    x, y = s.center(10, 5)
    s.srv.click(x, y)
    s.srv.click(x, y, mods=("ctrl",))
    s.srv.click(x, y, mods=("alt",))
    s.srv.click(x, y, mods=("shift",))
    s.srv.key("ctrl+d", "ctrl+d")
    s.srv.line("printf '\\033[?1000l\\033[?1006l'")
    want = "".join("^[[<%d;11;6M^[[<%d;11;6m" % (b, b) for b in (0, 16, 8))
    chk("mouse reports carry Ctrl/Alt, Shift bypasses", read("mouse") == want, read("mouse"), want)


def check_wheel(s):
    s.srv.line("seq 1 300")
    time.sleep(0.8)
    base = s.shot("scroll_bottom")
    s.srv.move(*s.center(10, 5))
    for _ in range(10):
        s.srv.xdo("click", "4")
        time.sleep(0.05)
    time.sleep(0.4)
    up = s.shot("scroll_up")
    chk("wheel up scrolls into the scrollback", not same(base, up))
    for _ in range(10):
        s.srv.xdo("click", "5")
        time.sleep(0.05)
    time.sleep(0.4)
    chk("wheel down returns to the live screen", same(base, s.shot("scroll_back")))


def check_alt_screen(s):
    s.srv.line("printf '\\033[2J\\033[HMARK-ROW-ZERO\\n'")
    time.sleep(0.5)
    ox, oy, cw, ch = s.cell
    row0 = (0, oy, SCREEN_W, oy + ch)
    before = s.shot("alt_before")
    s.srv.line("printf '\\033[?1049h\\033[HALT-SCREEN'; sleep 3; printf '\\033[?1049l'")
    time.sleep(1.0)
    during = s.shot("alt_during")
    chk("alternate screen replaces the content", not same(before, during, row0))
    time.sleep(3.5)
    after = s.shot("alt_after")
    chk("leaving the alternate screen restores it", same(before, after, row0))


def check_pager(s):
    s.srv.line("seq 1 200 > %s" % path("lines"))
    s.srv.line("less %s" % path("lines"))
    time.sleep(0.8)
    shown = s.shot("less")
    s.srv.key("space")
    s.srv.key("q")
    time.sleep(0.5)
    s.srv.line("echo after-less > %s" % path("o2"))
    chk("pager runs and returns to the shell", read("o2") == "after-less\n", read("o2"), "after-less\n")
    chk("pager drew its own screen", len(shown.getcolors(maxcolors=1 << 20) or []) > 4)


def check_exit(s):
    pid = int(read("shellpid").strip() or 0)
    s.srv.line("exit 7")
    gone = x11drv.wait_for(lambda: not os.path.exists("/proc/%d" % pid), 4.0)
    chk("shell exit is reaped (no zombie)", bool(gone) and pid > 0, pid, "gone")
    chk("window stays open after the shell exits", s.proc.poll() is None)
    s.shot("exited")


def check_idle(s):
    before = x11drv.cpu_ticks(s.proc.pid)
    time.sleep(3.0)
    used = x11drv.cpu_ticks(s.proc.pid) - before
    chk("idle without blink costs no CPU", used <= IDLE_TICKS_MAX, used, "<= %d ticks" % IDLE_TICKS_MAX)


def check_blink(srv):
    s = Session(srv, [])
    try:
        s.srv.key("a")
        time.sleep(0.6)
        cell = None
        for _ in range(BLINK_PROBES):
            cell = s.cursor_box("blink")
            if cell:
                break
            time.sleep(0.1)
        cell = (cell[0] - 3 * cell[2], cell[1], cell[2], cell[3]) if cell else None
        phases = set()
        for _ in range(10):
            im = s.shot("blink")
            phases.add(im.getpixel((int(cell[0] + 3.5 * cell[2]), int(cell[1] + cell[3] // 2))))
            time.sleep(0.12)
        chk("cursor blinks when focused", len(phases) >= 2, len(phases), ">= 2 phases")
    finally:
        s.stop()


def main():
    srv = x11drv.Server(SCREEN_W, SCREEN_H)
    try:
        s = Session(srv, ["--no-blink"])
        chk("terminal window appears", s.wid != 0, s.wid, "window id")
        if s.wid == 0:
            return
        try:
            check_typing(s)
            check_keys(s)
            check_resize(s)
            check_selection_and_copy(s)
            check_paste(s)
            check_mouse_reports(s)
            check_wheel(s)
            check_alt_screen(s)
            check_pager(s)
            check_idle(s)
            check_exit(s)
        finally:
            s.stop()
        check_blink(srv)
    finally:
        srv.close()


for tool in x11drv.missing_tools():
    print("SKIP: %s missing" % tool)
    sys.exit(0)
if not os.path.exists(SHELL):
    print("SKIP: %s missing" % SHELL)
    sys.exit(0)
try:
    main()
finally:
    shutil.rmtree(tmp, ignore_errors=True)
print("term_x11: %d checks, %d wrong" % (count, errors))
sys.exit(1 if errors else 0)

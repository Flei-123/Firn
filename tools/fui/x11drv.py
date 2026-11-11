#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REQUIRED_TOOLS = ("Xvfb", "xdotool", "xwd", "xclip")
DISPLAY_FIRST = 140
DISPLAY_LAST = 200
KEY_DELAY_MS = 25


def missing_tools():
    return [t for t in REQUIRED_TOOLS if shutil.which(t) is None]


def free_display():
    for n in range(DISPLAY_FIRST, DISPLAY_LAST):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    raise SystemExit("no free display")


class Server:
    def __init__(self, width, height):
        self.n = free_display()
        self.env = dict(os.environ, DISPLAY=":%d" % self.n)
        self.env.pop("XAUTHORITY", None)
        self.proc = subprocess.Popen(
            ["Xvfb", ":%d" % self.n, "-screen", "0", "%dx%dx24" % (width, height), "-nolisten", "tcp", "-noreset"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            if os.path.exists("/tmp/.X11-unix/X%d" % self.n):
                break
            time.sleep(0.05)
        time.sleep(0.3)
        if self.proc.poll() is not None:
            raise SystemExit("Xvfb :%d did not start" % self.n)
        self.helpers = []

    def run(self, *args):
        return subprocess.run(list(args), env=self.env, capture_output=True)

    def xdo(self, *args):
        return self.run("xdotool", *args).stdout.decode().strip()

    def find_window(self, title, seconds=10.0):
        end = time.time() + seconds
        while time.time() < end:
            ids = self.xdo("search", "--name", "^%s$" % title).split()
            if ids:
                return int(ids[0])
            time.sleep(0.1)
        return 0

    def focus(self, wid):
        self.xdo("windowfocus", str(wid))
        time.sleep(0.2)

    def type_text(self, text):
        self.xdo("type", "--delay", str(KEY_DELAY_MS), text)

    def key(self, *names):
        for name in names:
            self.xdo("key", name)
            time.sleep(0.12)

    def line(self, text):
        self.type_text(text)
        self.key("Return")

    def resize(self, wid, w, h):
        self.xdo("windowsize", str(wid), str(w), str(h))

    def move(self, x, y):
        self.xdo("mousemove", str(x), str(y))

    def click(self, x, y, button=1, repeat=1, mods=()):
        self.move(x, y)
        for m in mods:
            self.xdo("keydown", m)
        self.xdo("click", "--repeat", str(repeat), "--delay", "60", str(button))
        for m in mods:
            self.xdo("keyup", m)

    def drag(self, x0, y0, x1, y1, mods=()):
        self.move(x0, y0)
        for m in mods:
            self.xdo("keydown", m)
        self.xdo("mousedown", "1")
        self.move((x0 + x1) // 2, (y0 + y1) // 2)
        self.move(x1, y1)
        self.xdo("mouseup", "1")
        for m in mods:
            self.xdo("keyup", m)

    def set_clipboard(self, text):
        p = subprocess.Popen(["xclip", "-selection", "clipboard", "-i"], env=self.env,
                             stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        p.stdin.write(text.encode())
        p.stdin.close()
        self.helpers.append(p)
        time.sleep(0.3)

    def get_clipboard(self):
        r = self.run("xclip", "-selection", "clipboard", "-o")
        return r.stdout.decode()

    def shot(self, wid, path):
        raw = path + ".xwd"
        with open(raw, "wb") as f:
            subprocess.run(["xwd", "-silent", "-id", str(wid)], env=self.env, stdout=f, check=True)
        subprocess.run([sys.executable, "-I", os.path.join(HERE, "xwd2png.py"), raw, path],
                       check=True, capture_output=True)
        os.remove(raw)
        from PIL import Image
        return Image.open(path).convert("RGB")

    def close(self):
        for p in self.helpers:
            p.terminate()
            p.wait()
        self.proc.terminate()
        self.proc.wait()


def cpu_ticks(pid):
    fields = open("/proc/%d/stat" % pid).read().split(")")[1].split()
    return int(fields[11]) + int(fields[12])


def wait_for(predicate, seconds=5.0, step=0.05):
    end = time.time() + seconds
    while time.time() < end:
        value = predicate()
        if value:
            return value
        time.sleep(step)
    return predicate()

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/webcheck.py -- THE WEB DEMO IN A REAL BROWSER (round WASM, step 6).
#
# Loads demos/webdemo/ into a headless Chromium, takes screenshots through
# the DevTools protocol and compares them PIXEL FOR PIXEL with the PNGs
# that tools/fui/run.sh --images paints natively for the same page. Same
# pixels = same code: the page is described by the same two functions of
# tools/fui/gallery9_main.fi and painted by the same fUi, once compiled for
# x86-64, once for wasm32.
#
# Then it OPERATES the page, the way a person would -- through the browser's
# own input events, not by calling the module:
#   * idle        no event, no new picture (the frame counter stands still)
#   * hover       the pointer over a button changes its colour; away again,
#                 the page is the reference again, pixel for pixel
#   * click       a checkbox flips and flips back; a click into empty space
#                 takes the focus away -- the reference again
#   * wheel       the list scrolls one notch and back -- the reference again
#   * keyboard    Tab puts a focus ring on the first control, the arrow keys
#                 scroll the list under the pointer, space toggles
#   * device px   devicePixelRatio 2: the canvas has 2x the pixels and fUi
#                 paints with theme scale 2000
#
# Usage: webcheck.py <demo dir> <reference dir (run.sh --images belege)>
#        exit 0 = everything held
# Environment: WASM=<file in the demo dir> loads another build of the page,
#              PICTURES_ONLY=1 stops after the four pictures.
import base64
import io
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

import numpy as np
import websocket
from PIL import Image

DEMO = os.path.abspath(sys.argv[1])
REF = os.path.abspath(sys.argv[2])
OUT = os.environ.get("W", "/tmp/firn-webcheck")
WASM = os.environ.get("WASM", "gallery9.wasm")
PICTURES_ONLY = os.environ.get("PICTURES_ONLY", "") == "1"
os.makedirs(OUT, exist_ok=True)
failures = []


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Browser:
    def __init__(self):
        self.port = free_port()
        self.proc = subprocess.Popen(
            ["chromium", "--headless=new", "--no-sandbox", "--disable-gpu",
             "--hide-scrollbars", "--force-color-profile=srgb",
             "--remote-debugging-port=%d" % self.port, "--window-size=1400,900",
             "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        url = None
        for _ in range(100):
            try:
                with urllib.request.urlopen("http://127.0.0.1:%d/json" % self.port) as r:
                    pages = [t for t in json.load(r) if t["type"] == "page"]
                    if pages:
                        url = pages[0]["webSocketDebuggerUrl"]
                        break
            except OSError:
                pass
            time.sleep(0.1)
        if url is None:
            raise SystemExit("webcheck: chromium did not come up")
        self.ws = websocket.create_connection(url, timeout=30, suppress_origin=True)
        self.next = 0
        self.call("Page.enable")
        self.call("Runtime.enable")

    def call(self, method, **params):
        self.next += 1
        me = self.next
        self.ws.send(json.dumps({"id": me, "method": method, "params": params}))
        while True:
            m = json.loads(self.ws.recv())
            if m.get("id") == me:
                if "error" in m:
                    raise SystemExit("webcheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("value")

    def frames(self):
        return self.js("window.firnFrames || 0")

    def wait_frame(self, before, timeout=5.0):
        t0 = time.time()
        while time.time() - t0 < timeout:
            if self.frames() > before:
                return True
            time.sleep(0.02)
        return False

    def shot(self, w, h, name):
        r = self.call("Page.captureScreenshot", format="png",
                      clip={"x": 0, "y": 0, "width": w, "height": h, "scale": 1})
        img = Image.open(io.BytesIO(base64.b64decode(r["data"]))).convert("RGB")
        img.save(os.path.join(OUT, name + ".png"))
        return np.asarray(img).astype(np.int32)

    def close(self):
        try:
            self.ws.close()
        finally:
            self.proc.terminate()
            self.proc.wait()


def diff(a, b):
    if a.shape != b.shape:
        return -1
    return int((a != b).any(axis=2).sum())


def ref(name):
    return np.asarray(Image.open(os.path.join(REF, name)).convert("RGB")).astype(np.int32)


def verdict(ok, text):
    print("   %-66s %s" % (text, "OK" if ok else "FAILED"))
    if not ok:
        failures.append(text)


# ------------------------------------------------------------ the server
hport = free_port()
server = subprocess.Popen([sys.executable, "-m", "http.server", str(hport),
                           "--bind", "127.0.0.1", "--directory", DEMO],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
b = Browser()
try:
    def load(theme, w, h, dpr=1):
        b.call("Emulation.setDeviceMetricsOverride", width=w, height=h,
               deviceScaleFactor=dpr, mobile=False)
        b.call("Page.navigate", url="http://127.0.0.1:%d/index.html?w=%d&h=%d&theme=%s&wasm=%s"
               % (hport, w, h, theme, WASM))
        t0 = time.time()
        while time.time() - t0 < 30:
            if b.frames() >= 1:
                return True
            err = b.js("document.body && document.body.dataset.error")
            if err:
                print("   page error: %s" % err)
                return False
            time.sleep(0.05)
        return False

    print("== the page against the PNGs of tools/fui/run.sh --images ==")
    total = 0
    for theme, w, png in [("dark", 1240, "fui-deklarativ-dunkel.png"),
                          ("light", 1240, "fui-deklarativ-hell.png"),
                          ("dark", 980, "fui-deklarativ-schmal-dunkel.png"),
                          ("light", 980, "fui-deklarativ-schmal-hell.png")]:
        if not load(theme, w, 720):
            verdict(False, "%s %d: no picture within 30 s" % (theme, w))
            continue
        d = diff(b.shot(w, 720, "web-%s-%d" % (theme, w)), ref(png))
        total += max(d, 0)
        verdict(d == 0, "%-5s %4d x 720 vs %-32s %d px differ" % (theme, w, png, d))
    print("   differing pixels over all four pictures: %d" % total)
    if PICTURES_ONLY:
        raise SystemExit(1 if failures else 0)

    print("== operating the page (dark, 1240 x 720) ==")
    load("dark", 1240, 720)
    base = ref("fui-deklarativ-dunkel.png")
    mouse = lambda kind, x, y, **kw: b.call("Input.dispatchMouseEvent", type=kind, x=x, y=y, **kw)

    def settle():
        time.sleep(0.15)

    # idle: nothing changes, nothing is painted
    f0 = b.frames()
    time.sleep(1.0)
    verdict(b.frames() == f0, "idle for 1 s: no new picture (frames %d -> %d)" % (f0, b.frames()))

    # hover over "Neu" in the toolbar, then away
    f0 = b.frames()
    mouse("mouseMoved", 77, 124)
    painted = b.wait_frame(f0)
    d_hover = diff(b.shot(1240, 720, "op-hover"), base)
    verdict(painted and d_hover > 0, "pointer over 'Neu': repainted, %d px changed" % d_hover)
    f0 = b.frames()
    mouse("mouseMoved", 1000, 705)
    b.wait_frame(f0)
    settle()
    d = diff(b.shot(1240, 720, "op-hover-away"), base)
    verdict(d == 0, "pointer away again: the reference again (%d px differ)" % d)

    # click the checkbox "Zebrastreifen" twice, then into empty space
    click = lambda x, y: (mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1),
                          mouse("mouseReleased", x, y, button="left", buttons=0, clickCount=1))
    f0 = b.frames()
    click(587, 423)
    b.wait_frame(f0)
    settle()
    d_click = diff(b.shot(1240, 720, "op-click"), base)
    verdict(d_click > 0, "click on the checkbox: it flips, %d px changed" % d_click)
    f0 = b.frames()
    click(587, 423)
    b.wait_frame(f0)
    f0 = b.frames()
    click(1000, 705)
    b.wait_frame(f0)
    mouse("mouseMoved", 1000, 705)
    settle()
    d = diff(b.shot(1240, 720, "op-click-back"), base)
    verdict(d == 0, "click again, click into empty space: the reference (%d px)" % d)

    # the wheel over the list: one notch down, one up
    mouse("mouseMoved", 280, 400)
    settle()
    f0 = b.frames()
    mouse("mouseWheel", 280, 400, deltaX=0, deltaY=100)
    b.wait_frame(f0)
    settle()
    d_wheel = diff(b.shot(1240, 720, "op-wheel"), base)
    verdict(d_wheel > 0, "wheel one notch down over the list: scrolled, %d px" % d_wheel)
    f0 = b.frames()
    mouse("mouseWheel", 280, 400, deltaX=0, deltaY=-100)
    b.wait_frame(f0)
    mouse("mouseMoved", 1000, 705)
    settle()
    d = diff(b.shot(1240, 720, "op-wheel-back"), base)
    verdict(d == 0, "one notch up again: the reference (%d px)" % d)

    # the keyboard
    key = lambda k, code, vk, shift=0: (
        b.call("Input.dispatchKeyEvent", type="keyDown", key=k, code=code,
               windowsVirtualKeyCode=vk, modifiers=8 if shift else 0),
        b.call("Input.dispatchKeyEvent", type="keyUp", key=k, code=code,
               windowsVirtualKeyCode=vk, modifiers=8 if shift else 0))
    b.js("document.getElementById('firn').focus()")
    f0 = b.frames()
    key("Tab", "Tab", 9)
    b.wait_frame(f0)
    settle()
    shot_tab = b.shot(1240, 720, "op-tab")
    d_tab = diff(shot_tab, base)
    verdict(d_tab > 0, "Tab: a focus ring on the first control, %d px changed" % d_tab)
    verdict(b.js("document.activeElement && document.activeElement.id") == "firn",
            "Tab stays inside the canvas (the module used the key)")
    mouse("mouseMoved", 280, 400)
    settle()
    f0 = b.frames()
    key("ArrowDown", "ArrowDown", 40)
    b.wait_frame(f0)
    settle()
    d_down = diff(b.shot(1240, 720, "op-arrow"), shot_tab)
    verdict(d_down > 0, "ArrowDown over the list: scrolled one step, %d px" % d_down)
    f0 = b.frames()
    key("ArrowUp", "ArrowUp", 38)
    b.wait_frame(f0)
    # A click into empty space takes the focus ring away again.
    f0 = b.frames()
    click(1000, 705)
    b.wait_frame(f0)
    mouse("mouseMoved", 1000, 705)
    settle()
    d_kb = diff(b.shot(1240, 720, "op-keys-back"), base)
    verdict(d_kb == 0, "ArrowUp, click into empty space: the reference (%d px)" % d_kb)

    # space on the toggle "Weich auslaufen": focus it with a click, press space
    f0 = b.frames()
    click(592, 467)
    b.wait_frame(f0)
    settle()
    after_click = b.shot(1240, 720, "op-toggle-click")
    f0 = b.frames()
    key(" ", "Space", 32)
    b.wait_frame(f0)
    settle()
    d_space = diff(b.shot(1240, 720, "op-space"), after_click)
    verdict(d_space > 0, "space on the focused toggle: it flips back, %d px" % d_space)

    print("== device pixels: devicePixelRatio 2 ==")
    load("dark", 1240, 720, dpr=2)
    cw = b.js("document.getElementById('firn').width")
    chh = b.js("document.getElementById('firn').height")
    verdict(cw == 2480 and chh == 1440,
            "the canvas has %s x %s device pixels for 1240 x 720 CSS" % (cw, chh))
    r = b.call("Page.captureScreenshot", format="png")
    Image.open(io.BytesIO(base64.b64decode(r["data"]))).save(os.path.join(OUT, "web-dpr2.png"))
    print("   screenshot at ratio 2: %s" % os.path.join(OUT, "web-dpr2.png"))
finally:
    b.close()
    server.terminate()

print()
if failures:
    print("WEBCHECK FAILED: %d" % len(failures))
    for f in failures:
        print("   " + f)
    sys.exit(1)
print("WEBCHECK PASSED")

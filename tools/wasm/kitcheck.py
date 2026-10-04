#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/kitcheck.py -- THE LAUNCHER KIT IN A REAL BROWSER.
#
# examples/fui/launcher_kit.fi is one source: natively an X11 window
# (tools/fui/kitlive.py proves that one on an Xvfb), built with
# --target=wasm32-browser a page that paints into a canvas. This script loads
# the module into a headless Chromium, operates it with the browser's own
# mouse and key events, and reads the SCREENSHOT:
#   * the first picture: the sidebar, the accent button, the page ground
#   * Mods: the tab underline, the avatar hues, a hovered tile's ring
#   * a clicked tile: the check badge and a toast at the bottom right
#   * the dialog: scrim, panel, the focus ring walking Tab / Tab / Tab and
#     wrapping, Escape closing it
#   * the Markdown page: text on the page, the wheel scrolls it
#   * idle: after the toasts are gone no new picture is painted
# The browser draws its own text (the page's font), so text pixels are not
# compared; colours of grounds, accents and rings are, because they come from
# the theme.
#
# Usage: kitcheck.py <launcher_kit.wasm> <demo dir with index.html + firn.js>
# Environment: W=<dir for the screenshots> (default /tmp/firn-kitcheck)
import base64
import io
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

import numpy as np
import websocket
from PIL import Image

WASM = os.path.abspath(sys.argv[1])
DEMO = os.path.abspath(sys.argv[2])
OUT = os.environ.get("W", "/tmp/firn-kitcheck")
os.makedirs(OUT, exist_ok=True)
failures = []
W, H = 960, 640


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
             "--remote-debugging-port=%d" % self.port, "--window-size=1000,800",
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
            raise SystemExit("kitcheck: chromium did not come up")
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
                    raise SystemExit("kitcheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("value")

    def frames(self):
        return self.js("window.firnFrames || 0")

    def shot(self, name):
        r = self.call("Page.captureScreenshot", format="png",
                      clip={"x": 0, "y": 0, "width": W, "height": H, "scale": 1})
        img = Image.open(io.BytesIO(base64.b64decode(r["data"]))).convert("RGB")
        img.save(os.path.join(OUT, name + ".png"))
        return np.asarray(img).astype(np.int32)

    def close(self):
        try:
            self.ws.close()
        finally:
            self.proc.terminate()
            self.proc.wait()


def verdict(ok, text):
    print("   %-76s %s" % (text, "OK" if ok else "FAILED"), flush=True)
    if not ok:
        failures.append(text)


def near(px, col, tol=3):
    return all(abs(int(a) - b) <= tol for a, b in zip(px, col))


def count(img, box, col, tol=3):
    x0, y0, x1, y1 = box
    reg = img[y0:y1, x0:x1]
    return int((np.abs(reg - np.array(col)) <= tol).all(axis=2).sum())


def changed(a, b, box=None):
    if box:
        x0, y0, x1, y1 = box
        a = a[y0:y1, x0:x1]
        b = b[y0:y1, x0:x1]
    return int((a != b).any(axis=2).sum())


def rgb(h):
    return ((h >> 16) & 255, (h >> 8) & 255, h & 255)


tmp = tempfile.mkdtemp(prefix="kitcheck")
for f in ("index.html", "firn.js", "DejaVuSans.ttf"):
    shutil.copy(os.path.join(DEMO, f), tmp)
shutil.copy(WASM, os.path.join(tmp, "launcher_kit.wasm"))
hport = free_port()
server = subprocess.Popen([sys.executable, "-m", "http.server", str(hport),
                           "--bind", "127.0.0.1", "--directory", tmp],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
b = Browser()


def mouse(kind, x, y, **kw):
    b.call("Input.dispatchMouseEvent", type=kind, x=x, y=y, **kw)


def move(x, y):
    mouse("mouseMoved", x, y)
    time.sleep(0.25)


def click(x, y):
    mouse("mouseMoved", x, y)
    mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1)
    mouse("mouseReleased", x, y, button="left", buttons=0, clickCount=1)
    time.sleep(0.5)


def key(k, code=None, vk=0, text=None):
    p = {"key": k, "code": code or k}
    if vk:
        p["windowsVirtualKeyCode"] = vk
    if text is not None:
        p["text"] = text
    b.call("Input.dispatchKeyEvent", type="keyDown", **p)
    b.call("Input.dispatchKeyEvent", type="keyUp", **{"key": k, "code": code or k})
    time.sleep(0.3)


try:
    print("== launcher_kit.wasm (%d x %d, dark) ==" % (W, H))
    b.call("Emulation.setDeviceMetricsOverride", width=W, height=H, deviceScaleFactor=1, mobile=False)
    b.call("Page.navigate", url="http://127.0.0.1:%d/index.html?w=%d&h=%d&theme=dark&wasm=launcher_kit.wasm"
           % (hport, W, H))
    ok = False
    t0 = time.time()
    while time.time() - t0 < 40:
        if b.frames() >= 1:
            ok = True
            time.sleep(0.5)
            break
        err = b.js("document.body && document.body.dataset.error")
        if err:
            print("   page error: %s" % err)
            break
        time.sleep(0.05)
    verdict(ok, "the module starts and paints a first picture")
    if not ok:
        raise SystemExit("kitcheck: no picture")
    BASE, SURF, RAISED, ACCENT = rgb(0x1C1B22), rgb(0x2B2A33), rgb(0x37373F), rgb(0x1BD96A)
    s1 = b.shot("web-kit-1-home")
    verdict(near(s1[600, 600], BASE), "the page has the base colour: %s" % (s1[600, 600],))
    verdict(near(s1[600, 100], SURF), "the sidebar has the surface colour: %s" % (s1[600, 100],))
    verdict(count(s1, (266, 82, 340, 114), ACCENT) > 1200, "the primary button (Play) is the launcher accent")
    verdict(count(s1, (0, 44, 5, 84), ACCENT, 40) > 40, "the active nav item has the accent mark on its left edge")
    f0 = b.frames()
    time.sleep(0.6)

    # Mods: tabs and tiles
    click(100, 108)
    s2 = b.shot("web-kit-2-mods")
    verdict(changed(s1, s2) > 20000, "clicking Mods changes the page")
    verdict(count(s2, (250, 38, 380, 46), ACCENT, 40) > 60, "the active tab has the accent underline")
    verdict(count(s2, (320, 150, 420, 220), rgb(0x7AA2F7)) > 1200, "the first tile shows its avatar hue (#7AA2F7)")
    move(380, 420)
    s3 = b.shot("web-kit-3-hover")
    verdict(count(s3, (252, 316, 480, 520), ACCENT, 60) > 300, "a hovered tile gets the accent ring")
    click(380, 420)
    s4 = b.shot("web-kit-4-click")
    verdict(count(s4, (595, 560, 945, 625), RAISED) > 8000, "a clicked tile raises a toast at the bottom right")

    # the dialog from Settings
    click(100, 232)
    s5 = b.shot("web-kit-5-settings")
    click(330, 310)
    time.sleep(0.4)
    s6 = b.shot("web-kit-6-dialog")
    verdict(changed(s5, s6, (0, 0, 240, 640)) > 20000, "the dialog's scrim covers even the sidebar")
    verdict(near(s6[400, 300], SURF, 6), "the dialog's panel is the surface colour: %s" % (s6[400, 300],))
    key("Tab", "Tab", 9)
    s7 = b.shot("web-kit-7-focus1")
    key("Tab", "Tab", 9)
    s8 = b.shot("web-kit-8-focus2")
    key("Tab", "Tab", 9)
    s9 = b.shot("web-kit-9-focus3")
    verdict(changed(s7, s8) > 50, "Tab moves the focus ring from one button to the other")
    verdict(changed(s7, s9) < 30, "the third Tab wraps round to the first: the focus cannot leave the dialog")
    key("Escape", "Escape", 27)
    s10 = b.shot("web-kit-10-closed")
    verdict(near(s10[5, 5], s5[5, 5], 2), "Escape closes the dialog (the corner has the page colour again)")

    # the Markdown page
    click(100, 152)
    s11 = b.shot("web-kit-11-docs")
    ink = int((np.abs(s11[10:80, 270:700].sum(axis=2) - s11[10, 270].sum()) > 200).sum())
    verdict(ink > 300, "the Markdown page paints its heading (%d ink pixels)" % ink)
    mouse("mouseWheel", 600, 400, deltaX=0, deltaY=300)
    time.sleep(0.4)
    s12 = b.shot("web-kit-12-scrolled")
    verdict(changed(s11, s12, (260, 0, 960, 640)) > 5000, "the wheel scrolls the Markdown page")

    # idle: the toasts are gone after 5 s, then nothing is painted
    time.sleep(6.5)
    f1 = b.frames()
    time.sleep(1.5)
    f2 = b.frames()
    verdict(f2 == f1, "idle: no new picture while nothing moves (frames %d -> %d)" % (f1, f2))
finally:
    b.close()
    server.terminate()
    shutil.rmtree(tmp, ignore_errors=True)

if failures:
    print("KITCHECK FAILED (%d)" % len(failures))
    sys.exit(1)
print("KITCHECK PASSED")

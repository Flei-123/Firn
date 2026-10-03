#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/appcheck.py -- THE fui.app EXAMPLES IN A REAL BROWSER.
#
# examples/fui/{hello_window,counter,form}.fi are ONE source each; natively
# they open an X11 window (lib/@linux/fui/apphost.fi), built with
# --target=wasm32-browser they paint into a canvas (lib/@web/fui/apphost.fi).
# This script loads the three modules of demos/webapp/ into a headless
# Chromium and
#   * compares the first picture PIXEL FOR PIXEL with the PNG that
#     tools/fui/app_main.fi paints natively for the same program
#     (same pixels = same code, as tools/wasm/webcheck.py does for gallery9)
#   * operates each page through the browser's own input events:
#     hello   Close is clicked -> the page goes blank (a tab cannot close)
#     counter + three times -> the picture changes; the number is read back
#             by comparing with the native picture of "3"
#     form    click into the name field, type, Tab, type, click Send ->
#             the answer appears (picture differs from the empty form and
#             matches the native filled form)
#   * idle: 1 s without input paints nothing.
#
# Usage: appcheck.py <demo dir> <dir with the native PNGs of app_main>
# Environment: W=<dir for the screenshots> (default /tmp/firn-appcheck)
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
OUT = os.environ.get("W", "/tmp/firn-appcheck")
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
            raise SystemExit("appcheck: chromium did not come up")
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
                    raise SystemExit("appcheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("value")

    def frames(self):
        return self.js("window.firnFrames || 0")

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
    print("   %-68s %s" % (text, "OK" if ok else "FAILED"))
    if not ok:
        failures.append(text)


hport = free_port()
server = subprocess.Popen([sys.executable, "-m", "http.server", str(hport),
                           "--bind", "127.0.0.1", "--directory", DEMO],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
b = Browser()


def load(wasm, w, h):
    b.call("Emulation.setDeviceMetricsOverride", width=w, height=h,
           deviceScaleFactor=1, mobile=False)
    b.call("Page.navigate", url="http://127.0.0.1:%d/index.html?w=%d&h=%d&theme=light&wasm=%s"
           % (hport, w, h, wasm))
    t0 = time.time()
    while time.time() - t0 < 30:
        if b.frames() >= 1:
            time.sleep(0.2)
            return True
        err = b.js("document.body && document.body.dataset.error")
        if err:
            print("   page error: %s" % err)
            return False
        time.sleep(0.05)
    return False


def mouse(kind, x, y, **kw):
    b.call("Input.dispatchMouseEvent", type=kind, x=x, y=y, **kw)


def click(x, y):
    mouse("mouseMoved", x, y)
    mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1)
    mouse("mouseReleased", x, y, button="left", buttons=0, clickCount=1)
    time.sleep(0.4)  # the hover glide of a button runs 120 ms (r126)


def key(k, code=None, vk=0, text=None):
    p = {"key": k, "code": code or k}
    if vk:
        p["windowsVirtualKeyCode"] = vk
    if text is not None:
        p["text"] = text
    b.call("Input.dispatchKeyEvent", type="keyDown", **p)
    b.call("Input.dispatchKeyEvent", type="keyUp", **{"key": k, "code": code or k})


def type_text(t):
    for ch in t:
        key(ch, "Key" + ch.upper() if ch.isalpha() else "Digit0", 0, ch)
    time.sleep(0.15)


try:
    print("== hello_window.wasm (400 x 250) ==")
    ok = load("hello_window.wasm", 400, 250)
    verdict(ok, "the module starts and paints a first picture")
    s = b.shot(400, 250, "web-hello")
    d = diff(s, ref("app-hello.png"))
    verdict(d == 0, "first picture vs native app-hello.png: %d px differ" % d)
    f0 = b.frames()
    time.sleep(1.0)
    verdict(b.frames() == f0, "idle 1 s: no new picture (frames %d -> %d)" % (f0, b.frames()))
    click(200, 140)
    s2 = b.shot(400, 250, "web-hello-closed")
    blank = int((s2 != s2[0, 0]).any(axis=2).sum())
    verdict(blank == 0, "Close clicked: the page is blank (%d px not background)" % blank)

    print("== counter.wasm (300 x 200) ==")
    ok = load("counter.wasm", 300, 200)
    verdict(ok, "the module starts")
    c0 = b.shot(300, 200, "web-counter-0")
    for _ in range(2):
        click(182, 130)
    c2 = b.shot(300, 200, "web-counter-2")
    d = diff(c2, ref("app-counter.png"))
    verdict(diff(c0, c2) > 0, "two clicks on + change the picture")
    verdict(d == 0, "after two + it is the native picture of 2: %d px differ" % d)

    print("== form.wasm (420 x 300) ==")
    ok = load("form.wasm", 420, 300)
    verdict(ok, "the module starts")
    e0 = b.shot(420, 300, "web-form-empty")
    d = diff(e0, ref("app-form-empty.png"))
    verdict(d == 0, "empty form vs native app-form-empty.png: %d px differ" % d)
    click(210, 103)
    type_text("Justin Fleischerx")
    key("Backspace", "Backspace", 8)
    key("Tab", "Tab", 9)
    type_text("justin@fleitec.com")
    click(168, 196)
    e1 = b.shot(420, 300, "web-form-filled")
    d = diff(e1, ref("app-form-filled.png"))
    verdict(diff(e0, e1) > 0, "typing and Send change the picture")
    verdict(d == 0, "filled form vs native app-form-filled.png: %d px differ" % d)

    print("== notes.wasm (420 x 360): a text area ==")
    ok = load("notes.wasm", 420, 360)
    verdict(ok, "the module starts")
    n0 = b.shot(420, 360, "web-notes-empty")
    d = diff(n0, ref("app-notes-empty.png"))
    verdict(d == 0, "empty notes vs native app-notes-empty.png: %d px differ" % d)
    click(210, 140)
    type_text("first line")
    key("Enter", "Enter", 13)
    type_text("second line")
    click(168, 238)
    n1 = b.shot(420, 360, "web-notes-counted")
    d = diff(n1, ref("app-notes-counted.png"))
    verdict(diff(n0, n1) > 0, "typing two lines (Enter between them) and Count change the picture")
    verdict(d == 0, "notes with two lines + Count vs native app-notes-counted.png: %d px differ" % d)

    print("== files.wasm (380 x 300): a list of rows with symbols ==")
    ok = load("files.wasm", 380, 300)
    verdict(ok, "the module starts")
    f0 = b.shot(380, 300, "web-files-empty")
    d = diff(f0, ref("app-files.png"))
    verdict(d == 0, "file list vs native app-files.png: %d px differ" % d)
    click(190, 110)
    click(190, 138)
    f1 = b.shot(380, 300, "web-files-chosen")
    d = diff(f1, ref("app-files-chosen.png"))
    verdict(diff(f0, f1) > 0, "two clicks on rows change the picture")
    verdict(d == 0, "second row chosen vs native app-files-chosen.png: %d px differ" % d)
finally:
    b.close()
    server.terminate()

if failures:
    print("APPCHECK FAILED (%d)" % len(failures))
    sys.exit(1)
print("APPCHECK PASSED")

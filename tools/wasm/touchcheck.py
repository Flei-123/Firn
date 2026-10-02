#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/touchcheck.py -- REAL POINTERS IN A REAL BROWSER (roadmap r111).
#
# examples/fui/touchpad.fi listens to lib/fui/event.fi through app.on() and
# counts what it hears; demos/webapp/touchpad.wasm is that program in the
# browser. This script drives a headless Chromium with the browser's own
# input events -- Input.dispatchTouchEvent with SEVERAL touch points (every
# finger gets its own pointerId in the page) and Input.dispatchMouseEvent --
# and reads the program's counters back through the exported
# `touchpad_probe` (window.firnExports):
#   * a tap is one tap; the pointer is a touch (type 2)
#   * two fingers are two pointers with two ids (the second finger no mouse)
#   * a pan reports the distance the finger went
#   * a pinch reports the scale (fingers 100 px -> 160 px apart = 160 %) and
#     the turn of the two fingers (to vertical = 90 degrees)
#   * a long press fires after 500 ms, a cancel ends a finger without a tap
#   * a mouse click is a mouse pointer (type 1) and a tap
#   * idle: after the fingers are up nothing is painted
#
# Usage: touchcheck.py <demo dir with touchpad.wasm and firn.js>
import base64
import io
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

import websocket

DEMO = os.path.abspath(sys.argv[1])
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
            raise SystemExit("touchcheck: chromium did not come up")
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
                    raise SystemExit("touchcheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("value")

    def frames(self):
        return self.js("window.firnFrames || 0")

    def close(self):
        try:
            self.ws.close()
        finally:
            self.proc.terminate()
            self.proc.wait()


def verdict(ok, text):
    print("   %-72s %s" % (text, "OK" if ok else "FAILED"))
    if not ok:
        failures.append(text)


hport = free_port()
server = subprocess.Popen([sys.executable, "-m", "http.server", str(hport),
                           "--bind", "127.0.0.1", "--directory", DEMO],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
b = Browser()

NAMES = ["down", "max_down", "taps", "long", "pan_x", "pinch", "angle",
         "last_id", "last_type", "cancels", "ids_seen"]


def probe():
    return {n: b.js("Number(window.firnExports.touchpad_probe(%d))" % i)
            for i, n in enumerate(NAMES)}


def load(wasm, w, h):
    b.call("Emulation.setDeviceMetricsOverride", width=w, height=h,
           deviceScaleFactor=1, mobile=False)
    b.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
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


def touch(kind, pts):
    # pts: [(id, x, y), ...] -- the fingers that are on the glass AFTER the event
    # (touchStart / touchMove) or still on it (touchEnd)
    b.call("Input.dispatchTouchEvent", type=kind,
           touchPoints=[{"x": x, "y": y, "id": i} for (i, x, y) in pts])


def popcount(v):
    return bin(int(v)).count("1")


try:
    print("== touchpad.wasm (360 x 420) ==")
    ok = load("touchpad.wasm", 360, 420)
    verdict(ok, "the module starts and paints a first picture")
    verdict(b.js("typeof window.firnExports.firn_web_pointer_ex") == "function",
            "the module exports firn_web_pointer_ex (the loader uses it)")
    verdict(b.js("getComputedStyle(document.getElementById('firn')).touchAction") == "none",
            "the canvas has touch-action: none")
    p0 = probe()
    verdict(p0["down"] == 0 and p0["taps"] == 0, "nothing heard yet: %s" % p0)

    # ------------------------------------------------------------ one tap
    touch("touchStart", [(1, 180, 300)])
    time.sleep(0.05)
    touch("touchEnd", [])
    time.sleep(0.3)
    p = probe()
    verdict(p["taps"] == 1, "one finger tapped: taps = %d" % p["taps"])
    verdict(p["last_type"] == 2, "the pointer is a touch (type 2): %d" % p["last_type"])
    verdict(p["down"] == 0 and p["max_down"] == 1,
            "one pointer was down, none is now: max %d, now %d" % (p["max_down"], p["down"]))

    # ------------------------------------------------------------ one pan
    base = probe()["pan_x"]
    touch("touchStart", [(1, 100, 340)])
    for k in range(1, 11):
        touch("touchMove", [(1, 100 + 12 * k, 340)])
        time.sleep(0.02)
    touch("touchEnd", [])
    time.sleep(0.3)
    p = probe()
    verdict(abs((p["pan_x"] - base) - 120) <= 2,
            "a finger went 120 px: the pan says %d" % (p["pan_x"] - base))
    verdict(p["taps"] == 1, "a pan is no tap: taps still %d" % p["taps"])

    # ------------------------------------------------------------ a pinch
    before = probe()
    # the first finger id 1, the second id 2: 100 px apart, then 160 px
    touch("touchStart", [(1, 130, 300)])
    touch("touchStart", [(1, 130, 300), (2, 230, 300)])
    time.sleep(0.05)
    mid = probe()
    verdict(mid["down"] == 2 and mid["max_down"] == 2,
            "two fingers = two pointers down: now %d, max %d" % (mid["down"], mid["max_down"]))
    verdict(popcount(mid["ids_seen"]) >= 2,
            "the two pointers have different ids (ids_seen bits: %d)" % popcount(mid["ids_seen"]))
    for k in range(1, 7):
        touch("touchMove", [(1, 130 - 5 * k, 300), (2, 230 + 5 * k, 300)])
        time.sleep(0.02)
    time.sleep(0.1)
    p = probe()
    verdict(abs(p["pinch"] - 160) <= 6, "fingers 100 -> 160 px apart: scale %d %%" % p["pinch"])
    verdict(abs(p["angle"]) <= 3, "moving apart on a line: turn %d degrees" % p["angle"])
    touch("touchEnd", [])
    time.sleep(0.3)
    p = probe()
    verdict(p["down"] == 0, "both fingers up: down = %d" % p["down"])
    verdict(p["taps"] == before["taps"], "a pinch is no tap: taps %d" % p["taps"])

    # ---------------------------------------------------------- a rotation
    touch("touchStart", [(1, 130, 300)])
    touch("touchStart", [(1, 130, 300), (2, 230, 300)])
    steps = 9
    import math
    for k in range(1, steps + 1):
        a = math.radians(90.0 * k / steps)
        cx, cy, r = 180.0, 300.0, 50.0
        touch("touchMove", [(1, cx - r * math.cos(a), cy - r * math.sin(a)),
                            (2, cx + r * math.cos(a), cy + r * math.sin(a))])
        time.sleep(0.02)
    time.sleep(0.1)
    p = probe()
    verdict(abs(abs(p["angle"]) - 90) <= 6, "two fingers turned a quarter: %d degrees" % p["angle"])
    verdict(abs(p["pinch"] - 100) <= 6, "same distance while turning: scale %d %%" % p["pinch"])
    touch("touchEnd", [])
    time.sleep(0.2)

    # ------------------------------------------------- one finger lifts first
    touch("touchStart", [(1, 130, 300)])
    touch("touchStart", [(1, 130, 300), (2, 230, 300)])
    time.sleep(0.05)
    touch("touchEnd", [(2, 230, 300)])   # finger 1 lifts, finger 2 stays
    time.sleep(0.1)
    p = probe()
    verdict(p["down"] == 1, "the first finger lifted, one pointer is still down: %d" % p["down"])
    touch("touchEnd", [])
    time.sleep(0.3)
    p = probe()
    verdict(p["down"] == 0, "then none is down: %d" % p["down"])

    # --------------------------------------------------------- long press
    before = probe()
    touch("touchStart", [(1, 180, 300)])
    time.sleep(0.8)
    touch("touchEnd", [])
    time.sleep(0.2)
    p = probe()
    verdict(p["long"] == before["long"] + 1,
            "held for 0.8 s: one long press (%d -> %d)" % (before["long"], p["long"]))
    verdict(p["taps"] == before["taps"], "a long press is no tap: taps %d" % p["taps"])

    # ------------------------------------------------------------- cancel
    before = probe()
    touch("touchStart", [(1, 180, 300)])
    time.sleep(0.05)
    touch("touchCancel", [])
    time.sleep(0.2)
    p = probe()
    verdict(p["cancels"] == before["cancels"] + 1,
            "touchCancel: one pointer cancelled (%d -> %d)" % (before["cancels"], p["cancels"]))
    verdict(p["taps"] == before["taps"], "a cancel is no tap: taps %d" % p["taps"])
    verdict(p["down"] == 0, "nothing is down after the cancel: %d" % p["down"])

    # ---------------------------------------------------------- the mouse
    before = probe()
    b.call("Input.dispatchMouseEvent", type="mouseMoved", x=180, y=300)
    b.call("Input.dispatchMouseEvent", type="mousePressed", x=180, y=300,
           button="left", buttons=1, clickCount=1)
    b.call("Input.dispatchMouseEvent", type="mouseReleased", x=180, y=300,
           button="left", buttons=0, clickCount=1)
    time.sleep(0.3)
    p = probe()
    verdict(p["last_type"] == 1, "a mouse click is a mouse pointer (type 1): %d" % p["last_type"])
    verdict(p["taps"] == before["taps"] + 1, "and a tap (click): %d -> %d" % (before["taps"], p["taps"]))

    # --------------------------------------------------------------- idle
    f0 = b.frames()
    time.sleep(1.0)
    verdict(b.frames() == f0, "idle 1 s: no new picture (frames %d -> %d)" % (f0, b.frames()))
finally:
    b.close()
    server.terminate()

if failures:
    print("TOUCHCHECK FAILED (%d)" % len(failures))
    sys.exit(1)
print("TOUCHCHECK PASSED")

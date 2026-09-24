#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/gpucheck.py -- THE SAME PAGE IN MEMORY AND ON THE GPU
# (fUi tempo, stage 2, 24.09.2026).
#
#   python3 tools/wasm/gpucheck.py <demo dir> [max_share_per_mille] [max_mean]
#
# Loads demos/webdemo/ into a headless Chromium whose WebGL2 runs on
# SwiftShader (this server has no graphics card) and takes every picture
# twice: with ?gl=0 (fUi paints in memory -- the reference, which
# tools/wasm/webcheck.py holds against the native PNGs pixel for pixel) and
# with ?gl=1 (lib/fui/gpu.fi draws it through WebGL2). Per pair:
#
#   * the share of pixels whose largest channel difference is over 32 (a
#     difference an eye sees at a glance), the share over 8, the mean
#     absolute difference over all channels, the largest one, and a picture
#     of the differences (<n>-diff.png, amplified 4x)
#   * whether the GPU really ran (window.firnGpu), and no script error
#
# The GPU picture may differ where the software rounds differently -- glyphs
# placed at a quarter pixel from an 8-bit atlas, round rectangles in closed
# form instead of the cubic arc -- so the bounds are: at most
# `max_share_per_mille` per mille of the pixels over 32 (default 5, half a
# percent) and a mean under `max_mean` levels (default 1.0).
#
# Then, on the GPU:
#   * idle      no event, no new picture
#   * lost      the browser takes the context away and gives it back
#               (WEBGL_lose_context): the page makes its GPU objects anew
#               and paints the same picture again -- 0 px differ
#   * wheel     the list scrolls a notch and back: the first picture again
#
# Environment: W=<dir> for the pictures (default /tmp/firn-gpucheck),
#              WASM=<file in the demo dir>.   Exit 0 = everything held.
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
MAX_PM = float(sys.argv[2]) if len(sys.argv) > 2 else 5.0
MAX_MEAN = float(sys.argv[3]) if len(sys.argv) > 3 else 1.0
OUT = os.environ.get("W", "/tmp/firn-gpucheck")
WASM = os.environ.get("WASM", "gallery9.wasm")
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
        # WebGL2 on SwiftShader: no --disable-gpu, the software GL allowed
        self.proc = subprocess.Popen(
            ["chromium", "--headless=new", "--no-sandbox", "--use-angle=swiftshader",
             "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
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
            raise SystemExit("gpucheck: chromium did not come up")
        self.ws = websocket.create_connection(url, timeout=60, suppress_origin=True)
        self.next = 0
        self.errors = []
        self.call("Page.enable")
        self.call("Runtime.enable")

    def note(self, m):
        meth = m.get("method", "")
        if meth == "Runtime.exceptionThrown":
            d = m["params"]["exceptionDetails"]
            self.errors.append(d.get("exception", {}).get("description") or d.get("text", "?"))
        elif meth == "Runtime.consoleAPICalled" and m["params"].get("type") == "error":
            a = m["params"].get("args", [])
            self.errors.append(" ".join(str(x.get("value", x.get("description", ""))) for x in a))

    def call(self, method, **params):
        self.next += 1
        me = self.next
        self.ws.send(json.dumps({"id": me, "method": method, "params": params}))
        while True:
            m = json.loads(self.ws.recv())
            if m.get("id") == me:
                if "error" in m:
                    raise SystemExit("gpucheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})
            self.note(m)

    def js(self, expr, wait=False):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True,
                      awaitPromise=wait)
        return r.get("result", {}).get("value")

    def frames(self):
        return self.js("window.firnFrames || 0")

    def quiet(self, timeout=6.0):
        # painted, and then no new picture for 400 ms
        t0 = time.time()
        last, since = -1, time.time()
        while time.time() - t0 < timeout:
            n = self.frames()
            if n != last:
                last, since = n, time.time()
            elif n > 0 and time.time() - since > 0.4:
                return True
            time.sleep(0.05)
        return last > 0

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


def verdict(ok, text):
    print("   %-72s %s" % (text, "OK" if ok else "FAILED"))
    if not ok:
        failures.append(text)


def compare(a, b, name):
    """(per mille over 32, per mille over 8, mean, largest)"""
    if a.shape != b.shape:
        return None
    d = np.abs(a - b)
    m = d.max(axis=2)
    n = m.size
    Image.fromarray(np.minimum(d * 4, 255).astype(np.uint8)).save(
        os.path.join(OUT, name + "-diff.png"))
    return (1000.0 * (m > 32).sum() / n, 1000.0 * (m > 8).sum() / n,
            float(d.mean()), int(m.max()))


hport = free_port()
server = subprocess.Popen([sys.executable, "-m", "http.server", str(hport),
                           "--bind", "127.0.0.1", "--directory", DEMO],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
b = Browser()
try:
    def load(theme, w, h, gl):
        b.call("Emulation.setDeviceMetricsOverride", width=w, height=h,
               deviceScaleFactor=1, mobile=False)
        b.call("Page.navigate", url="http://127.0.0.1:%d/index.html?w=%d&h=%d&theme=%s&wasm=%s&gl=%d"
               % (hport, w, h, theme, WASM, gl))
        time.sleep(0.2)
        ok = b.quiet(20.0)
        err = b.js("document.body && document.body.dataset.error")
        if err:
            print("   page error: %s" % err)
            return False
        return ok

    print("== the page in memory (?gl=0) and on the GPU (?gl=1, WebGL2 on SwiftShader) ==")
    gpu_first = None
    for theme, w in [("dark", 1240), ("light", 1240), ("dark", 980), ("light", 980)]:
        name = "%s-%d" % (theme, w)
        if not load(theme, w, 720, 0):
            verdict(False, "%s: no picture in memory" % name)
            continue
        cpu = b.shot(w, 720, name + "-cpu")
        b.errors = []
        if not load(theme, w, 720, 1):
            verdict(False, "%s: no picture on the GPU" % name)
            continue
        on = b.js("!!window.firnGpu")
        gpu = b.shot(w, 720, name + "-gpu")
        if gpu_first is None:
            gpu_first = (theme, w, gpu)
        verdict(on, "%s: the GPU ran (window.firnGpu)" % name)
        verdict(not b.errors, "%s: no script error on the GPU%s" % (
            name, "" if not b.errors else " (" + " | ".join(b.errors[:3])[:200] + ")"))
        r = compare(cpu, gpu, name)
        if r is None:
            verdict(False, "%s: sizes differ" % name)
            continue
        pm32, pm8, mean, mx = r
        verdict(pm32 <= MAX_PM and mean <= MAX_MEAN,
                "%s: %.2f‰ over 32, %.2f‰ over 8, mean %.3f, largest %d" % (
                    name, pm32, pm8, mean, mx))

    # EVERY WAY fUi DRAWS (tools/fui/gpuzoo_web.fi): the calls gallery9
    # does not make -- pictures, turned shapes and text, drop shadows,
    # glass, text with shadow, outline and gradient, icons
    zoo = os.path.join(DEMO, "gpuzoo.wasm")
    if os.path.exists(zoo):
        print("== every way fUi draws (gpuzoo.wasm), in memory and on the GPU ==")
        for theme in ("light", "dark"):
            name = "zoo-%s" % theme
            def zload(gl):
                b.call("Emulation.setDeviceMetricsOverride", width=1240, height=720,
                       deviceScaleFactor=1, mobile=False)
                b.call("Page.navigate", url="http://127.0.0.1:%d/index.html?w=1240&h=720&theme=%s&wasm=gpuzoo.wasm&gl=%d"
                       % (hport, theme, gl))
                time.sleep(0.2)
                return b.quiet(20.0)
            if not zload(0):
                verdict(False, "%s: no picture in memory" % name)
                continue
            cpu = b.shot(1240, 720, name + "-cpu")
            b.errors = []
            if not zload(1):
                verdict(False, "%s: no picture on the GPU" % name)
                continue
            gpu = b.shot(1240, 720, name + "-gpu")
            verdict(bool(b.js("!!window.firnGpu")) and not b.errors,
                    "%s: on the GPU, no script error, no write missed the GPU%s" % (
                        name, "" if not b.errors else " (" + " | ".join(b.errors[:3])[:200] + ")"))
            r = compare(cpu, gpu, name)
            pm32, pm8, mean, mx = r
            verdict(pm32 <= MAX_PM and mean <= MAX_MEAN,
                    "%s: %.2f\u2030 over 32, %.2f\u2030 over 8, mean %.3f, largest %d" % (
                        name, pm32, pm8, mean, mx))

    print("== operating the page on the GPU (dark, 1240 x 720) ==")
    if load("dark", 1240, 720, 1):
        ref = b.shot(1240, 720, "op-ref")
        n0 = b.frames()
        time.sleep(1.0)
        verdict(b.frames() == n0, "idle: no new picture in 1 s (%d -> %d)" % (n0, b.frames()))
        # the context lost and given back: the same picture again
        b.errors = []
        b.js("""new Promise((res) => {
            const c = document.getElementById('firn');
            const ext = c.getContext('webgl2').getExtension('WEBGL_lose_context');
            ext.loseContext();
            setTimeout(() => { ext.restoreContext(); setTimeout(res, 300); }, 200);
        })""", wait=True)
        b.quiet(6.0)
        lost = b.shot(1240, 720, "op-restored")
        d = int((lost != ref).any(axis=2).sum())
        real = [e for e in b.errors if "context lost" not in e]
        verdict(d == 0 and not real, "lost + restored context: %d px differ from before%s" % (
            d, "" if not real else " (" + " | ".join(real[:2])[:160] + ")"))
        # a notch of the wheel over the list and back
        b.call("Input.dispatchMouseEvent", type="mouseMoved", x=300, y=400)
        n0 = b.frames()
        b.call("Input.dispatchMouseEvent", type="mouseWheel", x=300, y=400, deltaX=0, deltaY=120)
        b.quiet(4.0)
        moved = b.frames() > n0
        b.call("Input.dispatchMouseEvent", type="mouseWheel", x=300, y=400, deltaX=0, deltaY=-120)
        b.quiet(4.0)
        b.call("Input.dispatchMouseEvent", type="mouseMoved", x=1239, y=719)
        b.quiet(4.0)
        back = b.shot(1240, 720, "op-wheel-back")
        # the pointer at the corner leaves no hover behind: compare to the
        # picture with the pointer there
        load("dark", 1240, 720, 1)
        b.call("Input.dispatchMouseEvent", type="mouseMoved", x=1239, y=719)
        b.quiet(4.0)
        ref2 = b.shot(1240, 720, "op-ref2")
        d = int((back != ref2).any(axis=2).sum())
        verdict(moved and d == 0, "wheel a notch and back: %s, %d px differ" % (
            "painted" if moved else "NOTHING PAINTED", d))
    else:
        verdict(False, "dark 1240: no picture on the GPU")
finally:
    b.close()
    server.terminate()

print("GPU PICTURES " + ("WITHIN BOUNDS" if not failures else "NOT WITHIN BOUNDS (%d)" % len(failures)))
sys.exit(0 if not failures else 1)

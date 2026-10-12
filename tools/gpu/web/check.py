#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/web/check.py -- compares what the browser put on its canvas with the NATIVE mc_raster frames.
#   python3 tools/gpu/web/check.py <dir with index.html, glweb.js, mcscene.wasm> <dir with <scene>.cpu.rgba> [min_psnr=34]
# Serves <dir> over http on a free local port, loads it into headless Chromium (WebGL2 on SwiftShader, no GPU on this
# server), takes the DOM after the page finished (--dump-dom), and checks:
#   * the Firn-side lines ("scene ... ok", exit=0, a WebGL2 renderer string) -- the in-module WebGL2-vs-mc_raster compare
#   * per scene: the PNG snapshot of the CANVAS (taken by glweb.js after glweb_show) against the native raw frame:
#     coverage of non-clear pixels equal within 5 permille and PSNR >= min_psnr
# Exit 0 = all held. Needs chromium, python3 with PIL and numpy.
import base64, io, math, os, re, socket, subprocess, sys, threading, http.server, functools
import numpy as np
from PIL import Image

web, ref = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
MIN = float(sys.argv[3]) if len(sys.argv) > 3 else 34.0
W, H = 320, 180
CLEAR = (0x96, 0xBE, 0xE6)  # 0xFFE6BE96 = A B G R

s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a, **k): pass
handler = functools.partial(Quiet, directory=web)
srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
threading.Thread(target=srv.serve_forever, daemon=True).start()
try:
    dom = subprocess.run(
        ["chromium", "--headless=new", "--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader",
         "--ignore-gpu-blocklist", "--virtual-time-budget=30000", "--dump-dom", "http://127.0.0.1:%d/index.html" % port],
        capture_output=True, text=True, timeout=120).stdout
finally:
    srv.shutdown()

fail = []
m = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
out = m.group(1) if m else ""
import html
out = html.unescape(out)
print("\n".join("   " + l for l in out.strip().splitlines()))
scenes = re.findall(r"^scene (\S+) .* (ok|FAIL)$", out, re.M)
if len(scenes) < 7 or any(r != "ok" for _, r in scenes): fail.append("in-module compare: %d scenes, not all ok" % len(scenes))
if "exit=0" not in out: fail.append("exit code not 0")
if "WebGL 2.0" not in out: fail.append("no WebGL2 renderer line")
imgs = re.findall(r'<img src="data:image/png;base64,([^"]+)"', dom)
names = [n for n, _ in scenes]
if len(imgs) != len(names): fail.append("%d canvas snapshots for %d scenes" % (len(imgs), len(names)))
for n, b in zip(names, imgs):
    a = np.array(Image.open(io.BytesIO(base64.b64decode(b))).convert("RGB"), dtype=np.int32)
    f = os.path.join(ref, n + ".cpu.rgba")
    if not os.path.exists(f): fail.append(n + ": no native reference"); continue
    r = np.fromfile(f, dtype=np.uint8).reshape(H, W, 4)[:, :, :3].astype(np.int32)
    if a.shape != r.shape: fail.append("%s: size %s" % (n, a.shape)); continue
    mse = float(((a - r) ** 2).mean())
    psnr = 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
    cov_b = int((np.abs(a - np.array(CLEAR)).sum(axis=2) > 0).sum()); cov_n = int((np.abs(r - np.array(CLEAR)).sum(axis=2) > 0).sum())
    mm = abs(cov_b - cov_n) * 1000 // (W * H)
    ok = psnr >= MIN and mm <= 5
    print("   canvas vs native %-12s psnr=%.2f cov_browser=%d cov_native=%d %s" % (n, psnr, cov_b, cov_n, "ok" if ok else "FAIL"))
    if not ok: fail.append("%s: psnr %.2f cov mm %d" % (n, psnr, mm))
print("RESULT webgl2: " + ("ok" if not fail else "FAIL " + "; ".join(fail)))
sys.exit(1 if fail else 0)

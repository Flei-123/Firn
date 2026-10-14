#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/mc/web/check.py -- r106: the browser hills demo (demos/mcweb/hills.html?check=1) in headless Chromium against the native frames.
#   python3 -I tools/mc/web/check.py <dir with hills.html, glweb.js, hills.wasm> <dir with hills_<frame>.cpu.rgba>
# Serves <dir> over http, loads the page twice (WebGL2 on SwiftShader, no GPU here), and checks:
#   * Firn lines: "hills web ready", one "hills snap F ... glerr=0" per snapshot, key mask seen by the frame == the script (webraw works),
#     exit=0, frames=40, a WebGL 2.0 renderer line
#   * canvas PNG per snapshot frame vs the native CPU frame: PSNR >= 34 dB, coverage mismatch <= 5 permille
#   * snapshots of different frames differ (navigation moves the camera); run 1 and run 2 give identical canvas hashes (deterministic)
# Prints the SHA-256 of every canvas frame. Exit 0 = all held. Needs chromium, python3 with PIL and numpy.
import base64, hashlib, html, io, math, os, re, socket, subprocess, sys, threading, http.server, functools
import numpy as np
from PIL import Image

web, ref = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
MIN = 34.0
W, H = 320, 180
CLEAR = (0x96, 0xBE, 0xE6)
SNAPS = [0, 8, 16, 24, 39]
KEYS = {0: 0, 8: 1, 16: 8, 24: 2, 39: 0}   # expected key mask the frame saw (hills.script_keys)

s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a, **k): pass
srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(Quiet, directory=web))
threading.Thread(target=srv.serve_forever, daemon=True).start()

def run_page():
    dom = subprocess.run(
        ["chromium", "--headless=new", "--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader",
         "--ignore-gpu-blocklist", "--virtual-time-budget=30000", "--dump-dom",
         "http://127.0.0.1:%d/hills.html?check=1&wasm=hills.wasm" % port],
        capture_output=True, text=True, timeout=120).stdout
    m = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
    out = html.unescape(m.group(1)) if m else ""
    imgs = re.findall(r'<img src="data:image/png;base64,([^"]+)"[^>]*data-frame="(\d+)"', dom)
    return out, {int(f): base64.b64decode(b) for b, f in imgs}

try:
    out1, im1 = run_page()
    out2, im2 = run_page()
finally:
    srv.shutdown()

fail = []
print("\n".join("   " + l for l in out1.strip().splitlines()))
if "hills web ready" not in out1: fail.append("module did not start")
if "exit=0 frames=40" not in out1: fail.append("exit/frames line missing (expected exit=0 frames=40)")
if "WebGL 2.0" not in out1: fail.append("no WebGL2 renderer line")
snaps = {int(f): (int(t), int(k), int(e)) for f, t, k, e in re.findall(r"^hills snap (\d+) tris=(\d+) keys=(\d+) glerr=(\d+)$", out1, re.M)}
for f in SNAPS:
    if f not in snaps: fail.append("no Firn line for frame %d" % f); continue
    t, k, e = snaps[f]
    if e != 0: fail.append("frame %d: GL error %d" % (f, e))
    if k != KEYS[f]: fail.append("frame %d: key mask %d, expected %d (input injection/webraw)" % (f, k, KEYS[f]))
    if t == 0: fail.append("frame %d: 0 triangles" % f)
hashes = {}
frames = {}
for f in SNAPS:
    if f not in im1: fail.append("no canvas snapshot for frame %d" % f); continue
    a = np.array(Image.open(io.BytesIO(im1[f])).convert("RGB"), dtype=np.int32)
    frames[f] = a
    hashes[f] = hashlib.sha256(a.astype(np.uint8).tobytes()).hexdigest()
    fn = os.path.join(ref, "hills_%04d.cpu.rgba" % f)
    if not os.path.exists(fn): fail.append("frame %d: no native reference" % f); continue
    r = np.fromfile(fn, dtype=np.uint8).reshape(H, W, 4)[:, :, :3].astype(np.int32)
    if a.shape != r.shape: fail.append("frame %d: size %s" % (f, a.shape)); continue
    mse = float(((a - r) ** 2).mean())
    psnr = 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
    cb = int((np.abs(a - np.array(CLEAR)).sum(axis=2) > 0).sum()); cn = int((np.abs(r - np.array(CLEAR)).sum(axis=2) > 0).sum())
    mm = abs(cb - cn) * 1000 // (W * H)
    ok = psnr >= MIN and mm <= 5
    print("   canvas vs native frame %2d psnr=%.2f cov_browser=%d cov_native=%d sha256=%s %s" % (f, psnr, cb, cn, hashes[f][:16], "ok" if ok else "FAIL"))
    if not ok: fail.append("frame %d: psnr %.2f cov mm %d" % (f, psnr, mm))
# navigation moves the camera: every snapshot differs from the previous one
for i in range(1, len(SNAPS)):
    p, q = SNAPS[i - 1], SNAPS[i]
    if p in hashes and q in hashes and hashes[p] == hashes[q]: fail.append("frames %d and %d are identical (camera did not move)" % (p, q))
# determinism: second page load gives the same canvas bytes
same = all(f in im2 and hashlib.sha256(np.array(Image.open(io.BytesIO(im2[f])).convert("RGB")).tobytes()).hexdigest() == hashes.get(f) for f in SNAPS)
print("   run 2 canvas hashes %s run 1" % ("==" if same else "!="))
if not same: fail.append("canvas not deterministic between two loads")
print("RESULT mc-web: " + ("ok" if not fail else "FAIL " + "; ".join(fail)))
sys.exit(1 if fail else 0)

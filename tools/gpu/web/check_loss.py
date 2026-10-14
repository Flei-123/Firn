#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/web/check_loss.py <dir with index.html, glweb.js, loss.wasm> -- serves the dir, runs it in headless Chromium, checks the log.
import html, os, re, socket, subprocess, sys, threading, http.server, functools
web = os.path.abspath(sys.argv[1])
s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a, **k): pass
srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(Quiet, directory=web))
threading.Thread(target=srv.serve_forever, daemon=True).start()
try:
    dom = subprocess.run(
        ["chromium", "--headless=new", "--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader",
         "--ignore-gpu-blocklist", "--virtual-time-budget=20000", "--dump-dom", "http://127.0.0.1:%d/index.html" % port],
        capture_output=True, text=True, timeout=120).stdout
finally:
    srv.shutdown()
m = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
out = html.unescape(m.group(1)) if m else ""
print("\n".join("   " + l for l in out.strip().splitlines()))
fail = []
if "page: context lost" not in out: fail.append("browser never reported webglcontextlost")
if "page: context restored" not in out: fail.append("browser never reported webglcontextrestored")
if not re.search(r"lost-frames=[1-9]", out): fail.append("Firn side never saw CTX_LOST")
if "restores=1" not in out: fail.append("Firn side did not see exactly one CTX_RESTORED")
if "same=1" not in out: fail.append("frame after restore differs from the frame before the loss")
if "context loss ok" not in out: fail.append("no 'context loss ok' line")
print("RESULT webgl2-context-loss: " + ("ok" if not fail else "FAIL " + "; ".join(fail)))
sys.exit(1 if fail else 0)

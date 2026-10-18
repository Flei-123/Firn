#!/usr/bin/env python3
"""Capture reference screenshots of the mobile Jarvis web app with MOCK data.

Starts mockserver.py on a free local port (no real service, no login), then runs
capture.mjs (playwright + system chromium) at 412x915 CSS px, dpr 1 and 3.
usage: capture.py [--out DIR] [--dpr 1,3] [--themes fleitec,claude] [--screens chat,board]
"""
import argparse, os, subprocess, sys, time, socket, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

def free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p

def shrink(out, limit=150 * 1024):
    """Re-save PNGs above `limit` with a 256-colour palette (the dark gradients bloat 24-bit PNGs)."""
    try: from PIL import Image
    except ImportError: return
    for n in os.listdir(out):
        f = os.path.join(out, n)
        if n.endswith(".png") and "@" not in n and os.path.getsize(f) > limit:
            Image.open(f).convert("RGB").quantize(256, dither=Image.Dither.NONE).save(f, optimize=True)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=HERE)
    ap.add_argument("--dpr", default="1,3")
    ap.add_argument("--themes", default="fleitec,claude")
    ap.add_argument("--screens", default="")
    ap.add_argument("--public", default="/root/jarvis/public")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    port = free_port()
    srv = subprocess.Popen([sys.executable, os.path.join(HERE, "mockserver.py"), "--port", str(port), "--public", a.public])
    try:
        for _ in range(50):
            try: urllib.request.urlopen(f"http://127.0.0.1:{port}/api/me", timeout=1); break
            except Exception: time.sleep(0.1)
        r = subprocess.run(["node", os.path.join(HERE, "capture.mjs"), f"http://127.0.0.1:{port}", a.out, a.dpr, a.themes, a.screens])
        shrink(a.out)
        return r.returncode
    finally:
        srv.terminate()

if __name__ == "__main__":
    sys.exit(main())

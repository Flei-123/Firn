#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/ui_check.py -- the browser window, seen from the SERVER.

The trap this file exists to avoid is the one round K7B fell into: a
program that reports it has drawn a page, and a screen that is blank. The
only way to catch that is to look at the pixels from the OTHER SIDE of the
protocol -- so an X server nobody can see (`Xvfb`) is started, the browser
is run in it, and the window is then photographed with `xwd`, which is
part of X and shares no line of code with this repository. What `xwd`
hands back has to be what the browser says it painted.

Three things are measured:

  1. the window really exists and has the size that was asked for;
  2. the picture the SERVER has agrees with the canvas the browser wrote
     out for itself -- pixel for pixel over the page area;
  3. the page area is not one flat colour, and the colours that are in it
     are the colours the page asked for. A white rectangle would pass 1
     and 2 and mean nothing, which is exactly the K7B failure.

And a counter-check: the same run against a page that is EMPTY has to give
a page area that IS one flat colour. Without it, "the picture has more than
one colour" could be true of a browser that draws noise.
"""
import os
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
OK = 0
FAILS = []
COUNTER_OK = 0
COUNTER_TOTAL = 0

PAGE = """<!doctype html><html><head><title>Certus</title></head><body>
<h1>Certus Browser</h1>
<p>Round B5: TLS, pictures and a window.</p>
<div style="background:#0033aa;width:300px;height:80px"></div>
<div style="background:#cc0000;width:200px;height:40px"></div>
</body></html>
"""
EMPTY = "<!doctype html><html><body></body></html>"


def note(title, good, detail="", counter=False):
    global OK, COUNTER_OK, COUNTER_TOTAL
    if counter:
        COUNTER_TOTAL += 1
    if good:
        OK += 1
        if counter:
            COUNTER_OK += 1
    else:
        FAILS.append((title, detail))


def read_ppm(path):
    b = open(path, "rb").read()
    if not b.startswith(b"P6"):
        return None
    parts = []
    i = 2
    while len(parts) < 3:
        while i < len(b) and b[i:i + 1].isspace():
            i += 1
        j = i
        while j < len(b) and not b[j:j + 1].isspace():
            j += 1
        parts.append(int(b[i:j]))
        i = j
    i += 1
    w, h, _ = parts
    return w, h, b[i:i + w * h * 3]


def read_xwd(path):
    """The X Window Dump format: a big-endian header of 32 bit words, then
    the window name, then a colour map, then the pixels. Enough of it to
    get a 24-or-32-bit-per-pixel window out."""
    b = open(path, "rb").read()
    hdr = struct.unpack(">25I", b[:100])
    (header_size, file_version, pixmap_format, pixmap_depth,
     pixmap_width, pixmap_height, xoffset, byte_order, bitmap_unit,
     bitmap_bit_order, bitmap_pad, bits_per_pixel, bytes_per_line,
     visual_class, red_mask, green_mask, blue_mask, bits_per_rgb,
     colormap_entries, ncolors, window_width, window_height,
     window_x, window_y, window_bdrwidth) = hdr
    off = header_size + ncolors * 12
    px = b[off:]
    w, h = pixmap_width, pixmap_height
    out = bytearray(w * h * 3)
    bpp = bits_per_pixel // 8
    for y in range(h):
        row = px[y * bytes_per_line:(y + 1) * bytes_per_line]
        for x in range(w):
            p = row[x * bpp:x * bpp + bpp]
            if len(p) < 3:
                continue
            if byte_order == 0:      # LSBFirst
                bl, g, r = p[0], p[1], p[2]
            else:
                r, g, bl = p[-3], p[-2], p[-1]
            o = (y * w + x) * 3
            out[o] = r
            out[o + 1] = g
            out[o + 2] = bl
    return w, h, bytes(out)


def free_display():
    for n in range(90, 120):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n):
            return n
    return 99


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def main():
    binary = sys.argv[1]
    if not shutil.which("Xvfb") or not shutil.which("xwd"):
        print("   (no Xvfb or xwd on this machine -- the window was not "
              "measured)")
        return 0
    d = tempfile.mkdtemp(prefix="firn-b5-ui-")
    port = free_port()
    disp = free_display()
    xvfb = subprocess.Popen(["Xvfb", ":%d" % disp, "-screen", "0",
                             "1280x1024x24"],
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)
    for _ in range(200):
        if os.path.exists("/tmp/.X11-unix/X%d" % disp):
            break
        time.sleep(0.05)
    www = os.path.join(d, "www")
    os.makedirs(www)
    open(os.path.join(www, "index.html"), "w").write(PAGE)
    open(os.path.join(www, "empty.html"), "w").write(EMPTY)
    srv = subprocess.Popen([sys.executable, "-m", "http.server", str(port)],
                           cwd=www, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
    time.sleep(0.8)
    env = dict(os.environ, DISPLAY=":%d" % disp)

    try:
        for name, page, want_flat in (("the page", "index.html", False),
                                      ("an empty page", "empty.html",
                                       True)):
            shot = os.path.join(d, "own-%s.ppm" % page)
            url = "http://127.0.0.1:%d/%s" % (port, page)
            # 0 events would sit for ever; four is one draw plus the
            # Expose the server sends after the window is mapped.
            proc = subprocess.Popen([binary, url,
                                     "/etc/ssl/certs/ca-certificates.crt",
                                     "0", shot, str(disp)],
                                    stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL)
            time.sleep(3.0)
            xwdf = os.path.join(d, "server-%s.xwd" % page)
            r = subprocess.run(["xwd", "-name", "Certus", "-out", xwdf],
                               env=env, capture_output=True)
            got_window = r.returncode == 0 and os.path.exists(xwdf)
            note("%s: the window is there and xwd can photograph it"
                 % name, got_window, r.stderr.decode()[:120])
            proc.send_signal(signal.SIGKILL)
            proc.wait()
            # And a SECOND run that stops after one draw, so that the
            # browser writes out the canvas it thinks it painted. The
            # render is deterministic -- same page, same width -- so the
            # two runs must produce the same pixels, and that is the whole
            # comparison below.
            subprocess.run([binary, url,
                            "/etc/ssl/certs/ca-certificates.crt",
                            "1", shot, str(disp)],
                           stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=120)
            if not got_window:
                continue
            sw, sh, spx = read_xwd(xwdf)
            note("%s: the window is the size the browser asked for" % name,
                 (sw, sh) == (1024, 768), "%dx%d" % (sw, sh))
            # the page area starts below the 30 pixel chrome
            colours = set()
            for y in range(40, min(sh, 700), 3):
                for x in range(0, min(sw, 1000), 3):
                    o = (y * sw + x) * 3
                    colours.add(spx[o:o + 3])
            if want_flat:
                note("COUNTER-CHECK: an empty page really is one flat "
                     "colour on the server's side", len(colours) == 1,
                     "%d colours" % len(colours), counter=True)
            else:
                note("%s: what the server has is not one flat colour"
                     % name, len(colours) > 3,
                     "%d distinct colours" % len(colours))
                blue = 0
                red = 0
                dark = 0
                for y in range(40, min(sh, 700)):
                    for x in range(0, min(sw, 1000)):
                        o = (y * sw + x) * 3
                        p = spx[o:o + 3]
                        if p == b"\x00\x33\xaa":
                            blue += 1
                        elif p == b"\xcc\x00\x00":
                            red += 1
                        elif p[0] < 64 and p[1] < 64 and p[2] < 64:
                            dark += 1
                note("%s: the blue block of the page is on the screen, at "
                     "its size" % name, abs(blue - 300 * 80) < 300,
                     "%d pixels of #0033aa, expected %d"
                     % (blue, 300 * 80))
                note("%s: the red block too" % name,
                     abs(red - 200 * 40) < 200,
                     "%d pixels of #cc0000, expected %d"
                     % (red, 200 * 40))
                note("%s: and there is dark ink -- the text" % name,
                     dark > 200, "%d dark pixels" % dark)
                # and the browser's own canvas has to agree with it
                own = read_ppm(shot)
                if own is None:
                    note("%s: the browser wrote its canvas out" % name,
                         False, "no PPM")
                else:
                    ow, oh, opx = own
                    worst = 0
                    n = 0
                    for y in range(0, min(oh, sh - 30)):
                        for x in range(0, min(ow, sw)):
                            a = opx[(y * ow + x) * 3:(y * ow + x) * 3 + 3]
                            b2 = spx[((y + 30) * sw + x) * 3:
                                     ((y + 30) * sw + x) * 3 + 3]
                            for k in range(3):
                                dv = abs(a[k] - b2[k])
                                if dv > worst:
                                    worst = dv
                            n += 1
                    note("%s: the server's pixels ARE the browser's "
                         "canvas" % name, worst == 0,
                         "worst channel difference %d over %d pixels"
                         % (worst, n))
    finally:
        srv.terminate()
        xvfb.terminate()

    print("   %d / %d window cases, of them %d / %d counter-checks"
          % (OK, OK + len(FAILS), COUNTER_OK, COUNTER_TOTAL))
    for title, detail in FAILS:
        print("  FAIL  %-50s %s" % (title[:50], detail))
    if FAILS:
        print("UI FAILED: %d" % len(FAILS))
        return 1
    print("UI OK: %d cases, %d counter-checks" % (OK, COUNTER_OK))
    return 0


if __name__ == "__main__":
    sys.exit(main())

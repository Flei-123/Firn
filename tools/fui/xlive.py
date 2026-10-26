# SPDX-License-Identifier: MPL-2.0
import os
import struct
import subprocess
import time

from PIL import Image, ImageChops

SCREEN_W = 1100
SCREEN_H = 720


def start_xvfb(screen=(SCREEN_W, SCREEN_H), ready_timeout=30.0):
    base = 100 + os.getpid() % 800
    for step in range(60):
        number = 100 + (base - 100 + step * 7) % 800
        if os.path.exists("/tmp/.X%d-lock" % number):
            continue
        disp = ":%d" % number
        proc = subprocess.Popen(
            ["Xvfb", disp, "-screen", "0", "%dx%dx24" % screen],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        env = dict(os.environ, DISPLAY=disp)
        deadline = time.time() + ready_timeout
        while time.time() < deadline and proc.poll() is None:
            probe = subprocess.run(["xdotool", "getdisplaygeometry"], env=env,
                                   capture_output=True, text=True)
            if probe.returncode == 0 and probe.stdout.split() == [str(screen[0]), str(screen[1])]:
                return proc, disp
            time.sleep(0.1)
        proc.terminate()
        try:
            proc.wait(3)
        except Exception:
            proc.kill()
    raise RuntimeError("no X display could be started")


def decode_xwd(data):
    head = struct.unpack(">25I", data[:100])
    (hsize, version, fmt, depth, w, h, xoff, byte_order, bunit, bbo, bpad,
     bpp, bpl, vclass, rmask, gmask, bmask, bprgb, cmap_n, ncolors) = head[:20]
    if version != 7 or fmt != 2 or bpp != 32:
        raise ValueError("unsupported xwd: version %d format %d bpp %d" % (version, fmt, bpp))
    at = hsize + ncolors * 12
    if len(data) < at + bpl * h:
        raise ValueError("truncated xwd")
    px = data[at:at + bpl * h]
    masks = (rmask, gmask, bmask)
    if byte_order == 0 and masks == (0xFF0000, 0xFF00, 0xFF):
        mode = "BGRX"
    elif byte_order == 1 and masks == (0xFF0000, 0xFF00, 0xFF):
        mode = "XRGB"
    else:
        raise ValueError("unsupported xwd masks")
    return Image.frombuffer("RGB", (w, h), px, "raw", mode, bpl, 1).copy()


def capture(disp, directory, name, size=(SCREEN_W, SCREEN_H), attempts=8):
    last = None
    for attempt in range(attempts):
        try:
            done = subprocess.run(["xwd", "-root", "-silent", "-display", disp],
                                  capture_output=True, check=True)
            img = decode_xwd(done.stdout)
            if img.size == size:
                return img
            last = "size %dx%d" % img.size
        except Exception as exc:
            last = str(exc)
        time.sleep(0.2 * (attempt + 1))
    raise RuntimeError("screenshot %s failed: %s" % (name, last))


def count_diff(a, b, box, threshold=60):
    x0, y0, x1, y1 = box
    x0 = max(0, min(x0, a.size[0], b.size[0]))
    x1 = max(x0, min(x1, a.size[0], b.size[0]))
    y0 = max(0, min(y0, a.size[1], b.size[1]))
    y1 = max(y0, min(y1, a.size[1], b.size[1]))
    if x1 == x0 or y1 == y0:
        return 0
    region = (x0, y0, x1, y1)
    delta = ImageChops.difference(a.crop(region), b.crop(region))
    r, g, bl = delta.split()
    total = ImageChops.add(ImageChops.add(r, g), bl)
    return total.point(lambda v: 255 if v > threshold else 0).histogram()[255]


def wait_until(grab, pred, action=None, timeout=20.0, retry_after=5.0, hold=2, gap=0.15):
    deadline = time.time() + timeout
    retry_at = time.time() + retry_after
    streak = 0
    img = grab()
    while True:
        if pred(img):
            streak += 1
            if streak >= hold:
                return img, True
            time.sleep(gap)
        else:
            streak = 0
            if time.time() >= deadline:
                return img, False
            if action is not None and time.time() >= retry_at:
                action()
                retry_at = time.time() + retry_after
            time.sleep(gap)
        img = grab()

import sys, os, random, subprocess, tempfile
from PIL import Image

BIN = "/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin"
out = sys.argv[1]
random.seed(11)

def make(w, h, ch, noise_bits=1):
    im = Image.new({1: "L", 2: "LA", 3: "RGB", 4: "RGBA"}[ch], (w, h))
    px = []
    for y in range(h):
        for x in range(w):
            base = (x * 255 // max(1, w - 1) + (y // 7) * 9 + ((x // 11) ^ (y // 13)) * 5) & 255
            v = []
            for c in range(ch):
                n = random.randrange(1 << noise_bits) if noise_bits else 0
                v.append(((base + c * 53 + n) & 255) if c < 3 or ch in (1, 2) and c == 0 else ((x * 3 + y) & 255))
            px.append(tuple(v) if ch > 1 else v[0])
    im.putdata(px)
    return im

def read_pam(path):
    data = open(path, "rb").read()
    hdr, rest = data.split(b"ENDHDR\n", 1)
    f = dict(l.split() for l in hdr.decode().split("\n")[1:] if l)
    w, h, d = int(f["WIDTH"]), int(f["HEIGHT"]), int(f["DEPTH"])
    assert int(f["MAXVAL"]) == 255
    return w, h, d, rest

def plane_hash(w, h, d, raw):
    s = 0
    for c in range(d):
        for i in range(w * h):
            s = (s * 31 + raw[i * d + c]) % 1000000007
    return s

cases = [
    ("mg_rgb300x260_g0", (300, 260, 3), ["-g", "0", "-e", "3"]),
    ("mg_rgb520x300_g1", (520, 300, 3), ["-e", "5"]),
    ("mg_rgb300x200_wp", (300, 200, 3), ["-g", "0", "-P", "6", "-e", "4"]),
    ("mg_rgba300x200_g0", (300, 200, 4), ["-g", "0", "-e", "4"]),
    ("mg_grey260x130_g0", (260, 130, 1), ["-g", "0", "-e", "3"]),
    ("mg_greya200x140_g0", (200, 140, 2), ["-g", "0", "-e", "3"]),
    ("mg_rgb300x260_order", (300, 260, 3), ["-g", "0", "-e", "3", "--group_order=1"]),
    ("sq_rgb100x90", (100, 90, 3), ["-R", "1", "-e", "3"]),
    ("sq_rgb37x23", (37, 23, 3), ["-R", "1", "-e", "3"]),
    ("sq_rgb300x200_g0", (300, 200, 3), ["-R", "1", "-g", "0", "-e", "3"]),
    ("sq_rgba150x130_g0", (150, 130, 4), ["-R", "1", "-g", "0", "-e", "3"]),
    ("sq_grey129x65", (129, 65, 1), ["-R", "1", "-e", "3"]),
    ("sq_rgb513x7", (513, 7, 3), ["-R", "1", "-e", "3"]),
    ("sq_rgb7x300", (7, 300, 3), ["-R", "1", "-e", "3"]),
    ("sq_rgb200x150_wp", (200, 150, 3), ["-R", "1", "-g", "0", "-P", "6", "-e", "4"]),
    ("pred_rgb120x100_p0", (120, 100, 3), ["-P", "0", "-e", "3"]),
    ("pred_rgb120x100_p4", (120, 100, 3), ["-P", "4", "-e", "3"]),
    ("pred_rgb120x100_p13", (120, 100, 3), ["-P", "13", "-e", "3"]),
    ("pred_rgb120x100_p15", (120, 100, 3), ["-P", "15", "-e", "9"]),
    ("prog_rgb300x200", (300, 200, 3), ["-p", "-g", "0", "-e", "3"]),
]
lines = []
with tempfile.TemporaryDirectory() as td:
    for name, (w, h, ch), opts in cases:
        im = make(w, h, ch)
        png = os.path.join(td, name + ".png")
        im.save(png)
        jxl = os.path.join(out, name + ".jxl")
        r = subprocess.run([BIN + "/cjxl", png, jxl, "-d", "0", "-m", "1"] + opts, capture_output=True, text=True)
        if r.returncode != 0:
            print("FAIL", name, r.stderr[-200:], file=sys.stderr)
            continue
        pam = os.path.join(td, name + ".pam")
        r = subprocess.run([BIN + "/djxl", jxl, pam], capture_output=True, text=True)
        if r.returncode != 0:
            print("DJXL FAIL", name, r.stderr[-200:], file=sys.stderr)
            continue
        pw, ph, d, raw = read_pam(pam)
        lines.append('    expect("tests/data/jxl/mg/%s.jxl", %d, %d)' % (name, d, plane_hash(pw, ph, d, raw)))
        print(name, os.path.getsize(jxl), file=sys.stderr)
print("\n".join(lines))

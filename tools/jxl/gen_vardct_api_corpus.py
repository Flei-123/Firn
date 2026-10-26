import sys, math, random
from PIL import Image
out = sys.argv[1]
random.seed(11)

def pixel(x, y):
    return (int(127 + 100 * math.sin(x / 23.0 + y / 41.0)), int(127 + 90 * math.cos(y / 13.0 + x / 71.0)), (x * y >> 5) & 255)

def build(name, w, h, alpha):
    im = Image.new("RGBA" if alpha else "RGB", (w, h))
    px = []
    for y in range(h):
        for x in range(w):
            r, g, b = pixel(x, y)
            if alpha:
                px.append((r, g, b, (x * 255 // max(w - 1, 1) + y) & 255))
            else:
                px.append((r, g, b))
    im.putdata(px)
    im.save(f"{out}/{name}.png")

build("rgba64", 64, 48, True)
build("rgba300", 300, 200, True)
build("prog300", 300, 200, False)
build("prog600", 600, 300, False)

def ppm16(name, w, h):
    with open(f"{out}/{name}.ppm", "wb") as f:
        f.write(b"P6\n%d %d\n65535\n" % (w, h))
        data = bytearray()
        for y in range(h):
            for x in range(w):
                r, g, b = pixel(x, y)
                for v in (r, g, b):
                    s = v * 257 + ((x * 7 + y * 3) & 255)
                    s = min(s, 65535)
                    data += bytes((s >> 8, s & 255))
        f.write(data)

ppm16("d16_64", 64, 48)
ppm16("d16_300", 300, 200)

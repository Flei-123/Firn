import sys, random, math
from PIL import Image
out = sys.argv[1]
random.seed(11)
def img(w, h, f):
    im = Image.new("RGB", (w, h))
    im.putdata([f(x, y) for y in range(h) for x in range(w)])
    return im
def smooth(x, y):
    return (int(127 + 100 * math.sin(x / 9.0)), int(127 + 100 * math.cos(y / 7.0)), (x * 3 + y * 2) & 255)
def texture(x, y):
    n = random.randrange(40)
    return ((x * 5 + n) & 255, (y * 4 + n) & 255, ((x ^ y) * 6 + n) & 255)
for name, w, h, f in [("s64", 64, 64, smooth), ("t64", 64, 64, texture), ("s300x200", 300, 200, smooth), ("t37x29", 37, 29, texture), ("s600x300", 600, 300, smooth)]:
    img(w, h, f).save(f"{out}/{name}.png")

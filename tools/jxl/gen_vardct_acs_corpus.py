import sys, random, math
from PIL import Image
out = sys.argv[1]
random.seed(5)
W, H = 640, 480
def make(name, f, box):
    im = Image.new("RGB", (W, H))
    im.putdata([f(x, y) for y in range(H) for x in range(W)])
    im.crop(box).save(f"{out}/{name}.png")
make("acsmix", lambda x, y: (int(127 + 100 * math.sin(x / 37.0 + y / 53.0)), int(127 + 90 * math.cos(y / 11.0)), (x * y >> 6) & 255) if (x // 64 + y // 64) % 2 else (200, 100 + int(x / 10), 60), (0, 0, 256, 192))
make("acsedges", lambda x, y: ((255 if ((x // 3) + (y // 5)) % 2 else 20), (x * 2) & 255, (y * 3) & 255), (0, 0, 320, 192))
make("acsnoise", lambda x, y: (random.randrange(256) if (x // 32 + y // 32) % 3 == 0 else 100 + (x >> 2), 80 + ((x + y) >> 3), 150), (0, 0, 320, 192))

import math
import sys

from PIL import Image

SIZE = 256
out = sys.argv[1]
pixels = []
for y in range(SIZE):
    for x in range(SIZE):
        r = int(127 + 100 * math.sin(x / 41.0 + y / 57.0))
        g = int(127 + 90 * math.cos(y / 23.0) * math.sin(x / 90.0))
        if (x // 64 + y // 64) % 2:
            b = (x * 3 + y * 2) & 255
        else:
            b = 90 + int(60 * math.sin((x + y) / 30.0))
        pixels.append((r, g, b))
image = Image.new("RGB", (SIZE, SIZE))
image.putdata(pixels)
image.save(out)

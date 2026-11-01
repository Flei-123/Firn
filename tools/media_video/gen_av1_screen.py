import random
import sys
from PIL import Image, ImageDraw

PALETTE = [0, 40, 90, 128, 180, 220, 255]


def color(rng):
    return tuple(rng.choice(PALETTE) for _ in range(3))


def build(width, height, seed, repeat):
    rng = random.Random(seed)
    img = Image.new("RGB", (width, height), (240, 240, 240))
    draw = ImageDraw.Draw(img)
    for _ in range(max(6, width * height // 1200)):
        x = rng.randint(0, width - 1)
        y = rng.randint(0, height - 1)
        draw.rectangle([x, y, x + rng.randint(4, 48), y + rng.randint(3, 28)], fill=color(rng))
    for i in range(max(4, width * height // 3000)):
        draw.text((rng.randint(0, max(0, width - 20)), rng.randint(0, max(0, height - 10))), "Firn AV1 %d" % i, fill=color(rng))
    for _ in range(width * height // 4000):
        x = rng.randint(0, width - 1)
        y = rng.randint(0, height - 1)
        draw.line([x, y, x + rng.randint(-30, 30), y + rng.randint(-30, 30)], fill=color(rng))
    if repeat:
        tile = img.crop((0, 0, min(32, width), min(24, height)))
        for ty in range(0, height, 24):
            for tx in range(0, width, 32):
                if rng.random() < 0.5:
                    img.paste(tile, (tx, ty))
    return img


if __name__ == "__main__":
    out, width, height, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
    repeat = len(sys.argv) > 5 and sys.argv[5] == "repeat"
    build(width, height, seed, repeat).save(out)

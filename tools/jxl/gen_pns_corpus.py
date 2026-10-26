import sys
from PIL import Image, ImageDraw, ImageFont
out = sys.argv[1]
w, h = 256, 192
im = Image.new("RGB", (w, h), (235, 230, 215))
d = ImageDraw.Draw(im)
f = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 14)
for y in range(8, h - 16, 18):
    d.text((8, y), "the quick brown fox jumps over the lazy dog " * 2, fill=(20, 20, 30), font=f)
im.save(f"{out}/text.png")

import sys, io, random
from PIL import Image
import pillow_jxl
out = sys.argv[1]
random.seed(7)
def noise(w,h,mode,ch):
    im = Image.new(mode,(w,h))
    px = []
    for y in range(h):
        for x in range(w):
            v = tuple(((x*7+y*3+c*50)&255) ^ (random.randrange(8)) for c in range(ch))
            px.append(v if ch>1 else v[0])
    im.putdata(px)
    return im
def grad(w,h,mode,ch):
    im = Image.new(mode,(w,h))
    im.putdata([tuple((x*255//max(1,w-1) + c*40 + y)&255 for c in range(ch)) if ch>1 else (x*255//max(1,w-1)+y)&255 for y in range(h) for x in range(w)])
    return im
cases = [
 ("rgb16x16", noise(16,16,"RGB",3), {}),
 ("rgb37x23", grad(37,23,"RGB",3), {}),
 ("grey20x20", grad(20,20,"L",1), {}),
 ("rgba17x9", noise(17,9,"RGBA",4), {}),
 ("rgb300x200", grad(300,200,"RGB",3), {}),
 ("rgb64x64_e1", noise(64,64,"RGB",3), {"effort":1}),
 ("rgb64x64_e9", noise(64,64,"RGB",3), {"effort":9}),
 ("rgb40x40_e3", grad(40,40,"RGB",3), {"effort":3}),
 ("rgb40x40_e5", grad(40,40,"RGB",3), {"effort":5}),
 ("greya24x24", grad(24,24,"LA",2), {}),
 ("rgb1x1", Image.new("RGB",(1,1),(10,200,30)), {}),
 ("rgb300x20", noise(300,20,"RGB",3), {}),
 ("rgb513x7", grad(513,7,"RGB",3), {}),
]
for name, im, kw in cases:
    b = io.BytesIO()
    im.save(b, "JXL", lossless=True, **kw)
    open(f"{out}/{name}.jxl","wb").write(b.getvalue())
    im.save(f"{out}/{name}.png")
    print(name, len(b.getvalue()), b.getvalue()[:2].hex())

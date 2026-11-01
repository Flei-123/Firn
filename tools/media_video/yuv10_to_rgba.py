import sys
yuv = open(sys.argv[1], 'rb').read()
w, h = int(sys.argv[3]), int(sys.argv[4])
cw, ch = (w + 1) // 2, (h + 1) // 2
def s(i):
    return yuv[i * 2] | yuv[i * 2 + 1] << 8
out = bytearray()
frame = (w * h + 2 * cw * ch) * 2
for f in range(len(yuv) // frame):
    base = f * frame // 2
    for y in range(h):
        for x in range(w):
            yy = (s(base + y * w + x) - 64) * 298
            ci = (y // 2) * cw + x // 2
            u = s(base + w * h + ci) - 512
            v = s(base + w * h + cw * ch + ci) - 512
            for t in ((yy + v * 409 + 512) >> 10, (yy - u * 100 - v * 208 + 512) >> 10, (yy + u * 516 + 512) >> 10):
                out.append(min(255, max(0, t)))
            out.append(255)
open(sys.argv[2], 'wb').write(out)

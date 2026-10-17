# Same task without Pillow: PNG in -> 50 % bilinear (tent) -> PNG out. No JPEG
# (stdlib has no JPEG decoder) and no WebP (no encoder either).
import struct
import zlib


def read_png(path):
    data = open(path, "rb").read()
    pos, idat = 8, b""
    while pos < len(data):
        n, tag = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if tag == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
        elif tag == b"IDAT":
            idat += body
        pos += 12 + n
    assert depth == 8 and ctype in (2, 6)
    bpp = 3 if ctype == 2 else 4
    raw, rows, prev = zlib.decompress(idat), [], bytearray(w * bpp)
    for y in range(h):
        f = raw[y * (w * bpp + 1)]
        cur = bytearray(raw[y * (w * bpp + 1) + 1:(y + 1) * (w * bpp + 1)])
        for i in range(len(cur)):
            a = cur[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                cur[i] = (cur[i] + a) & 255
            elif f == 2:
                cur[i] = (cur[i] + b) & 255
            elif f == 3:
                cur[i] = (cur[i] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                cur[i] = (cur[i] + pr) & 255
        rows.append(cur)
        prev = cur
    return w, h, bpp, rows


def tent_resize(w, h, bpp, rows, dw, dh):
    sx, sy = w / dw, h / dh
    out = []
    for y in range(dh):
        cy = (y + 0.5) * sy
        ys = [(j, 1 - abs((j + 0.5 - cy) / sy)) for j in range(max(0, int(cy - sy)), min(h, int(cy + sy) + 1))]
        ys = [(j, wt) for j, wt in ys if wt > 0]
        row = bytearray()
        for x in range(dw):
            cx = (x + 0.5) * sx
            xs = [(i, 1 - abs((i + 0.5 - cx) / sx)) for i in range(max(0, int(cx - sx)), min(w, int(cx + sx) + 1))]
            xs = [(i, wt) for i, wt in xs if wt > 0]
            tot = sum(a * b for _, a in ys for _, b in xs)
            for ch in range(bpp):
                s = sum(rows[j][i * bpp + ch] * a * b for j, a in ys for i, b in xs)
                row.append(min(255, int(s / tot + 0.5)))
        out.append(row)
    return out


def write_png(path, w, h, bpp, rows):
    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))
    raw = b"".join(b"\0" + bytes(r) for r in rows)
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2 if bpp == 3 else 6, 0, 0, 0)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


w, h, bpp, rows = read_png("in.png")
small = tent_resize(w, h, bpp, rows, w // 2, h // 2)
write_png("out_stdlib.png", w // 2, h // 2, bpp, small)
w2, h2, _, _ = read_png("out_stdlib.png")
print(f"in {w}x{h}")
print(f"out {w2}x{h2}")

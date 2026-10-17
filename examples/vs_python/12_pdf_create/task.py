# 2-page PDF with title, wrapped paragraph, table and embedded JPEG; stdlib only.
# (With reportlab this is ~15 lines: SimpleDocTemplate + Paragraph + Table + Image.)
import struct
import textwrap

PARA = ("Firn compiles to small native programs and ships its own libraries for "
        "images, documents, networking and user interfaces. This report was written "
        "by one short program without any third party package.")
ROWS = [("Fruit", "Weight", "Price"), ("Apple", "120", "0.40"), ("Banana", "150", "0.25"), ("Cherry", "8", "0.10")]
W, H = 595, 842  # A4 in points


def jpeg_size(data):
    i = 2
    while i < len(data):
        marker, n = data[i + 1], struct.unpack(">H", data[i + 2:i + 4])[0]
        if marker in (0xC0, 0xC2):
            h, w = struct.unpack(">HH", data[i + 5:i + 9])
            return w, h
        i += 2 + n


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def text(x, y, size, s, font="F1"):
    return f"BT /{font} {size} Tf {x} {H - y} Td ({esc(s)}) Tj ET\n"


def page1():
    c = text(50, 70, 22, "Quarterly Report", "F2")
    for i, line in enumerate(textwrap.wrap(PARA, 75)):
        c += text(50, 110 + i * 16, 11, line)
    for r, row in enumerate(ROWS):
        for k, cell in enumerate(row):
            x, y = 50 + k * 120, 230 + r * 24
            c += f"{x} {H - y - 6} 120 24 re S\n" + text(x + 6, y + 10, 11, cell, "F2" if r == 0 else "F1")
    c += f"q 240 0 0 180 50 {H - 330 - 180} cm /Im1 Do Q\n"
    return c


def page2():
    return text(50, 70, 16, "Page 2", "F2") + text(50, 100, 11, "Second page: the data is in the table on page 1.")


photo = open("photo.jpg", "rb").read()
iw, ih = jpeg_size(photo)
objs = [b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>"]
res = b"/Resources << /Font << /F1 5 0 R /F2 6 0 R >> /XObject << /Im1 9 0 R >> >>"
for n, content in enumerate((page1(), page2())):
    objs.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Contents {7 + n} 0 R ".encode() + res + b" >>")
objs.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
objs.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
for content in (page1(), page2()):
    objs.append(f"<< /Length {len(content)} >>\nstream\n{content}endstream".encode())
objs.append((f"<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB "
             f"/BitsPerComponent 8 /Filter /DCTDecode /Length {len(photo)} >>\nstream\n").encode() + photo + b"\nendstream")
out, offsets = bytearray(b"%PDF-1.4\n"), []
for i, body in enumerate(objs, 1):
    offsets.append(len(out))
    out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
xref = len(out)
out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
out += b"".join(f"{o:010d} 00000 n \n".encode() for o in offsets)
out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
open("out.pdf", "wb").write(out)
print("pages 2")
print(f"image {iw}x{ih}")

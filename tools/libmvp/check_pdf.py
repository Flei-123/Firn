#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_pdf.py <pdf_probe> <workdir> <font.ttf>... -- lib/pdf
# held against poppler (pdfinfo, pdffonts, pdftotext, pdftoppm) and pypdf.
import io, os, struct, subprocess, sys
probe, work, fonts = sys.argv[1], sys.argv[2], sys.argv[3:]
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
def ok(m):
    print("  ok  ", m)
def run(*a):
    r = subprocess.run(a, capture_output=True, text=True)
    return r.returncode, r.stdout, r.stderr

for font in fonts:
    if not os.path.exists(font):
        print("  skip  (no %s)" % font)
        continue
    tag = os.path.basename(font)
    pdf = os.path.join(work, "probe_%s.pdf" % tag)
    rc, out, err = run(probe, pdf, font)
    if rc != 0:
        fail("%s: pdf_probe exit %d %s" % (tag, rc, err[-200:])); continue
    rc, out, err = run("pdfinfo", pdf)
    if rc == 0 and not err.strip() and "Pages:           2" in out and "1190.55 x 841.89" in out \
            and "OpenPlan – Wendeschützschaltung" in out and "PDF version:     1.7" in out:
        ok("%s: pdfinfo: 2 pages, A3, title, no complaints" % tag)
    else:
        fail("%s: pdfinfo %s %s" % (tag, out[:300], err[:300]))
    rc, out, err = run("pdffonts", pdf)
    rows = [l for l in out.splitlines()[2:] if l.strip()]
    if rc == 0 and len(rows) == 1 and "CID TrueType" in rows[0] and "Identity-H" in rows[0] \
            and rows[0].split()[-5:-2] == ["yes", "yes", "yes"] and "+" in rows[0].split()[0]:
        ok("%s: pdffonts: embedded, subset, ToUnicode" % tag)
    else:
        fail("%s: pdffonts %s" % (tag, out))
    rc, out, err = run("pdftotext", pdf, "-")
    want = ["Wendeschützschaltung -K1 -K2", "Größe: 3 × 400 V, Ω α β (Test)", "Seite 2: Klemmenplan X1"]
    got = [l for l in out.splitlines() if l.strip() and l != "\f"]
    if got == want and not err.strip():
        ok("%s: pdftotext gives back the text, umlauts and Greek included" % tag)
    else:
        fail("%s: pdftotext %s %s" % (tag, got, err[:200]))
    # the pixels
    rc, out, err = run("pdftoppm", "-r", "72", "-f", "1", "-l", "1", "-png", pdf, os.path.join(work, "r_" + tag))
    from PIL import Image
    im = Image.open(os.path.join(work, "r_%s-1.png" % tag)).convert("RGB")
    mm = 72 / 25.4
    def px(x, y): return im.getpixel((int(x * mm), int(y * mm)))
    checks = {"red square": (px(40, 40), (255, 0, 0)), "blue circle": (px(200, 150), (0, 0, 255)),
              "circle edge outside": (px(200, 118), (255, 255, 255)), "paper": (px(300, 120), (255, 255, 255)),
              "frame line": (px(10, 150), (0, 0, 0)), "image green cell (row 0)": (px(102, 22), (0, 200, 0)),
              "image transparent cell": (px(107, 22), (255, 255, 255))}
    wrong = {k: v for k, v in checks.items() if v[0] != v[1]}
    if not err.strip() and not wrong:
        ok("%s: pdftoppm: %d pixel checks (fill, circle, frame, image, alpha)" % (tag, len(checks)))
    else:
        fail("%s: pixels %s %s" % (tag, wrong, err[:200]))
    # structure through pypdf
    import pypdf
    r = pypdf.PdfReader(pdf, strict=True)
    ol = r.outline
    page_of = {p.indirect_reference.idnum: i for i, p in enumerate(r.pages)}
    annots = [a.get_object() for a in r.pages[0]["/Annots"]]
    internal = [a for a in annots if "/Dest" in a]
    uri = [a for a in annots if "/A" in a]
    oc = r.trailer["/Root"]["/OCProperties"]
    ocg_names = [g.get_object()["/Name"] for g in oc["/OCGs"]]
    content = r.pages[0].get_contents().get_data()
    conds = [
        (len(ol) == 2 and ol[0].title == "Übersicht" and isinstance(ol[1], list) and ol[1][0].title == "Seite 2",
         "bookmarks: 'Übersicht' with 'Seite 2' under it"),
        (r.get_destination_page_number(ol[0]) == 0 and r.get_destination_page_number(ol[1][0]) == 1,
         "bookmarks point to pages 1 and 2"),
        (len(internal) == 1 and page_of[internal[0]["/Dest"][0].idnum] == 1, "the title link jumps to page 2"),
        (len(uri) == 1 and uri[0]["/A"]["/URI"] == "https://example.org/openplan?a=(1)", "the URI link, parentheses escaped"),
        (ocg_names == ["Frame"] and b"/OC /OC0 BDC" in content, "layer 'Frame' declared and used"),
    ]
    for c, m in conds:
        (ok if c else fail)("%s: pypdf strict: %s" % (tag, m))
    # the embedded subset is a sound TrueType file
    f = r.pages[0]["/Resources"]["/Font"]["/F0"].get_object()
    data = f["/DescendantFonts"][0].get_object()["/FontDescriptor"]["/FontFile2"].get_object().get_data()
    padded = data + b"\0" * ((4 - len(data) % 4) % 4)
    total = sum(struct.unpack(">%dI" % (len(padded) // 4), padded)) & 0xFFFFFFFF
    try:
        from fontTools.ttLib import TTFont
        t = TTFont(io.BytesIO(data))
        t["glyf"]; t["hmtx"]
        used = sum(1 for n in t.getGlyphOrder() if t["glyf"][n].numberOfContours != 0)
        ft = "fontTools reads it, %d glyphs with outlines" % used
    except ImportError:
        ft = "fontTools not installed"
    if total == 0xB1B0AFBA:
        ok("%s: subset font %d octets (source %d), checksum 0xB1B0AFBA, %s" % (tag, len(data), os.path.getsize(font), ft))
    else:
        fail("%s: subset checksum %08x" % (tag, total))
    # determinism
    pdf2 = pdf + ".again"
    run(probe, pdf2, font)
    if open(pdf, "rb").read() == open(pdf2, "rb").read():
        ok("%s: the same calls give the same octets" % tag)
    else:
        fail("%s: output differs between two runs" % tag)
print("pdf: %d failed" % bad)
sys.exit(1 if bad else 0)

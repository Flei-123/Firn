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
    # PDF/A-2b: the same page, plus metadata and an output intent, held against veraPDF
    pa = os.path.join(work, "probe_a_%s.pdf" % tag)
    rc, out, err = run(probe, pa, font, "pdfa")
    if rc != 0:
        fail("%s: pdf_probe pdfa exit %d" % (tag, rc))
        continue
    rc, out, err = run("pdftotext", pa, "-")
    got = [l for l in out.splitlines() if l.strip() and l != "\f"]
    if got == want and not err.strip():
        ok("%s: PDF/A file: same text" % tag)
    else:
        fail("%s: PDF/A file text %s %s" % (tag, got, err[:200]))
    raw = open(pa, "rb").read()
    import re, xml.dom.minidom
    m = re.search(rb"<\?xpacket begin=.*?<\?xpacket end=\"w\"\?>", raw, re.S)
    try:
        dom = xml.dom.minidom.parseString(m.group(0).split(b"?>", 1)[1].rsplit(b"<?xpacket", 1)[0])
        txt = dom.documentElement.toxml()
        good = all(t in txt for t in ["<pdfaid:part>2</pdfaid:part>", "<pdfaid:conformance>B</pdfaid:conformance>",
            "OpenPlan \u2013 Wendesch\u00fctzschaltung", "pdf_probe &amp; &lt;Firn&gt;", "2026-09-"])
        if good and raw.count(b"/GTS_PDFA1") == 1 and raw.count(b"/Subtype /XML") == 1:
            ok("%s: XMP packet is well-formed XML with the claim, title, escaped tool and date; one intent" % tag)
        else:
            fail("%s: XMP content: %s" % (tag, txt[:300]))
    except Exception as e:
        fail("%s: XMP packet: %r" % (tag, e))
    # the embedded profile: a valid ICC file that lcms turns into exactly the sRGB of lcms
    try:
        import zlib
        from PIL import ImageCms, Image
        mm = re.search(rb"/N 3 /Length (\d+)( /Filter /FlateDecode)? >>\nstream\n", raw)
        icc = raw[mm.end():mm.end() + int(mm.group(1))]
        if mm.group(2):
            icc = zlib.decompress(icc)
        prof = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        tr = ImageCms.buildTransform(prof, ImageCms.createProfile("sRGB"), "RGB", "RGB")
        px = [(r, g, b) for r in (0, 7, 128, 255) for g in (0, 64, 200) for b in (0, 3, 255)]
        im = Image.new("RGB", (len(px), 1))
        im.putdata(px)
        got = list(ImageCms.applyTransform(im, tr).get_flattened_data() if hasattr(Image.Image, "get_flattened_data") else ImageCms.applyTransform(im, tr).getdata())
        if len(icc) == struct.unpack(">I", icc[:4])[0] == 440 and got == px:
            ok("%s: embedded sRGB profile: 440 octets, lcms maps %d colours to themselves" % (tag, len(px)))
        else:
            fail("%s: embedded profile: size %d, colours %s" % (tag, len(icc), got[:4]))
    except Exception as e:
        fail("%s: embedded profile: %r" % (tag, e))
    if b"/F 4" in raw and b"/Name (Default)" in raw:
        ok("%s: link flags and optional-content configuration name" % tag)
    else:
        fail("%s: PDF/A: no /F 4 or /Name" % tag)
    if b"GTS_PDFA1" in open(pdf, "rb").read():
        fail("%s: a plain file claims PDF/A" % tag)
    vera = os.environ.get("VERAPDF") or "/opt/verapdf/verapdf"
    if os.path.exists(vera):
        rc, out, err = run(vera, "--flavour", "2b", "--format", "text", pa)
        if rc == 0 and out.startswith("PASS"):
            ok("%s: veraPDF: PDF/A-2b compliant" % tag)
        else:
            fail("%s: veraPDF: %s" % (tag, out[:300]))
        rc, out, err = run(vera, "--flavour", "2b", "--format", "text", pdf)
        if out.startswith("FAIL"):
            ok("%s: veraPDF: the plain file is not PDF/A (the check can fail)" % tag)
        else:
            fail("%s: veraPDF accepts the plain file: %s" % (tag, out[:200]))
    else:
        print("  skip  (no veraPDF: set VERAPDF)")
    pa2 = pa + ".again"
    run(probe, pa2, font, "pdfa")
    if open(pa, "rb").read() == open(pa2, "rb").read():
        ok("%s: PDF/A: the same calls give the same octets" % tag)
    else:
        fail("%s: PDF/A output differs between two runs" % tag)
print("pdf: %d failed" % bad)
sys.exit(1 if bad else 0)

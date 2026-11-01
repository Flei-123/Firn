#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import sys
import zipfile

out = sys.argv[1]

fonts = [("F1", "Helvetica"), ("F2", "Times-Roman"), ("F3", "Courier-Bold"), ("F4", "Helvetica-Oblique"), ("F5", "Times-BoldItalic")]
lines = ["BT"]
y = 360
for i, (key, _) in enumerate(fonts):
    lines.append("/%s 20 Tf 1 0 0 1 30 %d Tm (Lazy faces %d: The quick brown fox jumps over the lazy dog) Tj" % (key, y, i))
    y -= 40
lines.append("ET")
content = "\n".join(lines).encode()
objs = [
    b"<</Type/Catalog/Pages 2 0 R>>",
    b"<</Type/Pages/Kids[3 0 R]/Count 1>>",
    ("<</Type/Page/Parent 2 0 R/MediaBox[0 0 520 400]/Contents 4 0 R/Resources<</Font<<%s>>>>>>>>" %
     "".join("/%s %d 0 R" % (k, 5 + i) for i, (k, _) in enumerate(fonts))).encode(),
    b"<</Length %d>>\nstream\n" % len(content) + content + b"\nendstream",
] + [("<</Type/Font/Subtype/Type1/BaseFont/%s>>" % n).encode() for _, n in fonts]
pdf = bytearray(b"%PDF-1.4\n")
offs = []
for i, o in enumerate(objs):
    offs.append(len(pdf))
    pdf += b"%d 0 obj\n" % (i + 1) + o + b"\nendobj\n"
xref = len(pdf)
pdf += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
for o in offs:
    pdf += b"%010d 00000 n \n" % o
pdf += b"trailer<</Size %d/Root 1 0 R>>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
open(out + "/a.pdf", "wb").write(pdf)

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
body = "".join(
    '<w:p><w:r><w:rPr>%s</w:rPr><w:t xml:space="preserve">%s</w:t></w:r></w:p>' % (rpr, txt)
    for rpr, txt in [
        ("", "Lazy faces in a document. The quick brown fox jumps over the lazy dog."),
        ("<w:b/>", "Bold line with the same fox and the same dog."),
        ("<w:i/>", "Italic line: pack my box with five dozen liquor jugs."),
        ('<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/>', "Monospace line 0123456789"),
    ]
)
doc = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="%s"><w:body>%s<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>' % (W, body)
with zipfile.ZipFile(out + "/a.docx", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
    z.writestr("_rels/.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
    z.writestr("word/document.xml", doc)

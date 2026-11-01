#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import struct
import sys
import zipfile
import zlib

NS = ('xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
      'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"')
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"


def rels(items):
    body = "".join('<Relationship Id="%s" Type="%s%s" Target="%s"/>' % (i, REL, t, g) for i, t, g in items)
    return '<?xml version="1.0"?><Relationships xmlns="%s">%s</Relationships>' % (PKG, body)


def png(w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def png_split(w, h, left, right):
    row = b"\x00" + bytes(left) * (w // 2) + bytes(right) * (w - w // 2)
    raw = row * h

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def shape_geo(idx, name, prst, x, y, w, h, fill, extra=""):
    return ('<p:sp><p:nvSpPr><p:cNvPr id="%d" name="%s"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>'
            '<a:xfrm><a:off x="%d" y="%d"/><a:ext cx="%d" cy="%d"/></a:xfrm><a:prstGeom prst="%s">%s</a:prstGeom>%s</p:spPr></p:sp>') % (
        idx, name, x, y, w, h, prst, extra, fill)


def theme():
    cols = ["dk1:000000", "lt1:FFFFFF", "dk2:44546A", "lt2:E7E6E6", "accent1:4472C4", "accent2:ED7D31", "accent3:A5A5A5",
            "accent4:FFC000", "accent5:5B9BD5", "accent6:70AD47", "hlink:0563C1", "folHlink:954F72"]
    body = "".join('<a:%s><a:srgbClr val="%s"/></a:%s>' % (c.split(":")[0], c.split(":")[1], c.split(":")[0]) for c in cols)
    return '<?xml version="1.0"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="T"><a:themeElements><a:clrScheme name="C">%s</a:clrScheme></a:themeElements></a:theme>' % body


def sp(idx, name, x, y, w, h, body, fill="", ph=""):
    return ('<p:sp><p:nvSpPr><p:cNvPr id="%d" name="%s"/><p:cNvSpPr/><p:nvPr>%s</p:nvPr></p:nvSpPr>'
            '<p:spPr>%s%s</p:spPr>%s</p:sp>') % (
        idx, name, ph, '<a:xfrm><a:off x="%d" y="%d"/><a:ext cx="%d" cy="%d"/></a:xfrm>' % (x, y, w, h) if w else "",
        fill, body)


def tx(paras, extra=""):
    return '<p:txBody><a:bodyPr%s/><a:lstStyle/>%s</p:txBody>' % (extra, paras)


def para(text, ppr="", rpr=""):
    return '<a:p>%s<a:r>%s<a:t>%s</a:t></a:r></a:p>' % (ppr, rpr, text)


def slide(shapes):
    return '<?xml version="1.0"?><p:sld %s><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>%s</p:spTree></p:cSld></p:sld>' % (NS, shapes)


def build(path):
    master_tree = (sp(2, "Title", 457200, 274638, 8229600, 1143000, tx('<a:p><a:endParaRPr lang="en-US"/></a:p>'), ph='<p:ph type="title"/>')
                   + sp(3, "Body", 457200, 1600200, 8229600, 4525963, tx('<a:p><a:endParaRPr lang="en-US"/></a:p>'), ph='<p:ph type="body" idx="1"/>')
                   + sp(4, "Bar", 0, 6400800, 9144000, 457200, "", fill='<a:solidFill><a:srgbClr val="1F4E79"/></a:solidFill>'))
    master = ('<?xml version="1.0"?><p:sldMaster %s><p:cSld><p:bg><p:bgPr><a:solidFill><a:schemeClr val="bg1"/></a:solidFill></p:bgPr></p:bg>'
              '<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>%s</p:spTree></p:cSld>'
              '<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>'
              '<p:txStyles><p:titleStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="4400" b="1"><a:solidFill><a:schemeClr val="tx2"/></a:solidFill></a:defRPr></a:lvl1pPr></p:titleStyle>'
              '<p:bodyStyle><a:lvl1pPr marL="342900" indent="-342900"><a:buChar char="&#8226;"/><a:defRPr sz="2800"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:defRPr></a:lvl1pPr>'
              '<a:lvl2pPr marL="742950" indent="-285750"><a:buChar char="-"/><a:defRPr sz="2400"/></a:lvl2pPr></p:bodyStyle>'
              '<p:otherStyle><a:lvl1pPr><a:defRPr sz="1800"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:defRPr></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>') % (NS, master_tree)
    layout = ('<?xml version="1.0"?><p:sldLayout %s><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
              '%s%s</p:spTree></p:cSld></p:sldLayout>') % (
        NS, sp(2, "T", 0, 0, 0, 0, "", ph='<p:ph type="title"/>'), sp(3, "B", 0, 0, 0, 0, "", ph='<p:ph idx="1"/>'))
    s1 = slide(
        sp(2, "Title 1", 0, 0, 0, 0, tx(para("Hello Firn")), ph='<p:ph type="title"/>')
        + sp(3, "Content", 0, 0, 0, 0, tx(para("First point") + para("Second sub point", '<a:pPr lvl="1"/>') + para("Third point")), ph='<p:ph idx="1"/>')
        + sp(4, "Box", 6858000, 4800000, 1800000, 900000,
             tx(para("Centered", '<a:pPr algn="ctr"/>', '<a:rPr lang="en-US" sz="2000" b="1"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr>'), ' anchor="ctr"'),
             fill='<a:solidFill><a:schemeClr val="accent2"/></a:solidFill><a:ln w="19050"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>'))
    tbl = ('<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Tbl"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>'
           '<p:xfrm><a:off x="457200" y="1600200"/><a:ext cx="6000000" cy="1200000"/></p:xfrm><a:graphic><a:graphicData uri="x"><a:tbl>'
           '<a:tblPr firstRow="1" bandRow="1"><a:tableStyleId>{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}</a:tableStyleId></a:tblPr>'
           '<a:tblGrid><a:gridCol w="3000000"/><a:gridCol w="3000000"/></a:tblGrid>'
           '<a:tr h="400000"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Head A</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>'
           '<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Head B</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr>'
           '<a:tr h="400000"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Cell 1</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>'
           '<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Cell 2</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr>'
           '</a:tbl></a:graphicData></a:graphic></p:graphicFrame>')
    pic = ('<p:pic><p:nvPicPr><p:cNvPr id="6" name="Pic"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/></p:blipFill>'
           '<p:spPr><a:xfrm><a:off x="457200" y="3200000"/><a:ext cx="1200000" cy="900000"/></a:xfrm></p:spPr></p:pic>')
    grp = ('<p:grpSp><p:nvGrpSpPr><p:cNvPr id="7" name="G"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="3000000" y="3200000"/><a:ext cx="2000000" cy="1000000"/>'
           '<a:chOff x="0" y="0"/><a:chExt cx="1000" cy="500"/></a:xfrm></p:grpSpPr>%s</p:grpSp>') % sp(
        8, "In", 0, 0, 500, 250, tx(para("Grouped", '', '<a:rPr lang="en-US" sz="1400"/>')), fill='<a:solidFill><a:srgbClr val="FFC000"/></a:solidFill>')
    pic2 = ('<p:pic><p:nvPicPr><p:cNvPr id="9" name="Crop"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/><a:srcRect l="50000"/></p:blipFill>'
            '<p:spPr><a:xfrm><a:off x="5300000" y="3200000"/><a:ext cx="1200000" cy="900000"/></a:xfrm></p:spPr></p:pic>')
    ell = shape_geo(10, "Ell", "ellipse", 6500000, 1600200, 2000000, 1000000,
                    '<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="0"/></a:gradFill>')
    rr = shape_geo(11, "Round", "roundRect", 6500000, 2900000, 2000000, 900000,
                   '<a:solidFill><a:srgbClr val="00AA00"/></a:solidFill><a:ln w="38100"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>',
                   '<a:avLst><a:gd name="adj" fmla="val 50000"/></a:avLst>')
    s2 = slide(sp(2, "Title 2", 0, 0, 0, 0, tx(para("Second slide")), ph='<p:ph type="title"/>') + tbl + pic + pic2 + ell + rr + grp)
    pres = ('<?xml version="1.0"?><p:presentation %s><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>'
            '<p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/>'
            '<p:defaultTextStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr></p:defaultTextStyle></p:presentation>') % NS
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>')
        z.writestr("_rels/.rels", rels([("rId1", "officeDocument", "ppt/presentation.xml")]))
        z.writestr("ppt/presentation.xml", pres)
        z.writestr("ppt/_rels/presentation.xml.rels", rels([("rId1", "slideMaster", "slideMasters/slideMaster1.xml"), ("rId2", "slide", "slides/slide1.xml"), ("rId3", "slide", "slides/slide2.xml")]))
        z.writestr("ppt/slideMasters/slideMaster1.xml", master)
        z.writestr("ppt/slideMasters/_rels/slideMaster1.xml.rels", rels([("rId1", "theme", "../theme/theme1.xml")]))
        z.writestr("ppt/slideLayouts/slideLayout1.xml", layout)
        z.writestr("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rels([("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]))
        z.writestr("ppt/theme/theme1.xml", theme())
        z.writestr("ppt/slides/slide1.xml", s1)
        z.writestr("ppt/slides/_rels/slide1.xml.rels", rels([("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")]))
        z.writestr("ppt/slides/slide2.xml", s2)
        z.writestr("ppt/slides/_rels/slide2.xml.rels", rels([("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId2", "image", "../media/image1.png"), ("rId3", "image", "../media/image2.png")]))
        z.writestr("ppt/media/image1.png", png(8, 6, (200, 30, 30)))
        z.writestr("ppt/media/image2.png", png_split(8, 6, (10, 20, 230), (20, 200, 20)))


if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    build(os.path.join(out, "basic.pptx"))

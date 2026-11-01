# SPDX-License-Identifier: MPL-2.0
import docx
from docx.enum.section import WD_SECTION
from docx.oxml import parse_xml, OxmlElement
from docx.oxml.ns import qn
from docx.shared import Cm

LOREM = ("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore "
         "et dolore magna aliqua.")

NS = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
      'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
      'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" '
      'xmlns:v="urn:schemas-microsoft-com:vml"')


def textbox_run(x_emu, y_emu, w_emu, h_emu, lines, rel="page", behind="0"):
    paras = "".join("<w:p><w:r><w:t>%s</w:t></w:r></w:p>" % t for t in lines)
    return parse_xml(
        '<w:r %s><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>'
        '<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="%s" '
        'locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
        '<wp:positionH relativeFrom="%s"><wp:posOffset>%d</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="%s"><wp:posOffset>%d</wp:posOffset></wp:positionV>'
        '<wp:extent cx="%d" cy="%d"/><wp:wrapNone/><wp:docPr id="1" name="Text Box"/>'
        '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
        '<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="%d" cy="%d"/></a:xfrm>'
        '<a:prstGeom prst="rect"/><a:solidFill><a:srgbClr val="FFFF99"/></a:solidFill></wps:spPr>'
        '<wps:txbx><w:txbxContent>%s</w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>'
        '</a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice></mc:AlternateContent></w:r>'
        % (NS, behind, rel, x_emu, rel, y_emu, w_emu, h_emu, w_emu, h_emu, paras))


def f17_textbox(path):
    d = docx.Document()
    d.add_paragraph("Before the box.")
    p = d.add_paragraph("Anchor paragraph with floating box.")
    p._p.append(textbox_run(3600000, 900000, 2200000, 900000, ["Box line alpha", "Box line beta"]))
    d.add_paragraph("After the box. " + LOREM)
    d.save(path)


def f18_vml_textbox(path):
    d = docx.Document()
    p = d.add_paragraph("Host paragraph for vml box.")
    p._p.append(parse_xml(
        '<w:r %s><w:pict><v:shape id="s1" type="#_x0000_t202" '
        'style="position:absolute;margin-left:200pt;margin-top:30pt;width:150pt;height:60pt;'
        'mso-position-horizontal-relative:page;mso-position-vertical-relative:page">'
        '<v:textbox><w:txbxContent><w:p><w:r><w:t>Vml box words</w:t></w:r></w:p></w:txbxContent></v:textbox>'
        '</v:shape></w:pict></w:r>' % NS))
    d.add_paragraph("Tail paragraph.")
    d.save(path)


def f19_anchor_image(path):
    import os, tempfile
    from docx_fixtures import png_bytes
    d = docx.Document()
    p = d.add_paragraph("Paragraph with anchored image to the right. " + LOREM)
    fp = os.path.join(tempfile.mkdtemp(), "a.png")
    with open(fp, "wb") as f:
        f.write(png_bytes(40, 40, (0, 0, 200)))
    p.add_run().add_picture(fp, width=Cm(3))
    inline = p._p.xpath(".//wp:inline")[0]
    anchor = parse_xml(
        '<wp:anchor %s distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" '
        'locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
        '<wp:positionH relativeFrom="margin"><wp:align>right</wp:align></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>100000</wp:posOffset></wp:positionV></wp:anchor>' % NS)
    for tag in ("wp:extent",):
        anchor.append(inline.find(qn(tag)))
    anchor.append(parse_xml('<wp:wrapNone %s/>' % NS))
    for tag in ("wp:docPr", "wp:cNvGraphicFramePr", "a:graphic"):
        anchor.append(inline.find(qn(tag)))
    inline.getparent().replace(inline, anchor)
    d.add_paragraph("Second paragraph.")
    d.save(path)


def f20_vmerge_rows(path):
    d = docx.Document()
    t = d.add_table(rows=5, cols=3)
    t.style = "Table Grid"
    big = t.cell(1, 0).merge(t.cell(3, 0))
    big.text = "merged tall " + " ".join(["word%d" % i for i in range(60)])
    side = t.cell(2, 2).merge(t.cell(4, 2))
    side.text = "side merge"
    for r in range(5):
        t.cell(r, 1).text = "mid%d" % r
    t.cell(0, 0).text = "top"
    t.cell(0, 2).text = "topr"
    t.cell(4, 0).text = "bottom"
    d.save(path)


def f21_tall_row(path):
    d = docx.Document()
    d.add_paragraph("Intro line.")
    t = d.add_table(rows=2, cols=2)
    t.style = "Table Grid"
    c = t.cell(0, 0)
    c.text = "tall cell start"
    for i in range(150):
        c.add_paragraph("tall row line %d %s" % (i, LOREM[:40]))
    t.cell(0, 1).text = "right side"
    t.cell(1, 0).text = "after tall"
    d.add_paragraph("End.")
    d.save(path)


def f22_ctx_spacing(path):
    d = docx.Document()
    for i in range(6):
        p = d.add_paragraph("tight line %d" % i)
        p.paragraph_format.space_after = Cm(1)
        ppr = p._p.get_or_add_pPr()
        ppr.append(OxmlElement("w:contextualSpacing"))
    d.add_paragraph("loose line")
    d.save(path)


def f23_pgnum_restart(path):
    d = docx.Document()
    s0 = d.sections[0]
    s0.footer.paragraphs[0].text = "Foot"
    d.add_paragraph("First section text.")
    s1 = d.add_section(WD_SECTION.NEW_PAGE)
    s1.footer.is_linked_to_previous = False
    fp = s1.footer.paragraphs[0]
    fp.add_run("Num ")
    run = fp.add_run()
    for kind, text in (("begin", None), (None, " PAGE "), ("end", None)):
        if kind:
            el = OxmlElement("w:fldChar")
            el.set(qn("w:fldCharType"), kind)
        else:
            el = OxmlElement("w:instrText")
            el.set(qn("xml:space"), "preserve")
            el.text = text
        run._r.append(el)
    pg = OxmlElement("w:pgNumType")
    pg.set(qn("w:start"), "7")
    s1._sectPr.append(pg)
    d.add_paragraph("Second section restarts at seven.")
    d.save(path)


FIXTURES_EXT = [f17_textbox, f18_vml_textbox, f19_anchor_image, f20_vmerge_rows, f21_tall_row, f22_ctx_spacing,
                f23_pgnum_restart]

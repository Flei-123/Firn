import sys, zipfile, os

NS = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
      'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
      'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
      'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
      'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
      'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" '
      'xmlns:xlink="http://www.w3.org/1999/xlink" '
      'xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"')

STYLES = '''<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles %s><office:styles>
<style:default-style style:family="paragraph"><style:text-properties fo:font-size="12pt"/></style:default-style>
<style:style style:name="Standard" style:family="paragraph"/>
<style:style style:name="Heading_20_1" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-top="0.4cm" fo:margin-bottom="0.2cm" fo:keep-with-next="always"/><style:text-properties fo:font-size="180%%" fo:font-weight="bold"/></style:style>
<style:style style:name="Table_20_Contents" style:family="paragraph" style:parent-style-name="Standard"/>
<style:style style:name="Strong" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
<text:list-style style:name="L1"><text:list-level-style-bullet text:level="1" text:bullet-char="&#8226;"><style:list-level-properties text:list-level-position-and-space-mode="label-alignment"><style:list-level-label-alignment text:label-followed-by="listtab" fo:text-indent="-0.5cm" fo:margin-left="1cm"/></style:list-level-properties></text:list-level-style-bullet></text:list-style>
<text:list-style style:name="L2"><text:list-level-style-number text:level="1" style:num-suffix="." style:num-format="1"><style:list-level-properties text:list-level-position-and-space-mode="label-alignment"><style:list-level-label-alignment fo:text-indent="-0.6cm" fo:margin-left="1.2cm"/></style:list-level-properties></text:list-level-style-number>
<text:list-level-style-number text:level="2" style:num-suffix=")" style:num-format="a"><style:list-level-properties text:list-level-position-and-space-mode="label-alignment"><style:list-level-label-alignment fo:text-indent="-0.6cm" fo:margin-left="2.4cm"/></style:list-level-properties></text:list-level-style-number></text:list-style>
</office:styles>
<office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="21cm" fo:page-height="29.7cm" fo:margin-top="2cm" fo:margin-bottom="2cm" fo:margin-left="2cm" fo:margin-right="2cm"/></style:page-layout></office:automatic-styles>
<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Header text</text:p></style:header><style:footer><text:p>Page <text:page-number/> of <text:page-count/></text:p></style:footer></style:master-page></office:master-styles>
</office:document-styles>''' % NS

def content(body, auto=''):
    return ('<?xml version="1.0" encoding="UTF-8"?><office:document-content %s><office:automatic-styles>%s</office:automatic-styles><office:body><office:text>%s</office:text></office:body></office:document-content>' % (NS, auto, body))

def write(path, body, auto='', extra=None):
    with zipfile.ZipFile(path, 'w') as z:
        z.writestr(zipfile.ZipInfo('mimetype'), 'application/vnd.oasis.opendocument.text')
        z.writestr('META-INF/manifest.xml', '<?xml version="1.0"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"/>')
        z.writestr('styles.xml', STYLES)
        z.writestr('content.xml', content(body, auto))
        for k, v in (extra or {}).items():
            z.writestr(k, v)

def png():
    import struct, zlib
    w, h = 8, 8
    raw = b''.join(b'\x00' + b'\xff\x00\x00' * w for _ in range(h))
    def ch(t, d):
        c = struct.pack('>I', len(d)) + t + d
        return c + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + ch(b'IDAT', zlib.compress(raw)) + ch(b'IEND', b'')

def main(out):
    os.makedirs(out, exist_ok=True)
    write(out + '/plain.odt', '<text:p>Hello ODT</text:p><text:p>second   paragraph with <text:span text:style-name="Strong">bold</text:span> and<text:s text:c="2"/>spaces</text:p>')
    write(out + '/heading.odt', '<text:h text:outline-level="1" text:style-name="Heading_20_1">Title</text:h><text:p>Body text under the title.</text:p>')
    write(out + '/lists.odt', '<text:list text:style-name="L1"><text:list-item><text:p>one</text:p></text:list-item><text:list-item><text:p>two</text:p></text:list-item></text:list><text:list text:style-name="L2"><text:list-item><text:p>first</text:p><text:list><text:list-item><text:p>sub a</text:p></text:list-item><text:list-item><text:p>sub b</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>second</text:p></text:list-item></text:list>')
    tauto = ('<style:style style:name="Tab1" style:family="table"><style:table-properties style:width="12cm" table:align="left"/></style:style>'
             '<style:style style:name="Tab1.A" style:family="table-column"><style:table-column-properties style:column-width="4cm"/></style:style>'
             '<style:style style:name="Tab1.C" style:family="table-cell"><style:table-cell-properties fo:background-color="#ffff99" fo:border="0.5pt solid #000000"/></style:style>')
    cell = lambda t, a='': '<table:table-cell table:style-name="Tab1.C" %s><text:p text:style-name="Table_20_Contents">%s</text:p></table:table-cell>' % (a, t)
    write(out + '/table.odt', '<table:table table:name="T" table:style-name="Tab1"><table:table-column table:style-name="Tab1.A" table:number-columns-repeated="3"/>'
          '<table:table-row>' + cell('a1') + cell('b1', 'table:number-columns-spanned="2"') + '<table:covered-table-cell/></table:table-row>'
          '<table:table-row>' + cell('a2', 'table:number-rows-spanned="2"') + cell('b2') + cell('c2') + '</table:table-row>'
          '<table:table-row><table:covered-table-cell/>' + cell('b3') + cell('c3') + '</table:table-row></table:table>', tauto)
    write(out + '/image.odt', '<text:p>Before <draw:frame svg:width="2cm" svg:height="2cm" text:anchor-type="as-char"><draw:image xlink:href="Pictures/a.png"/></draw:frame> after</text:p>', extra={'Pictures/a.png': png()})
    write(out + '/textbox.odt', '<text:p>Body line one</text:p><text:p>Anchor <draw:frame svg:x="9cm" svg:y="1cm" svg:width="5cm" svg:height="2cm" text:anchor-type="paragraph"><draw:text-box><text:p>Boxed text</text:p><text:p>Second box line</text:p></draw:text-box></draw:frame>paragraph tail</text:p><text:p>Body line three</text:p>')
    longp = ''.join('<text:p>Paragraph number %d with some filler words to make the line wrap around the page width a few times for pagination.</text:p>' % i for i in range(120))
    write(out + '/long.odt', longp)

if __name__ == '__main__':
    main(sys.argv[1])

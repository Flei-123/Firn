#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Produces lib/browser/foreign_data.fi -- the correction tables of foreign content.

A WORKBENCH, NOT A PRODUCT: there is no parser logic here. The three tables
of WHATWG 13.2.6.5 ("Creating and inserting nodes", the adjustment steps)
stand here as plain word lists, and the script turns them into Firn string
literals whose length is written out -- a string literal has the type
`[u8; N]` in Firn (SPEC 8), so the length has to be a compile time constant.

  * SVG element names        "altglyph" -> "altGlyph"        (37 pairs)
  * SVG attribute names      "attributename" -> "attributeName"  (58 pairs)
  * MathML attribute names   "definitionurl" -> "definitionURL"  (1 pair)
  * foreign attributes       "xlink:href" -> prefix "xlink", local "href",
                             namespace XLink                 (11 entries)

Usage:  python3 tools/domb1/gen_foreign.py
"""

import os

# WHATWG 13.2.6.5, the SVG element name table.
SVG_TAGS = [
    ("altglyph", "altGlyph"),
    ("altglyphdef", "altGlyphDef"),
    ("altglyphitem", "altGlyphItem"),
    ("animatecolor", "animateColor"),
    ("animatemotion", "animateMotion"),
    ("animatetransform", "animateTransform"),
    ("clippath", "clipPath"),
    ("feblend", "feBlend"),
    ("fecolormatrix", "feColorMatrix"),
    ("fecomponenttransfer", "feComponentTransfer"),
    ("fecomposite", "feComposite"),
    ("feconvolvematrix", "feConvolveMatrix"),
    ("fediffuselighting", "feDiffuseLighting"),
    ("fedisplacementmap", "feDisplacementMap"),
    ("fedistantlight", "feDistantLight"),
    ("fedropshadow", "feDropShadow"),
    ("feflood", "feFlood"),
    ("fefunca", "feFuncA"),
    ("fefuncb", "feFuncB"),
    ("fefuncg", "feFuncG"),
    ("fefuncr", "feFuncR"),
    ("fegaussianblur", "feGaussianBlur"),
    ("feimage", "feImage"),
    ("femerge", "feMerge"),
    ("femergenode", "feMergeNode"),
    ("femorphology", "feMorphology"),
    ("feoffset", "feOffset"),
    ("fepointlight", "fePointLight"),
    ("fespecularlighting", "feSpecularLighting"),
    ("fespotlight", "feSpotLight"),
    ("fetile", "feTile"),
    ("feturbulence", "feTurbulence"),
    ("foreignobject", "foreignObject"),
    ("glyphref", "glyphRef"),
    ("lineargradient", "linearGradient"),
    ("radialgradient", "radialGradient"),
    ("textpath", "textPath"),
]

# WHATWG 13.2.6.5, "adjust SVG attributes".
SVG_ATTRS = [
    ("attributename", "attributeName"),
    ("attributetype", "attributeType"),
    ("basefrequency", "baseFrequency"),
    ("baseprofile", "baseProfile"),
    ("calcmode", "calcMode"),
    ("clippathunits", "clipPathUnits"),
    ("diffuseconstant", "diffuseConstant"),
    ("edgemode", "edgeMode"),
    ("filterunits", "filterUnits"),
    ("glyphref", "glyphRef"),
    ("gradienttransform", "gradientTransform"),
    ("gradientunits", "gradientUnits"),
    ("kernelmatrix", "kernelMatrix"),
    ("kernelunitlength", "kernelUnitLength"),
    ("keypoints", "keyPoints"),
    ("keysplines", "keySplines"),
    ("keytimes", "keyTimes"),
    ("lengthadjust", "lengthAdjust"),
    ("limitingconeangle", "limitingConeAngle"),
    ("markerheight", "markerHeight"),
    ("markerunits", "markerUnits"),
    ("markerwidth", "markerWidth"),
    ("maskcontentunits", "maskContentUnits"),
    ("maskunits", "maskUnits"),
    ("numoctaves", "numOctaves"),
    ("pathlength", "pathLength"),
    ("patterncontentunits", "patternContentUnits"),
    ("patterntransform", "patternTransform"),
    ("patternunits", "patternUnits"),
    ("pointsatx", "pointsAtX"),
    ("pointsaty", "pointsAtY"),
    ("pointsatz", "pointsAtZ"),
    ("preservealpha", "preserveAlpha"),
    ("preserveaspectratio", "preserveAspectRatio"),
    ("primitiveunits", "primitiveUnits"),
    ("refx", "refX"),
    ("refy", "refY"),
    ("repeatcount", "repeatCount"),
    ("repeatdur", "repeatDur"),
    ("requiredextensions", "requiredExtensions"),
    ("requiredfeatures", "requiredFeatures"),
    ("specularconstant", "specularConstant"),
    ("specularexponent", "specularExponent"),
    ("spreadmethod", "spreadMethod"),
    ("startoffset", "startOffset"),
    ("stddeviation", "stdDeviation"),
    ("stitchtiles", "stitchTiles"),
    ("surfacescale", "surfaceScale"),
    ("systemlanguage", "systemLanguage"),
    ("tablevalues", "tableValues"),
    ("targetx", "targetX"),
    ("targety", "targetY"),
    ("textlength", "textLength"),
    ("viewbox", "viewBox"),
    ("viewtarget", "viewTarget"),
    ("xchannelselector", "xChannelSelector"),
    ("ychannelselector", "yChannelSelector"),
    ("zoomandpan", "zoomAndPan"),
]

# WHATWG 13.2.6.5, "adjust MathML attributes".
MATH_ATTRS = [
    ("definitionurl", "definitionURL"),
]

# WHATWG 13.2.6.5, "adjust foreign attributes": name, prefix ("-" = none),
# local name, namespace (the NS_* number of lib/browser/node.fi).
NS_XLINK = 4
NS_XML = 5
NS_XMLNS = 6
FOREIGN_ATTRS = [
    ("xlink:actuate", "xlink", "actuate", NS_XLINK),
    ("xlink:arcrole", "xlink", "arcrole", NS_XLINK),
    ("xlink:href", "xlink", "href", NS_XLINK),
    ("xlink:role", "xlink", "role", NS_XLINK),
    ("xlink:show", "xlink", "show", NS_XLINK),
    ("xlink:title", "xlink", "title", NS_XLINK),
    ("xlink:type", "xlink", "type", NS_XLINK),
    ("xml:lang", "xml", "lang", NS_XML),
    ("xml:space", "xml", "space", NS_XML),
    ("xmlns", "-", "xmlns", NS_XMLNS),
    ("xmlns:xlink", "xmlns", "xlink", NS_XMLNS),
]

# Names the foreign rules need that do not stand in lib/browser/tag.fi.
EXTRA = ["sub", "sup", "var", "span", "encoding", "color", "face", "size",
         "definitionurl", "text/html", "application/xhtml+xml",
         "mglyph", "malignmark"]


def literal(name, words, comment):
    text = " ".join(words)
    raw = text.encode("utf-8")
    out = []
    out.append("// %s" % comment)
    out.append("#[no_gc]")
    out.append("fn %s(out: *mut mem.Buf) {" % name)
    out.append("    var t: [u8; %d] = \"%s\"" % (len(raw), text))
    out.append("    var i: usize = 0")
    out.append("    while i < %d {" % len(raw))
    out.append("        mem.buf_push(out, t[i])")
    out.append("        i = i + 1")
    out.append("    }")
    out.append("}")
    return "\n".join(out)


def main():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    target = os.path.join(root, "lib", "browser", "foreign_data.fi")

    head = '''// lib/browser/foreign_data.fi -- GENERATED by tools/domb1/gen_foreign.py.
// DO NOT EDIT BY HAND. The source of the order is the script.
//
// The four correction tables of WHATWG 13.2.6.5. They are word lists: two
// words per entry for the name corrections (lower case, correct case), four
// words per entry for the foreign attributes (name, prefix, local name,
// namespace number). `lib/browser/foreign.fi` splits them at the spaces and
// turns every word into an atom.

import html.mem

export {
    svg_tag_bytes, svg_attr_bytes, math_attr_bytes, foreign_attr_bytes,
    extra_bytes,
    SVG_TAG_COUNT, SVG_ATTR_COUNT, MATH_ATTR_COUNT, FOREIGN_ATTR_COUNT,
    EXTRA_COUNT,
}

const SVG_TAG_COUNT: usize = %d
const SVG_ATTR_COUNT: usize = %d
const MATH_ATTR_COUNT: usize = %d
const FOREIGN_ATTR_COUNT: usize = %d
const EXTRA_COUNT: usize = %d
''' % (len(SVG_TAGS), len(SVG_ATTRS), len(MATH_ATTRS), len(FOREIGN_ATTRS),
       len(EXTRA))

    parts = [head.rstrip('\n')]
    parts.append(literal(
        "svg_tag_bytes",
        [w for pair in SVG_TAGS for w in pair],
        "The SVG element names: lower case, correct case."))
    parts.append(literal(
        "svg_attr_bytes",
        [w for pair in SVG_ATTRS for w in pair],
        "The SVG attribute names: lower case, correct case."))
    parts.append(literal(
        "math_attr_bytes",
        [w for pair in MATH_ATTRS for w in pair],
        "The MathML attribute names: lower case, correct case."))
    parts.append(literal(
        "foreign_attr_bytes",
        [w for e in FOREIGN_ATTRS for w in (e[0], e[1], e[2], str(e[3]))],
        "The foreign attributes: name, prefix, local name, namespace."))
    parts.append(literal(
        "extra_bytes", EXTRA,
        "Names the foreign rules need beyond lib/browser/tag.fi."))

    with open(target, "w", encoding="utf-8") as fh:
        fh.write("\n\n".join(parts) + "\n")
    print("wrote %s" % target)


if __name__ == "__main__":
    main()

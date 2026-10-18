# SPDX-License-Identifier: MPL-2.0
# Builds tools/fui/fixtures/gsubnest.ttf: a tiny subset of DejaVu Sans with
# a hand-made GSUB for tools/fui/gsubctx_main.fi (multiple substitution with
# several outputs, nested context lookups, an `arab` rlig that inserts).
# Run: python3 tools/fui/gsubnest_gen.py   (needs fontTools)
import io, sys
from fontTools.ttLib import TTFont
from fontTools import subset
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

SRC = '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'
OUT = 'tools/fui/fixtures/gsubnest.ttf'

opts = subset.Options()
opts.layout_features = []
opts.glyph_names = True
opts.notdef_outline = True
opts.hinting = False
opts.name_IDs = [1, 2]
font = TTFont(SRC)
sub = subset.Subsetter(opts)
sub.populate(unicodes=[0x41, 0x42, 0x43, 0x44, 0x627, 0x628, 0x645])
sub.subset(font)

FEA = '''
languagesystem DFLT dflt;
languagesystem arab dflt;
languagesystem latn dflt;

lookup L0  { sub A by A B C; } L0;
lookup L1  { sub B by C D; } L1;
lookup L2  { sub C by D; } L2;
lookup L3  { sub B by D; } L3;
lookup L4  { sub D by C; } L4;
lookup L5  { sub C by A; } L5;
lookup L6  { sub A by D A; } L6;
lookup L7  { sub A B' lookup L3 C; } L7;
lookup L8  { sub A B' lookup L7 C' lookup L5; } L8;

lookup LM  { sub uni0628 by uni0628 uni0627 uni0628; } LM;
lookup LS  { sub uni0627 by D; } LS;

# W23-b: extras in the matching. 11/12: meem -> meem alef beh (extras), then a
# ligature of two EXTRAS (alef beh -> D), 17: a ligature from an extra into the next position (D meem -> beh). 13-15: ligatures that reach over
# extras (pos/extra mixes), 16: multiple substitution with 0 outputs.
lookup LM2 { sub uni0645 by uni0645 uni0627 uni0628; } LM2;
lookup LL  { sub uni0627 uni0628 by D; } LL;
lookup LG1 { sub A B by D; } LG1;
lookup LG2 { sub B C by D; } LG2;
lookup LG3 { sub A B C by D; } LG3;
lookup LN  { sub D by NULL; } LN;
lookup LX  { sub D uni0645 by uni0628; } LX;

feature dlig { script latn; lookup L8; } dlig;
feature rlig { script arab; lookup LM; lookup LS; lookup LM2; lookup LL; lookup LX; } rlig;
'''
addOpenTypeFeaturesFromString(font, FEA)
font.save(OUT)
t = TTFont(OUT)
go = t.getGlyphOrder()
print('glyphs', {n: go.index(n) for n in ['A','B','C','D','uni0627','uni0628']})
for i, l in enumerate(t['GSUB'].table.LookupList.Lookup):
    print('lookup', i, 'type', l.LookupType)

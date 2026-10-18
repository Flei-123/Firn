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
sub.populate(unicodes=[0x41, 0x42, 0x43, 0x44, 0x627, 0x628, 0x645,
                       0x646, 0x647, 0x648, 0x649, 0x64A])
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

# W26-c: alternate (3), reverse chaining (8), the match end of context rules.
# 18: A from [B C D] (alternate 1..3). 19: waw from [heh noon] (arab rlig).
# 20: rsub A' A by B (reverse: A A A -> A B A). 21: rsub B A' C by D
# (backtrack + lookahead). 22: A -> B. 23: sub A' lookup 22 A' -- TWO input
# glyphs, only the first has a record: after the match the walk goes on BEHIND
# the second glyph (A A A -> B A A, not B B A). 24: sub A' lookup 6 B
# (A -> D, extra A). 25: sub A' lookup 24 B' C' lookup 5 -- the nested lookup
# inserts an extra in front of the unit the second record names.
# 26: noon -> heh. 27: sub noon' lookup 26 noon' (arab rlig, as 23 on noon).
# 28: alefmaksura -> heh alefmaksura. 29: sub alefmaksura' lookup 28 (arab rlig:
# the inserted extra must be walked over). 30: rsub yeh' yeh by heh (arab
# rlig). 31: sub A' lookup 18 B (a nested alternate).
lookup LA1 { sub A from [B C D]; } LA1;
lookup LA2 { sub uni0648 from [uni0647 uni0646]; } LA2;
lookup LR1 { rsub A' A by B; } LR1;
lookup LR2 { rsub B A' C by D; } LR2;
lookup LS1 { sub A by B; } LS1;
lookup LC1 { sub A' lookup LS1 A'; } LC1;
lookup LN1 { sub A' lookup L6 B; } LN1;
lookup LC2 { sub A' lookup LN1 B' C' lookup L5; } LC2;
lookup LSn { sub uni0646 by uni0647; } LSn;
lookup LCn { sub uni0646' lookup LSn uni0646'; } LCn;
lookup LMy { sub uni0649 by uni0647 uni0649; } LMy;
lookup LCy { sub uni0649' lookup LMy; } LCy;
lookup LRy { rsub uni064A' uni064A by uni0647; } LRy;
lookup LN3 { sub A' lookup LA1 B; } LN3;

feature dlig { script latn; lookup L8; } dlig;
feature rlig { script arab; lookup LM; lookup LS; lookup LM2; lookup LL; lookup LX; lookup LA2; lookup LCn; lookup LCy; lookup LRy; } rlig;
'''
addOpenTypeFeaturesFromString(font, FEA)
font.save(OUT)
t = TTFont(OUT)
go = t.getGlyphOrder()
print('glyphs', {n: go.index(n) for n in ['A','B','C','D','uni0627','uni0628','uni0645','uni0646','uni0647','uni0648','uni0649','uni064A']})
for i, l in enumerate(t['GSUB'].table.LookupList.Lookup):
    print('lookup', i, 'type', l.LookupType)

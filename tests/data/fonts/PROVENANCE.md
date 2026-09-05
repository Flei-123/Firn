# tests/data/fonts -- the two fonts of round B3

Round B3 draws text, so it needs a font, and it needs one that lives in
the repository: a test suite that reaches into `/usr/share/fonts` measures
the machine it runs on and not the engine.

## `Ahem.ttf` -- 21,768 octets

The test font of the CSS working group, downloaded on 26 August 2026 from

    https://raw.githubusercontent.com/web-platform-tests/wpt/master/fonts/Ahem.ttf

Every glyph is a square of one em, the ascent is 0.8 em and the descent
0.2 em. That makes it the font in which a layout test can say "this box is
40 px wide" and mean it, and it is what a large part of the WPT css/ area
sets with `font: 10px/1 Ahem`. Round B2 IMITATED those metrics with a
made-up font; round B3 reads the real one.

Licence: the WPT suite is 3-clause BSD; Ahem itself is in the public
domain (Todd Fahrner, 1995).

## `FirnSans.ttf` -- 14,396 octets

A **subset of DejaVu Sans**, made here with fontTools:

```sh
python3 - <<'PY'
from fontTools import subset
opts = subset.Options()
opts.layout_features = ['*']; opts.glyph_names = True
opts.notdef_outline = True; opts.hinting = False
opts.drop_tables = ['GSUB','GPOS','GDEF','MATH','FFTM','fpgm','prep','cvt ','gasp']
f = subset.load_font('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', opts)
s = subset.Subsetter(options=opts)
s.populate(text=ASCII + 'áéíóúàèñüößÄÖÜ' + 'αβγπΩ' + '€£¥©®°±×÷–—‘’“”…')
s.subset(f); subset.save_font(f, 'FirnSans.ttf', opts)
PY
```

and the legacy `kern` table copied over from the original with the pairs
whose two glyphs survived the subsetting (469 of them).

**Why a subset and not the whole font.** 136 glyphs instead of 6253 is
14 kB instead of 750 kB, and a repository is not a font foundry. **Why
DejaVu and not something drawn here.** Because a font drawn here would be
a font that happens to use the features this reader implements. DejaVu is
a real font by other people: 13 of its 136 glyphs are **composite** (an
`é` is an `e` and an `acute` with an offset), its outlines have two
off-curve points in a row where the implied midpoint rule applies, and its
`cmap` is a real format 4 with several segments. Every one of those is a
place where a TrueType reader can be wrong, and none of them would exist
in a font made to be easy.

Licence: the Bitstream Vera / DejaVu licence -- free to use, modify and
redistribute, including in subsetted form.

## What they are measured with

`tools/paintb3/font_check.py`:

* the METRICS (units per em, ascent, descent, glyph id per character,
  advance width, side bearing, bounding box, kerning pair) against
  **fontTools**, character for character over the whole font,
* the PIXELS against a second rasteriser written in Python with a
  completely different algorithm (scanline crossings with a winding
  counter, exact in x, 16 samples per pixel row in y).

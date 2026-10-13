# Font fallback, colour emoji, grapheme clusters

Gate 135 (`tools/fontfb/run.sh`), 13.10.2026. Files: `lib/font/fallback.fi` (chain + painter), `lib/font/cbdt.fi`
(CBDT/CBLC PNG strikes), `lib/font/colr.fi` (COLR v0 / CPAL), `lib/str/grapheme.fi` (UAX #29),
`generated/grapheme_tables.fi` (from `tools/grapheme/gen.py`), `lib/font/ttf.fi` (reader).

## The chain (`font.fallback`)

```
var c: fallback.Chain = fallback.chain_new()          // maps the grapheme table
fallback.chain_add(&c, &latin, fallback.K_LATIN)      // up to 6 fonts
fallback.chain_add(&c, &emoji, fallback.K_EMOJI)
var cx: fallback.PaintCtx = fallback.ctx_new()
let adv: f64 = fallback.chain_paint(&cx, &c, text_ptr, text_len, 32.0, x, baseline, 0xFF000000, 0, pixels, w, h)
fallback.chain_free(&c)
```

* Text is cut into **extended grapheme clusters** (UAX #29, Unicode 17). A **run** is a row of clusters drawn by one font;
  a run never splits a cluster. The font of a cluster = first font in the chain whose cmap has the **first** code point
  (gid != 0). Nothing has it: font 0, `.notdef`.
* A cluster that asks for emoji (U+FE0F, ZWJ, skin tone U+1F3FB..F, regional indicator, keycap U+20E3) tries the `K_EMOJI`
  fonts first. A plain U+1F600 is **not** forced to the emoji font: if an earlier font (DejaVu Sans has many) maps it, that
  font draws it. Put the emoji font before such fonts when that is wanted.
* Painting: colour glyph kinds in this order: **CBDT/CBLC bitmap** (if the font has them), **COLR v0 layers** with CPAL
  palette 0, else the outline (optional shadow). ZWJ, VS, skin tone, keycap and tag characters (U+E0020..7F) and VS-supplement code points draw nothing of their
  own (the base glyph carries them); combining marks are drawn with the advance of their glyph (0 in DejaVu).
* `chain_width` = sum of the advances of the drawn glyphs (no kerning).
* `ttf.font_init` now accepts a font **without `loca`/`glyf`** if it has `CBDT` (bitmap-only fonts); their outlines are empty.

## CBDT / CBLC (`font.cbdt`)

* Reads index sub-tables 1, 2, 3, 5 and image formats 17, 18, 19 (PNG). PNG decoding by `paint.png` (Firn DEFLATE).
* Strike choice `cbdt_pick(px)`: smallest ppem >= px, else the largest (Noto Color Emoji has one strike: 109 ppem, 136x128).
* Scale `s = px / ppem`; the bitmap's top-left lands at `pen + bearingX*s`, `baseline - bearingY*s` (fractions kept).
  `s <= 1`: exact area average on premultiplied pixels; `s > 1`: bilinear. Source-over onto 0xAABBGGRR pixels (alpha forced 0xFF).
* Windows twin: not needed (no OS calls); not built or run there.

## Grapheme clusters (`str.grapheme`)

`gb_map/gb_free`, `gb_next(t, p, n, i)` (octets of the next cluster), `gb_count`, `gb_class`, `gb_ext_pict`, `gb_incb`.
Rules GB3-GB9c, GB11 (emoji ZWJ), GB12/13 (flags). Table: 1631 ranges (13 KB), binary search. Regenerate: `python3 tools/grapheme/gen.py`
(inputs + sha256 in `tools/grapheme/data/`, `tools/ucd/DerivedCoreProperties.txt` for InCB). No locale tailoring, no word/sentence/line breaks.

## What is proven (all on Linux x86-64, `tools/fontfb/run.sh`)

| check | against | result |
| --- | --- | --- |
| bitmap glyph at 109 px, 20 glyphs of Noto Color Emoji | Pillow 12 / FreeType, embedded colour, BASIC layout | max channel diff 1 |
| same 30 glyphs: position (bearings) and pixels | PNG inside the font via fontTools, same compositing | exact (diff 0) |
| scaled 20/32/64 px (shrink) and 150 px (enlarge), 24 images | independent numpy implementation (area average / bilinear) | diff <= 1 |
| chain run "Ab" + emoji + "e-acute" at 32 px | numpy (emoji), FreeType 8x area-reduced (letters) | emoji diff 0; letters mean diff 0.70/255 |
| runs (cluster -> font) and total width, 614 texts (fixed + random from emoji/Indic/Hangul/flag pool) | fontTools cmap/hmtx + `regex` `\X` | 0 differences |
| clusters: official `GraphemeBreakTest.txt` 17.0.0 | | 766 / 766 |
| clusters: 30000 random sequences | `regex` 2026.9.29 `\X` | 0 differences |
| clusters: 20000 random octet strings (invalid UTF-8) | tiling + progress | 0 failures |
| COLR v0 layers + palette of a test font, run/width | fontTools | as before (`tools/fontfb/probe.fi`) |
| unit test `tests/2863` | hand-made rule cases | ok |

A deliberate mutant (bearing off by 1 px; GB11 removed) is caught by the gate (75 / 62 failures).
Found and fixed on the way: `paint_outline` made its raster window `ascent - descent` tall (descent is stored positive), which
**cut every glyph a few pixels above the baseline**.

## NOT tested / NOT done (honest)

* **sbix** (Apple bitmaps), **COLR v1** (gradients, transforms), **SVG-in-OpenType**, CBDT image formats 1/2/5/6/7 and index format 4: not read.
  Only Noto Color Emoji (CBLC index 1 + image 17) was run; index formats 2/3/5 and image formats 18/19 are implemented but **never exercised**.
* **No shaping**: no GSUB ligatures through the chain, so a **flag** (two regional indicators) and a **ZWJ family** are drawn as their
  parts side by side (`gsub.fi` exists for Arabic in fUi, it is not wired into the chain). No GPOS, no kerning across glyphs, no bidi in the chain.
* Only the first CPAL palette; no foreground-colour glyph recolouring for bitmaps; no emoji-presentation default
  (Emoji_Presentation) logic -- see the plain-U+1F600 note above.
* Chain limits: 6 fonts, 64 runs per call (the rest is dropped); fonts are read from memory (the caller loads the 11 MB file).
* Hinting: the own rasteriser draws unhinted; no sub-pixel (LCD) text.
* Reference versions: regex module Unicode data may differ from 17.0 for brand-new code points (none in the pools).
* Windows / Android / a real GPU path are not part of this gate.

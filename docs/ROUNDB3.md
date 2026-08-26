# Round B3 — painting: from a tree of rectangles to a picture

Round B2 ended with a tree in which every box had a position and a size,
and said plainly what was missing: there was no picture. Nobody could look
at anything. This round is that step, and — as in B1 and B2 — the step
matters less than the **measuring stick** that comes with it.

---

## 1. The numbers

### 1.1 The official reference tests

The corpus is the `css/css-backgrounds/` and `css/css-color/` areas of the
**Web Platform Tests**, every file in them that carries
`<link rel="match">`, together with the reference file it names: **541
pairs** of documents that have to look the same. What is in the corpus and
by which rule is in `tests/data/wpt-ref/PROVENANCE.md`; nothing was picked
by hand.

| | pairs | quota |
|---|---|---|
| pictures equal **and not empty** | **202 / 541** | **37.34 %** |
| equal but empty (`vacuous`, **not** counted) | 32 | 5.91 % |
| pictures differ | 307 | 56.75 % |

**The 32 are the honest part of this table.** A reference test is the
easiest measurement in the world to pass by accident: an engine that draws
nothing passes every one of them, because both sides come out white and
white equals white. `tools/paintb3/reftest.py` therefore counts a pair only
if the test rendering also differs from a blank page in at least 64 pixels,
and prints the empty matches separately. Had they been counted, this round
would be reporting 43.25 % — and 32 of those tests would be passing for the
one reason that must never count.

### 1.2 The font: 393 glyphs, 0 metric deviations

| | |
|---|---|
| characters checked against **fontTools** (units per em, ascent, descent, glyph id, advance, side bearing, bounding box) | 408 |
| deviations | **0** |
| kerning pairs checked against fontTools | 469 |
| deviations | **0** |
| glyphs whose PIXELS agree with an independent rasteriser | **393 / 393** |
| mean deviation of the coverage per glyph | 0.0010 (FirnSans), 0.0009 (Ahem) |
| largest deviation of any single pixel, over all 393 glyphs | **0.049** |
| glyphs drawn empty that should not be | **0** |

### 1.3 Do the letters fit the boxes the layout made?

This is the question section 3 of the round asked by name, and it is worth
its own measurement because a "yes" is worth nothing without a
counter-check that can say "no".

| | |
|---|---|
| ink standing outside a fixed width box, 4 texts × 4 widths | **0 px** |
| a shrink-to-fit box wider than the ink it holds, **metrics fed back** | **2 px** |
| the same with the round B2 font (`textfit.py` counter-check) | **63 px** |

The 2 px are the right side bearing of the last glyph, which is inside the
box and correctly so. The 63 px are what the layout of round B2 believed
about a proportional typeface: one em per character, whatever the
character. Without the counter-check the 2 px would prove nothing.

### 1.4 PNG, and the time it all takes

PNG is written and read here, and each direction is measured against
**Pillow**, which shares no line of code with it: a page rendered as PPM
and as PNG, read back by Pillow — largest deviation **0**; and four colour
types × the five PNG row filters written by Pillow and read back by
`lib/paint/png_main.fi` — largest deviation **0**.

Times, averaged over the 1082 renderings of the reference corpus, at
800 × 600 (AMD EPYC 7571, one core, release build):

| step | per page |
|---|---|
| HTML + CSS + cascade + layout (rounds B1 and B2) | 5.0 ms |
| **building the display list** | **0.02 ms** |
| **rastering it** | **31 ms** |
| the whole page | 36 ms |

The display list is a thousand times cheaper than drawing it, which is the
whole argument for having one: the expensive half can be skipped when
nothing has changed, and the cheap half can be rebuilt and compared.

### 1.5 What did not get worse

| | before this round | after |
|---|---|---|
| html5lib tree construction (round B1) | 1837 / 1936 | **1837 / 1936** |
| WPT layout, corpus B2 | 59 / 186 | **59 / 186** |
| reflow 800 → 400 → 800 | 471 / 471 | **471 / 471** |
| the own cases against Chromium (rounds 61/67) | 1087 / 1087 boxes, 5171 / 5171 probe points | **unchanged** |

The layout numbers are unchanged **on purpose**: the new font metrics are
switched on by a handle on the box tree, and round B2's programme does not
pass one. Its corpus is Ahem, whose glyphs really are one em wide, so the
two would agree there anyway — but "would agree" is not a measurement, and
leaving the old path bit-identical is.

---

## 2. What was built

### 2.1 The display list — and why the order is the hard part

`lib/paint/display.fi` turns the box tree into an ordered list of drawing
commands: fill a rectangle, fill it with a gradient, four borders, a run of
text, a shadow, a clip. It would have been possible to walk the box tree
and draw as one goes. A browser does not, and not out of tidiness:

* **the order is not the tree order.** CSS 2.1, Appendix E puts a float
  over the background of the paragraph beside it and **under** its letters;
  it puts a positioned box with no `z-index` over every ordinary block,
  however deep that block sits; and it makes a `z-index: 1` inside a
  `z-index: 0` unable to escape its parent, however large the number.
* **a list can be counted, printed and compared.** A `draw_rect` deep in a
  recursion can only be checked by looking at the picture afterwards.
* **the two halves can be timed apart** — see 1.4.

The order itself is not new: `lib/layout/stack.fi` computed it in round 67
and it was measured against Chromium at 5171 probe points with
`document.elementFromPoint`. What round B3 had to add is the **phase**. A
box is not painted in one go — its background and border belong to step 3
or 4 of Appendix E and its **text** to step 5, after the floats. An order
with one entry per box cannot say that, and a float would have covered the
letters beside it. `paint_order_ops` is the same walk with `PH_BOX` and
`PH_TEXT` on every entry; `paint_order` is untouched, so the proof of round
67 still means what it meant.

The clip is carried the same way. A flat order has no subtree left to
bracket, so `overflow: hidden` is stamped onto every box before the walk
(`box_stamp_clip`) and every command carries the rectangle its clipping
ancestors leave over. The same trick, for the same reason, carries the
inherited `opacity` (`box_stamp_alpha`).

### 2.2 The rasteriser

`lib/font/raster.fi`. **Signed area accumulation** — the algorithm
`font-rs` made popular — rather than the classic active edge table, and the
reason is anti-aliasing: an active edge list needs a second, finer
subdivision inside the pixel row to produce grey at all, typically 4 or 16
sub-scanlines, which is 4 or 16 times the work for 4 or 16 grey levels.
This method walks every edge once and writes into each pixel it touches the
**exact, analytically computed area** the edge cuts out of it. A 45 degree
edge through the middle of a pixel gives 0.5, not "two of four samples".

It is checked on shapes whose area is known:

```
rectangle 10.5 x 7.25    76.125 covered pixels   (exact:  76.125)
triangle 30 x 20        300.000                  (exact: 300)
the same, wound the other way   300.000
a ring, 20x20 minus 10x10       300.000
a rectangle hanging out of the window on two sides  100.000
a quarter circle of radius 20   313.349           (exact: 314.159)
```

The last line is the only one that is not exact, and it is not an error:
the curve is cut into 12 straight pieces, and an inscribed polygon has less
area than its circle. The deviation of the outline is 0.043 px — under a
twentieth of a pixel, below what the anti-aliasing of the next step can
show. The step count is derived from that number and written down in the
file.

The file has **no imports at all**: `std.math` is not `#[no_gc]`, and the
four pieces of arithmetic it needs are eleven lines. That is what makes it
usable from a kernel — see section 5.

### 2.3 What can be drawn

* **rectangles**, with the fast path taken seriously: an opaque rectangle
  with no radius and no mask fills its interior with stores instead of
  blends. On a page whose background is one 800 × 600 rectangle that was
  the single biggest item in the raster time (127 ms → 44 ms per page when
  it went in).
* **round corners**, with the overlap rule of css-backgrounds-3, 5.5 —
  `border-radius: 100px` on a 50 px box gives 25 px corners **on all four
  corners**, not 50 on one.
* **borders in eight styles**: `solid`, `double`, `dotted`, `dashed`,
  `groove`, `ridge`, `inset`, `outset`. A border is not a shape but the
  **difference of two shapes**, and the difference is taken on the coverage
  and not on the pixels — drawing the outer rectangle and then the inner
  one in the background colour is visibly wrong the moment anything is
  behind the box. Which side a pixel of the ring belongs to is the mitre
  rule: the corner is cut along the line from the outer corner to the inner
  one, so a 1 px top and a 20 px left meet at the right angle.
* **gradients**, `linear` and `radial`, with the angle convention of CSS —
  `0deg` points at the **top** and the numbers grow **clockwise**, which is
  not the convention of mathematics and is the classic way to get a
  gradient upside down. Stops without a position are spread evenly between
  the ones that have one.
* **shadows**, `box-shadow` outer and `inset` and `text-shadow`, with a
  real Gaussian blur done as three box passes with a **running sum**, so
  the cost does not grow with the radius. The relation between the CSS blur
  radius and the standard deviation is the one of css-backgrounds-3, 6.2:
  the radius is twice the deviation. An outer shadow is not painted under
  its own border box, or a half transparent box would show its own shadow
  through itself.
* **transparency** and the **seven separable blend modes** of
  css-compositing-1, on a premultiplied canvas.
* **clipping**, rectangular and — through a mask — round.

### 2.4 The TrueType reader

`lib/font/ttf.fi` reads `head`, `hhea`, `maxp`, `hmtx`, `cmap` (formats 0,
4, 6 and 12), `loca`, `glyf` including **composite glyphs** with a full 2×2
transform, and `kern` format 0. It never allocates: it is handed the file
and a scratch buffer and writes its outlines straight into the rasteriser.

Two subtleties of the format are where an implementation is usually wrong,
and both are handled and commented at the point they occur:

* **two off-curve points in a row imply an on-curve point exactly between
  them.** A font that draws a circle with four control points and no
  on-curve point at all is legal, and an implementation that does not
  insert the implied midpoints draws a diamond.
* **`hmtx` repeats its last entry** for every glyph beyond
  `numberOfHMetrics`. That is how a monospaced font stores one number for
  3000 glyphs, and forgetting it gives every glyph after the first few an
  advance of zero.

### 2.5 The road back to the layout

`lib/font/metrics.fi` is the one place that answers "how wide is this
character", and both halves ask it: the layout through `layout.box`, the
painter directly. `lib/layout/flow.fi` now sums **per-character** advances
with kerning instead of multiplying one number by the character count, and
`lib/layout/box.fi` records, for every word it places, **where it stands**
— up to round B2 the item list was working material and was thrown away
after the last line, so nothing anywhere knew which characters were where.
A painter cannot be built on top of that.

The handle is a raw `u64` on the box, not a managed pointer: the box tree
is collected and the font is not. `0` means "no font", and that is not an
error but exactly the behaviour of round B2, which its corpus is measured
against.

---

## 3. How text was checked, and why not the obvious way

**The warning this round was given, from round K7B in the kernel:** there,
a screen was measured as 87 per cent correct while every single letter was
missing. The 87 per cent were the black background. Whoever checks text by
counting pixels against the whole area is measuring the background.

So text is checked three times, and never against an area:

**Per glyph, against an independent rasteriser.** `font_check.py` renders
every one of the 393 glyphs of the two fonts and compares it with a second
rasteriser written in Python with a deliberately different algorithm — a
textbook scanline crossing test with a winding counter, exact in x and
sampled 16 times per pixel row in y. The outline both are fed comes from
**fontTools**, a TrueType parser nobody here wrote, which also decomposes
the composite glyphs. Reported per glyph: the ink of each, the mean
deviation of the coverage, the largest deviation of any single pixel, and
the intersection over union — and a glyph with zero ink where the reference
has ink is a failure, not a rounding difference.

*A yardstick has to be checked too.* The first version of this file used
matplotlib's `Path.contains_points` as the reference. It was thrown out
because it is wrong for this job: matplotlib treats the subpaths of a
compound path as a union, so the counter of an `O` comes back filled. It
reported `O`, `D`, `Q`, `©` and `®` as broken with an overlap of 0.45, and
the engine was right about all five.

*And the metric was chosen after looking at what it does.* Intersection
over union counts pixels above half coverage, so a vertical stem whose edge
falls on exactly 0.500 flips a whole column for a difference of one part in
a hundred. Greek beta at 48 px does exactly that: largest single-pixel
deviation 0.049, IoU 0.92. The quota is decided by the coverage, not by the
IoU; the IoU is printed beside it, because a glyph drawn in the wrong place
has a bad IoU *and* a bad deviation.

**Per page, as a count that cannot be re-frozen away.** Each of the seven
own cases carries in `expected.txt` not only the hash of its picture but
the number of octets it painted and **the number of pixels its glyph
rasteriser really set**. Both have to stay within half of the frozen value.
A hash alone would not catch a regression to an empty page: whoever
re-froze the hash would freeze the empty picture with it. A glyph count of
zero cannot be re-frozen without the number changing in the commit,
visibly.

**Per corpus, as a guard on the quota.** See 1.1.

---

## 4. What is open, and named

Openly, because a list of limits is worth more than a quota that hides
them. Of the 307 failing reference pairs, the largest groups are:

| what is missing | failing pairs that use it |
|---|---|
| `url(...)` — a bitmap in a background | 98 |
| `lab()`, `lch()`, `oklab()`, `oklch()`, `color()` | 78 |
| `background-repeat` | 55 |
| `background-size` | 50 |
| `border-image` | 34 |
| `<script>` — the reference test scripts itself | 29 |
| `<table>` layout | 24 |
| `background-attachment` | 18 |

(The columns overlap: a test can need two of them.)

In detail, and in the engine's own words:

* **B3-1 NO BITMAPS ON THE PAGE.** PNG is decoded (`decode_png`, colour
  types 0, 2, 4 and 6 at eight bits, all five row filters, checked against
  Pillow) and PPM and PNG are written. What is missing is the step in
  between: `<img>` is not a replaced element in this engine's box tree, so
  a decoded picture has nowhere to go. **JPEG is not decoded at all** — the
  round's own priority said PNG first and say so if the time runs out, and
  the time ran out. Neither is a `background-image: url(...)`.
* **B3-2 NO `background-repeat`, `-size`, `-position`, `-origin`,
  `-attachment`.** A background is painted once, over the whole box, and
  `background-clip` (border, padding, content) is the only one of the six
  that is implemented.
* **B3-3 ONE SHADOW PER PROPERTY.** `box-shadow` and `text-shadow` take a
  comma separated list; the first is kept and the rest ignored.
* **B3-4 ONE FONT PER PAGE.** There is no `font-family` matching and no
  `@font-face`. The harness gives a page Ahem if it names Ahem and the
  engine's own font otherwise, and gives the SAME font to a test and its
  reference.
* **B3-5 `opacity` IS NOT A LAYER.** It is multiplied into the alpha of
  every command of the subtree instead of the subtree being composited
  once. The difference shows only where two children of the same half
  transparent parent overlap: a layer blends once there, this blends twice.
* **B3-6 NO `transform`, NO `filter`, NO `clip-path`.** The clip is a
  rectangle, or a mask where a rounded box clips — a `clip-path` polygon is
  not parsed.
* **B3-7 THE NON-SEPARABLE BLEND MODES** (`hue`, `saturation`, `color`,
  `luminosity`) are missing; the seven separable ones are there.
* **B3-8 GRADIENTS: linear and radial only**, up to six stops, no
  `conic-gradient`, no `repeating-*`, and the radial one is always the
  default — an ellipse in the middle reaching to the farthest corner.
* **B3-9 COLOURS: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`,
  `rgba()`, `hsl()`, `hsla()`, the named colours, `currentcolor`,
  `transparent`.** The modern colour spaces (`lab`, `lch`, `oklab`,
  `oklch`, `color()`, `color-mix()`) are not parsed — 78 of the failing
  pairs, and the largest single item on the list.
* **B3-10 NO HINTING.** The instructions in `fpgm`, `prep` and `glyf` are
  skipped. At 11 px that is visible against a browser; against the
  reference rasteriser of this round it is not, because that one does not
  hint either.
* **B3-11 CFF/OpenType outlines are refused**, not silently drawn empty. A
  font with PostScript outlines needs a second outline reader.

---

## 5. The doubling with Osum, and where the shared code should live

Round K10 in the **Osum repository** is building a TrueType reader and a
glyph rasteriser at the same time, for the window server, and that is a
real doubling: the same two files, twice, in two repositories that will
drift apart the moment one of them fixes a bug.

**It should not be two copies, and the place for the one copy is
`lib/font/` in Firn** — which is where this round put it, on purpose and in
a shape that can be lifted out:

```
lib/font/raster.fi   the scanline rasteriser.  NO imports at all, every
                     function #[no_gc], allocates nothing -- the caller
                     hands it memory with `raster_attach` and asks
                     `raster_bytes_needed` how much.
lib/font/ttf.fi      the TrueType reader.  Imports font.raster and
                     nothing else, every function #[no_gc], allocates
                     nothing -- the caller hands it the file and one
                     scratch buffer for the points of one glyph.
lib/font/metrics.fi  ascent, descent, advance, kerning.  Imports
                     font.ttf, #[no_gc] throughout.
```

None of the three touches the garbage collector, the heap, `std`, or a
system call. That is not an accident of style: **a kernel has no heap while
it is drawing**, and a `Gc[...]` in a window server is not an option. The
three files compile under `profile kernel` as they stand.

What is **not** shareable and should stay on the browser side is
`lib/paint/` — the display list knows about `Box`, `Style` and the cascade,
and a window server has none of those. The boundary between the two is
exactly one function call: `font_outline(font, gid, raster, pen_x,
baseline_y, scale, scratch, cap, 0)`.

**The concrete proposal for Osum K10**, in the order it should happen:

1. K10 does **not** write its own reader. It takes `lib/font/raster.fi`,
   `lib/font/ttf.fi` and `lib/font/metrics.fi` from this repository as they
   are.
2. Osum already builds against Firn's `lib/` (its `FIRNLIB` points here),
   so the three files need no copy — an `import font.ttf` is enough. Where
   the two repositories are built apart, the three files are the ONE
   directory that is vendored, and the vendoring script names the commit.
3. `tools/paintb3/font_check.py` moves with them, or is duplicated: it is
   the yardstick, and a shared reader with two different yardsticks is
   worse than two readers. It needs nothing from the browser — a font file
   and a program that prints glyph bitmaps.
4. What K10 will need and this round did not build: **a glyph cache**. The
   browser rasterises a glyph every time it draws it, which is fine at
   31 ms a page and not fine at 60 frames a second. The cache belongs in
   `lib/font/`, next to the two, and it belongs to whoever needs it first.

---

## 6. How to run it

```sh
bash tools/paintb3/run.sh          # the whole section 62, about 3 minutes
```

and one page by hand:

```sh
compiler/target/release/firnc -o /tmp/paint lib/paint/b3_main.fi
python3 - <<'PY' > /tmp/page.png
import struct, subprocess
blk = lambda b: struct.pack('<I', len(b)) + b
html = b"<!doctype html><body><p style='font-size:28px;color:#036'>Firn</p>"
job  = (struct.pack('<II', 400, 200) + blk(html)
        + blk(open('lib/dom/ua.css','rb').read()) + blk(b'')
        + blk(open('tests/data/fonts/FirnSans.ttf','rb').read())
        + struct.pack('<I', 1))          # 1 = PNG, 0 = PPM, +2 = report
import sys; sys.stdout.buffer.write(
    subprocess.run(['/tmp/paint'], input=job, capture_output=True).stdout)
PY
```

The corpus is re-fetched with `tools/paintb3/harvest.py`; it is never
called from `test.sh`, and neither is anything else that opens a socket.

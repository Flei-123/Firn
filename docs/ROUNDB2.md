# Round B2 — layout: from the tree with styles to a tree of rectangles

Round B1 ended with a DOM and a computed style on every element, and said
plainly what was missing: nobody knew **where** anything stood. This round
is that step — and, more than the step itself, it is the **measuring
stick** for it.

The layout code did not start from nothing. Rounds 61 and 67 had built the
box model, the block and inline flow, floats, positioning and a flexbox,
and measured all of it against a real Chromium **on cases written here**.
That is a proof that the engine does what its author thought. It is not a
proof that the author was right about the standard. This round asks a
suite nobody here wrote.

---

## 1. The numbers

The corpus is the `css/` area of the **Web Platform Tests**; what is in it
and by which rule is section 2. The same harness was run against three
engines: the tree as round B1 left it (`main`, i.e. the layout of rounds
61 and 67), the tree after this round, and **Chromium 141** — the browser
answering the same questions through the same comparison.

| | tests of corpus B2 | quota | single checks |
|---|---|---|---|
| before this round (`main`) | **42 / 186** | 22.58 % | 3044 / 4867 |
| after this round | **59 / 186** | **31.72 %** | 3535 / 4867 |
| Chromium 141, same harness | **138 / 186** | 74.19 % | — |

The third line is the one that keeps the second honest. 26 of the 186 are
`margin-trim` tests, and Chromium fails **all** of them (the property is
not implemented there either); a further 12 need an image, a video or an
SVG to be decoded. The 74 % is what an engine that is not this one reaches
on this corpus — not 100 %, and the difference to 100 is not a defect of
the corpus but the price of harvesting mechanically instead of picking
what suits.

Per directory, so the reader can see where the engine stands and where it
does not:

```
directory                                    tests   main     B2   Chromium
css/css-flexbox                                 66     22     31     52
css/css-sizing/stretch                          20      6      7     20
css/css-box/margin-trim/computed-margin-values  19      5      5      0
css/css-flexbox/abspos                          18      0      5     18
css/css-sizing                                  16      4      5     16
css/css-box/margin-trim                          7      0      0      0
css/css-flexbox/alignment                        7      0      0      7
css/css-flexbox/intrinsic-size                   7      0      1      5
css/css-sizing/aspect-ratio                      7      0      0      4
css/CSS2/floats                                  4      2      2      4
css/css-align/blocks                             4      0      0      2
css/css-sizing/contain-intrinsic-size            3      0      0      3
css/CSS2/normal-flow                             2      2      2      2
css/css-align/baseline-rules                     2      0      0      2
css/CSS2/linebox                                 1      0      0      0
css/css-align/abspos                             1      0      0      1
css/css-flexbox/balance                          1      0      0      1
css/css-position                                 1      1      1      1
TOTAL                                          186     42     59    138
```

The three groups that are **not** corpus B2 are measured as well and
printed by the same run, so that nothing disappears by being put aside
(section 2.2):

```
vertical (writing modes, rtl)     0 / 171 tests    3053 / 7984 checks
grid                              1 /  22 tests      54 /  151 checks
script (needs a scripted DOM)     6 /  92 tests     483 / 1874 checks
everything but `script`          60 / 379 tests    6642 / 13002 checks
```

And the second measurement of this round, the one about **reflow**:

| | documents identical after 800 → 400 → 800 |
|---|---|
| a second `layout_document` without a reset (`main`) | **269 / 471** |
| `flow.relayout_document` (this round) | **471 / 471** |

The old proof of rounds 61 and 67 did not get weaker: `tools/layout/run.sh`
still reports **1087 / 1087 own boxes, 1087 / 1087 equal to Chromium,
deviation 0.00 %, paint order 5171 / 5171**, in all three build stages.

---

## 2. The measuring stick, and how a reference test is read without pixels

### 2.1 Why the WPT css/ area can be used at all

Most of `css/` in the Web Platform Tests are **reftests**: the test file
and a reference file are rendered and the PICTURES are compared. An engine
that stops at the box tree cannot run those, and inventing a pixel
comparison for it would be inventing the thing this round is not about.

But a large part of the suite is self-describing in another way. The test
carries its expectation in the markup and `resources/check-layout-th.js`
checks it:

```html
<div data-expected-width="100" data-offset-y="20">
```

What that script compares are the **CSSOM View accessors**:
`offsetWidth`, `offsetHeight`, `offsetLeft`, `offsetTop`, `clientWidth`,
`clientHeight`, `clientLeft`, `clientTop`, `scrollWidth`, `scrollHeight`,
`getBoundingClientRect()`, the computed `display` and the used margins and
paddings. Those are **position and size, not pixels** — exactly what a
layout produces. The expectations were written by the CSS working group
and by the engineers of the other browsers.

So the method of this round is:

* `lib/layout/b2_main.fi` prints those accessors, per element, for every
  element that carries a `data-` attribute, and the `data-` attributes
  with them (so the harness never parses the HTML a second time with a
  foreign parser),
* `tools/layoutb2/harness.py` compares, attribute for attribute, with the
  **same tolerance** `check-layout-th.js` uses: its `assert_tolerance`
  fails only when `Math.abs(actual - expected) >= 1`. A difference below
  one pixel passes. That is the suite's own rule, and it exists because a
  browser reports these accessors as rounded integers while a layout
  engine computes in sixty-fourths.
* A test counts as passed only when **every** one of its checks passes.

The accessors are not a formality. Two of them are worth naming, because
getting them wrong marks correct layout as wrong:

* `offsetLeft` is measured from the **padding edge** of the offsetParent,
  not from its border edge.
* **Except when the offsetParent is the unpositioned `body`**: then every
  engine returns the coordinate in the initial containing block. Measured,
  not guessed — a `div` in a `body` with `margin: 8px; border: 4px;
  padding: 6px` reports `offsetLeft` 18 in Chromium 141, and 18 is its
  distance from the initial containing block, not 0 from the padding edge
  of the body.

### 2.2 Which tests are in the corpus

Everything harvested lies in `tests/data/wpt-css/` (`PROVENANCE.md`
there): every file of the WPT directories `css/css-flexbox`, `css/CSS2`,
`css/css-box`, `css/css-sizing`, `css/css-position` and `css/css-align`
that includes `check-layout-th.js` — **471 files**, unfiltered inside
those directories.

Out of those, three groups are set aside **mechanically** by
`harness.py`, and each is counted and printed:

| group | rule | why |
|---|---|---|
| `script` | an inline `<script>` does more than call `checkLayout()` | the test builds its own document from JavaScript; `lib/js/` is not wired to the DOM, so the document under test never comes into being |
| `grid` | the file uses `display: grid`, `grid-template`, `grid-auto-*` | CSS grid is the NEXT round; this one is box model, normal flow, inline flow, floats, positioning, flexbox |
| `vertical` | a vertical `writing-mode` or `direction: rtl` | css-writing-modes is a module of its own and this engine has none |

The rest — **186 tests** — is corpus B2. The rules are three regular
expressions in `harness.py`; nobody had to decide test by test, and the
quota of every group is printed next to the headline number.

### 2.3 The font

Many of these tests set `font: 10px/1 Ahem`. Ahem is the test font of the
CSS working group: every glyph is a square of one em, ascent 0.8 em,
descent 0.2 em. `lib/layout/box.fi` computes with **exactly those
metrics**, for every font, since round 61 (`font_advance` = font size,
`font_ascent` = 0.8, `font_descent` = 0.2). An Ahem test therefore
measures here what it measures in a browser. A test that uses another font
does not, and no real font is read — that is B3. The interface is the four
functions in `box.fi`; a real font stands in the same place.

### 2.4 The calibration

`tools/layoutb2/chrome_check.py` runs the whole corpus through a real
Chromium with the **same comparison** (the JavaScript in that file reads
the same attributes and applies the same one-pixel tolerance). It answers
two questions the engine under test cannot: whether the rules of the
harness are right, and how many of these tests can be passed at all. Its
result is the 138 in the table above.

It needs a browser and the Ahem font and is therefore **never** called
from `test.sh`. Round 78 froze the browser out of the acceptance on
purpose, and this round does not thaw it.

---

## 3. What the layout can do, and what it cannot

### 3.1 What stands (rounds 61, 67 and this one)

* **The box model**: content, padding, border, margin; `box-sizing`;
  `min-`/`max-` on both axes; percentages of margin and padding against
  the WIDTH of the containing block, also the vertical ones.
* **Margin collapsing**, including the two hard parts: margins that
  collapse **through** a box (`thr_pos`/`thr_neg` next to
  `top_pos`/`bot_pos`), and margins that **leave** a box upwards when no
  border, padding or formatting context stands in the way.
* **Normal flow**: block boxes under each other, inline boxes next to each
  other, `inline-block`, anonymous block boxes around runs of inline
  content, line boxes with baselines and `vertical-align`.
* **Line breaking**: words measured, broken at the allowed places,
  `line-height`, `text-align` including `justify`, `white-space`
  (`normal`, `pre`, `nowrap`, `pre-wrap`, `pre-line`).
* **Positioning**: `static`, `relative`, `absolute`, `fixed`, `sticky`;
  `z-index` and stacking contexts with the seven steps of CSS 2.1
  Appendix E; `float` and `clear` with block formatting contexts.
* **Flexbox** after css-flexbox-1: both axes, `flex-grow`/`shrink`/
  `basis`, line breaking, `justify-content`, `align-items`/`-self`/
  `-content`, `order`, automatic margins, the automatic minimum size —
  and, new here, the static position of absolutely positioned children.
* **The keyword sizes** `min-content`, `max-content`, `fit-content` and
  `stretch` (css-sizing-3/4), in the inline axis fully, in the block axis
  `stretch`.

### 3.2 What this round FOUND, and it is the point of the exercise

Five real defects, all of them invisible to the 146 cases of rounds 61
and 67, all found by the foreign suite or by an own case written against
Chromium:

1. **The anonymous block box carried the style of its parent.** A block
   with padding, a border or a margin that holds BOTH text and a block
   gave the anonymous box around the text the parent's padding, border and
   margin a **second time** — everything below it stood 36 px too low in
   the test that found it. Fixed with `cascade.style_anonymous`: initial
   box properties, inherited properties from the parent
   (`tests/1185_layout_anon_edges.fi`).
2. **A flex container without a definite main size shrank its items to
   nothing.** `display: flex; flex-direction: column` without a height —
   the most ordinary flex container on the web — measured **zero**,
   because an indefinite main size arrived as 0 and step 9.7 then found a
   negative free space. The used main size of such a container IS the sum
   of the hypothetical sizes of its items (css-flexbox-1, 9.3.1), so the
   free space is exactly zero (`tests/1186_layout_flex_base.fi`).
3. **Intrinsic widths did not see the margins of their children.** They
   are asked BEFORE the subtree is laid out, and until then the child's
   `ml`/`pl` are zero. A flex item whose child carries a margin came out
   too narrow by exactly that margin.
4. **`start` is not `flex-start`.** Round 67 folded `start`, `left`,
   `self-start` into `flex-start`. Under `flex-direction: row-reverse` the
   physical pair and the flex-relative pair are each other's mirror image,
   and eighteen WPT tests ask exactly that.
5. **A wrapping container with one line is not a single-line container.**
   The rule "a single-line container with a definite cross size hands that
   size to its line" was applied whenever ONE line came out — which
   removes all free space and makes `align-content` do nothing.

Plus the two features the corpus demanded and the engine did not have: the
**static position of abspos children of a flex container**
(css-flexbox-1, 4.1: aligned as the sole flex item, with the
single-item fallbacks of css-align-3) and the **keyword sizes**.

### 3.3 What is missing, named

* **Replaced elements have no intrinsic size.** `<canvas width=10
  height=10>`, `<video>`, `<svg>`, `<object>` — the default 300×150 and
  the sizes out of the HTML attributes are not read, and an image is not
  decoded at all. That is the largest single block of failures in the
  corpus (24 of the remaining tests touch it).
* **`aspect-ratio`** (css-sizing-4) is not implemented — 10 tests.
* **`gap` / `row-gap` / `column-gap`** are not in the cascade — 6 tests.
* **`margin-trim`** — 26 tests, and no browser passes them either.
* **Writing modes and `direction: rtl`** — the whole `vertical` group,
  171 tests, measured and reported at 0.
* **CSS grid** — the next round.
* **Tables** are a display type in the cascade but not a formatting
  context: a table lays out as a block.
* **Form controls** (`<input>`, `<button>`, `<select>`) have no intrinsic
  size of their own.
* **`scrollWidth`/`scrollHeight`** are computed from the box tree without
  a scrolling box of their own, and `overflow: scroll` reserves no
  scrollbar.
* The **computed `display`** is reported as it comes out of the cascade;
  css-display-3 blockifies it for floats, abspos boxes and flex items at
  computed-value time. Two tests in the corpus ask for it.

---

## 4. The layout tree, and what a reflow has to redo

B3 draws what this round produces, and it will have to draw it again when
the window changes or a script moves something. So the split matters more
than the tree format:

**Viewport-independent** — survives a reflow:

* the **box tree** itself (`lib/layout/build.fi`): which boxes exist, in
  which order, which of them are anonymous, which are out of flow. It
  changes only when the DOM or a `display` changes.
* the **style** on every box.
* the **intrinsic contribution** of every box, `int_min`/`int_max` with
  the flag `int_ok`. Style and text alone decide it — percentages count as
  zero there (css-sizing-3) — so no window width can change it. It is
  computed once, on demand, and read out of the box afterwards. It is the
  only cache in this engine, and this is why it is allowed to exist.

**Viewport-dependent** — thrown away and computed again
(`box.reset_box_geometry`):

* every used value: `x`, `y`, `w`, `h`, the four rings `m*`, `b*`, `p*`,
  the baseline, the static position, the shift of `position: relative`,
  the collapsed margin sets,
* the **line boxes** (`b.lines`), the **inline items** (`b.items`) and the
  **fragments** of an inline box (`b.frags`),
* the flags `placed`, `static-abs`, `frozen`, `collapse-through`.

`flow.relayout_document(root, vw, vh)` is exactly those two sentences in
code: reset, then `layout_document`. The proof that the line is drawn in
the right place is the number in section 1 — **471 of 471** documents of
the whole harvest give, after 800 → 400 → 800, the very same output as a
single layout at 800. The counter-check is in the same table: calling
`layout_document` twice **without** the reset, as `main` does, leaves only
**269 of 471** documents unchanged. Line boxes are appended twice, a
placement run adds its offsets a second time — a browser built on that
would drift with every resize.

What B3 reads: one `Box` per fragment with the **border box** in viewport
coordinates (`x`, `y`, `w`, `h` — the rectangle
`getBoundingClientRect()` returns), the four rings around it, the line
boxes with their baselines under `b.lines`, the fragments of inline boxes
under `b.frags`, and the paint order as a flat vector from
`stack.paint_order` (round 67), which already implements the seven steps
of CSS 2.1 Appendix E.

---

## 5. What was built

```
lib/layout/b2_main.fi        NEW  the CSSOM driver: offsetLeft/Top/Width/
                                  Height, clientLeft/Top/Width/Height,
                                  scrollWidth/Height, getBoundingClientRect,
                                  the computed display, the used margins
                                  and paddings, and the data- attributes.
                                  With `--reflow` it lays every document
                                  out at 800, 400 and 800 again.
lib/layout/flow.fi                the static position in a flex container,
                                  `align_norm` for start/end, the line
                                  cross size of a wrapping container, the
                                  keyword sizes, the intrinsic widths that
                                  see the edges of their children, the
                                  container without a definite main size,
                                  the intrinsic cache, relayout_document
lib/layout/box.fi                 int_min/int_max/int_ok,
                                  reset_box_geometry, the keyword units
                                  are not lengths
lib/layout/build.fi               the anonymous box gets an anonymous style
lib/css/cascade.fi                style_anonymous, AL_START/AL_END,
                                  the four keyword size units

tools/layoutb2/harness.py    NEW  the comparison, attribute for attribute
                                  after check-layout-th.js
tools/layoutb2/chrome_check.py NEW the same corpus through a real Chromium
                                  (calibration, never in test.sh)
tools/layoutb2/run.sh        NEW  three build stages, the corpus, the
                                  reflow proof, the regression limits
tools/layoutb2/minquota.txt  NEW  59 tests, 471 documents

tests/data/wpt-css/          NEW  471 official tests + PROVENANCE.md
tests/1185_layout_anon_edges.fi     NEW
tests/1186_layout_flex_base.fi      NEW
tests/1187_layout_reflow.fi         NEW
tests/1188_layout_keyword_sizes.fi  NEW
test.sh                           section 61
```

The four test programs carry numbers **computed by hand** from CSS 2.1,
css-flexbox-1 and css-sizing-3 and held against Chromium 141 afterwards
with the same document — so that no arithmetic slip of the author is
frozen into a test.

---

## 6. How to check it

```sh
bash tools/layoutb2/run.sh          # the whole section, no browser needed
python3 tools/layoutb2/harness.py .layoutb2-work/b2_opt --show 10
                                    # the first ten failing tests, in detail
python3 tools/layoutb2/harness.py .layoutb2-work/b2_opt --reflow-check
python3 tools/layoutb2/chrome_check.py    # the calibration, needs Chromium
                                          # and the Ahem font
bash test.sh                        # section 61 among the rest
```

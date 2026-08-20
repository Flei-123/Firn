# Round 61 -- layout: from the computed style to the box with coordinates

The fourth large piece of the browser engine, after the HTML5 tokenizer
(rounds 40 to 51), the tree construction with the DOM core (round 54) and
the CSS path (round 60): **from the computed style per node to a rectangle
per box** -- the step that comes before painting.

Everything in this round is written in Firn. The proof runs against an
**independent implementation** -- the same cases are rendered by a real
Chromium in headless mode and compared box for box with
`getBoundingClientRect()`. An expectation written by the author of the
code proves only that the code does what the author thought; a foreign
engine proves that the thought was right.

---

## 1. The result in numbers

| Measurement | Result |
|---|---|
| own cases, box tree against the frozen expectation | **705 / 705** boxes in **91** cases |
| the same cases against **Chromium**, box for box | **705 / 705** equal, deviation **0.00 %** |
| the same quota in all three build stages | opt = --no-opt = dev-fast |
| hand-computed checks in `tests/940` to `tests/945` | **36** boxes, all correct |
| soak run, 60 s, 84,100 rounds | RSS growth **16 KiB**, counter check strikes (+90 MiB) |
| cross-check on `testdata/realweb/` (60,611 boxes) | **1.64 %** to the bit -- see section 7 |
| throughput on `testdata/realweb/` (callgrind) | **429,325** instructions per element, **5,842** per byte |

`tools/layout/run.sh` compares all three build stages and stops on a
difference.

---

## 2. What was built

```
lib/layout/box.fi           the box, the inline item, the fragment,
                            collapsed margins, the font metric,
                            the float context
lib/layout/build.fi         DOM + styles -> box tree (anonymous blocks)
lib/layout/flow.fi          the layout: block flow with margin collapsing,
                            inline flow with line boxes, floats,
                            position relative/absolute, flexbox
lib/layout/layout_main.fi   the driver: HTML + CSS -> one rectangle
                            per element
lib/layout/bench_main.fi    the measuring version without the report
lib/layout/soak_layout.fi   the endurance run

lib/css/cascade.fi          extended from 23 to 47 properties

tools/layout/make_font.py   builds the measuring font
tools/layout/chrome.py      measures a case with a real Chromium
tools/layout/harness.py     the own cases, both comparisons
tools/layout/realweb.py     the cross-check on real pages
tools/layout/gc_layout.sh   the soak run with a counter check
tools/layout/throughput.sh  instructions per element (callgrind)
tools/layout/run.sh         everything together
tools/layout/ua.css         the user agent stylesheet
tools/layout/cases/         91 cases, each with its frozen box tree
```

### 2.1 The box tree is not the DOM tree

Three differences decide everything that follows, and the third is the one
that is usually forgotten:

* `display: none` produces **no box**, and none for any descendant.
* A floated or absolutely positioned element is **blockified**: `float:
  left` on a `span` makes a block box out of it (CSS 2.1, 9.7).
* A block container with **both** block-level and inline-level children
  wraps every run of inline children in an **anonymous block box** (CSS
  2.1, 9.2.1.1). Without that step `<div>text<p>x</p>more</div>` is one
  pixel high and everything below it sits wrong. White space **between**
  two block-level boxes produces no box at all.

### 2.2 Margin collapsing -- the part everybody underestimates

Two adjoining margins do not add up; they become the **largest positive
plus the smallest negative** one. And "adjoining" reaches **through**
boxes: the top margin of a box is adjoining the top margin of its first
child as long as no border, no padding and no block formatting context
stands in between. The margin then **leaves** the box -- which is why in
`tools/layout/cases/12_collapse_parent.html` the `body` starts at y = 30
although nothing in it has a margin.

A box whose own two margins meet ("collapse through") disappears from the
flow while its margin stays. Two numbers have to be kept apart for it, and
that is the mistake that is easy to make:

* **where the box comes to stand** is decided by the margins ABOVE it
  alone (`Box.thr_pos` / `thr_neg`),
* **what goes on collapsing downwards** is the set of both its margins
  (`top_pos` / `bot_pos`).

In `tests/941` that is the difference between y = 62 and y = 90 for two
boxes that follow each other directly.

The three exits at which collapsing stops: a border or padding on the
side in question, a block formatting context root (float, absolute,
inline-block, flex, `overflow` other than `visible`, and the **root
element**), and a `clear` that really introduces clearance.

### 2.3 The block flow

The equation of CSS 2.1, 10.3.3 has to come out even:

```
margin-left + border-left + padding-left + width
            + padding-right + border-right + margin-right = containing block
```

Which of the unknowns gives way is decided by which of them is `auto` --
and if none is, the **right margin** gives way (in left-to-right text).
Two automatic margins share what is left, which is the classic way to
centre something.

`box-sizing: border-box` means the given width already contains padding
and border; subtracting them can go below zero, and the content width is
then zero and not negative. `min-width` and `max-width` measure the **same
box** `width` does -- with `border-box` therefore the border box, which is
off by exactly padding plus border if it is forgotten
(`tools/layout/cases/0a_box_sizing_percent.html`).

Percentages of margin **and of padding** refer to the **width** of the
containing block -- also the vertical ones. That is not a mistake in the
standard but the price of a layout that must not depend on a height it
does not know yet.

### 2.4 The inline flow

An inline formatting context is flattened into a **vector of items**
before a single line is broken: words, spaces, atomic inlines, and a
marker for opening and closing an inline box. That turns the line breaker
into a loop over an array instead of a walk over a tree with a resumable
state.

What matters and is easy to get wrong:

* **White space collapses across element borders.** `<span>A </span><span>
  B</span>` has ONE space between A and B, and it belongs to the FIRST
  span -- whose rectangle is therefore 40 px wide and not 20
  (`22_white_space`).
* **A line box without content is not generated at all.**
  `<div><span></span></div>` is zero pixels high; only an inline box with
  a ring of its own (margin, border, padding) makes a line
  (CSS 2.1, 9.4.2 -- `89_empty_inline`).
* **The content area of an inline box is ascent plus descent, not the line
  height.** Padding and border grow it outwards, and they do NOT make the
  line box taller (CSS 2.1, 10.6.1 -- `23_inline_boxes`).
* **An atomic inline is a break opportunity on both sides.** Three
  inline-blocks written without a space between them still fall onto three
  lines if they do not fit (`8d_inline_block_grid`).
* **The baseline of an inline-block** is the baseline of its last line box
  if it has one and `overflow` is `visible`; otherwise its bottom margin
  edge (CSS 2.1, 10.8.1 -- `80_ib_baseline_text`).
* **An inline box broken over three lines is three fragments**, and
  `getBoundingClientRect()` reports their union (`28_multi_line_span`).

### 2.5 Floats

A float lies in the coordinates of the **formatting context root**, and
the context is handed down to nested blocks with a shift, so that a line
box three levels deep still knows how much room is left.

The point that is usually got wrong: **block boxes go on running
underneath a float.** In `tests/943` the box of `#g` starts at x = 0 and
keeps the full 800 px although a 100 px float stands next to it; only the
**line boxes** give way, which is why the text of `#h` starts at x = 100
while its box starts at x = 0.

Two exceptions:

* A box that opens its own formatting context does not slide underneath a
  float -- it becomes **narrower** and stands next to it. Its own left
  margin is swallowed where the float pushes further than the margin would
  have (`88_bfc_beside_float`).
* The auto height of a formatting context root **reaches around** its
  floats (CSS 2.1, 10.6.7). That is the whole reason `overflow: hidden` on
  a container makes the floats inside it count again (`3b_float_contained`).

A word that does not fit next to a float moves the whole line **down** to
where there is room (`31_float_two_sides`).

### 2.6 Positioning and flexbox

`position: relative` moves the box and its subtree and leaves a hole.
`position: absolute` measures against the **padding box** of the nearest
positioned ancestor, resolves `left`/`right`/`width` after CSS 2.1, 10.3.7
and `top`/`bottom`/`height` after 10.6.4, and falls back to the **static
position** where both offsets are `auto`.

The flexbox is one axis, one line, `align-items: stretch`. The part that
is not decoration is the **loop** of css-flexbox-1, 9.7: distributing the
free space once and clamping afterwards LOSES the space a clamped item
gives back. With `max-width` on one of two growing items that is the
difference between 300 and 500 pixels -- measured against Chromium in
`56_flex_minmax` and checked by hand in `tests/944`.

---

## 3. The measuring font, and why there is one

A layout engine cannot be compared against a browser as long as the width
of a letter is unknown. The way out is the one the CSS working group has
been using since 1999: a font whose metrics are a round number.
`tools/layout/make_font.py` **builds** such a font, so that nothing has to
be downloaded and the numbers stand in the source instead of in a binary
blob:

```
units per em    1000
advance width   1000  for EVERY glyph   -> text width = n * font-size
ascent           800                    -> 0.8 em above the baseline
descent          200                    -> 0.2 em below the baseline
line gap           0                    -> `line-height: normal` = 1.0 em
```

All three metric sets of an OpenType file (`hhea`, the typo values of
`OS/2`, the win values of `OS/2`) carry the same numbers, so it makes no
difference which of them the browser reads.

**The rounding is not a detail.** A browser scales the metrics of a font
to the font size and rounds each one to a WHOLE pixel. `font-size: 23.4px`
therefore gives ascent 19 and descent 5 -- a line of 24 and not of 23.4.
Without that rounding every case with a font size that is not a whole
number is off, and `1.17em` (the `h3` of the user agent stylesheet) is
exactly such a case. The rule `round(0.8 * size) + round(0.2 * size)` was
measured against Chromium for thirteen font sizes between 3.3 and 40 px
and matches all thirteen.

**The layout grid.** Every used length is snapped to a sixty-fourth of a
pixel, downwards. That is a decision, not a rule of the standard: CSS says
nothing about precision, and every engine has such a grid -- Blink and
WebKit use 1/64 px, Gecko 1/60. Without a grid the same document computes
differently on different machines, and `1em` of `font-size: 23.4px`
becomes a number nobody can reproduce. It is also the reason the
cross-check comes out to the last digit instead of "almost".

---

## 4. The two proofs, and what each of them is worth

### 4.1 The frozen box tree (`tools/layout/cases/*.expected`)

Every case has a file with one line per element:

```
html         0.0000     0.0000   800.0000    45.0000
head         0.0000     0.0000     0.0000     0.0000
style        0.0000     0.0000     0.0000     0.0000
body         0.0000    30.0000   800.0000    15.0000
...
```

Text against text, no picture. **What this is honestly worth:** these
files were produced FROM the engine and afterwards confirmed box for box
against Chromium. They are a **regression lock**, not an independent
proof -- they catch the day a change moves a box, without a browser having
to be installed. The independent proof is the browser, and it is run
separately.

### 4.2 The hand-computed checks (`tests/940` to `tests/945`)

Where the hand-derivation really lives. Six test programs in Firn, part of
`test.sh`, with **36 rectangles whose numbers were computed with a pencil
from CSS 2.1 and css-flexbox-1** and only afterwards held once against
Chromium, so that no arithmetic slip of the author is frozen into a test:

* `940` box model and block flow -- escaping margin, `border-box`, two
  automatic margins, `max-width` and `min-height`
* `941` margin collapsing -- adjoining, collapsing through, a negative
  margin that shortens the root element
* `942` inline flow -- line breaking at the exact pixel, `line-height`,
  the content area of an inline box
* `943` floats and positioning -- block boxes under the float, line boxes
  beside it, relative and absolute
* `944` flexbox -- `flex: 1 1 0`, and the loop that gives back what a
  clamped item hands over
* `945` the box tree is collected, with a counter check that must fail

### 4.3 The cross-check against Chromium

`tools/layout/chrome.py` copies the case into a temporary directory
together with the measuring font, appends a measuring script and runs
`chrome-headless-shell --dump-dom`. The script walks the elements in
document order -- the same order the Firn driver uses -- and calls
`getBoundingClientRect()` on each. `getBoundingClientRect()` returns the
BORDER box in viewport coordinates, which is exactly what the Firn driver
prints, so the two can be subtracted without a conversion.

Two things the harness has to get right, and both cost a wrong measurement
if they are missed:

* **Wait for the font.** Measuring before `@font-face` has loaded compares
  the engine against a FALLBACK font; the whole cross-check is then
  worthless and looks like a bug in the engine. The script waits for
  `document.fonts.ready`.
* **Standards mode.** Without a `<!DOCTYPE html>` a browser lays out in
  quirks mode, and the root element then stretches to the viewport. Firn
  does not implement quirks mode (section 6); every case carries a
  doctype.

**Result: 705 of 705 boxes in 91 cases equal, deviation 0.00 %.** The
tolerance is one sixty-fourth of a pixel -- the layout unit itself, so
nothing can hide in it.

---

## 5. The cases

91 files in `tools/layout/cases/`, grouped by subject:

| group | cases | subject |
|---|---|---|
| `01` .. `0a` | 10 | box model, widths, percentages, borders per side |
| `10` .. `1b` | 11 | margin collapsing in every shape |
| `20` .. `2g` | 17 | inline flow, white space, line height, alignment |
| `30` .. `3b` | 12 | floats, `clear`, formatting contexts |
| `40` .. `48` | 9 | `position: relative` and `absolute` |
| `50` .. `57` | 8 | flexbox |
| `60` .. `62` | 3 | the user agent stylesheet |
| `70` .. `73` | 4 | mixed documents, deep nesting, empty boxes |
| `80` .. `8f` | 16 | the cases meant to hurt |

The last group is the one that found the bugs: the empty line box, the
break opportunity around an atomic inline, and the width of a formatting
context root next to a float were all wrong until `89`, `8d` and `88`
existed.

---

## 6. What is honestly missing

Named, not hidden. Everything here is a **deliberate gap**, not an
oversight:

1. **Tables.** `display: table` is parsed and treated as a block. There is
   no table algorithm -- no column widths, no row heights, no
   `border-collapse`. That is a round of its own.
2. **Replaced elements.** `img`, `input`, `button`, `select` have no
   intrinsic size: no image is decoded and the presentational attributes
   `width=` and `height=` are not mapped to properties. They come out as
   0 x 0.
3. **Real fonts.** One metric (advance 1 em, ascent 0.8, descent 0.2), no
   font file is read, no shaping, no kerning, no ligatures. Everything
   about line breaking that is not "break at a space" is missing: no
   UAX 14, no hyphenation, no `word-break`, no East Asian rules.
4. **Quirks mode.** The document mode is in the DOM (round 54) and is not
   used. Every case runs in standards mode.
5. **Block-in-inline.** A block-level box inside an inline box has to
   split the inline box into three parts. Instead the inline box is
   promoted to a block box -- a bounded, deterministic and named
   deviation.
6. **Flexbox** is one line and one axis. `flex-wrap` is parsed and
   ignored, `flex-direction: column` falls back to `row`,
   `justify-content` and `align-items` other than `stretch` do not exist.
   Grid does not exist at all.
7. **Floats in an inline formatting context** are placed at the top of the
   line on which they appear; a float that does not fit there does not
   push the following line further than the algorithm of CSS 2.1, 9.5.1
   would. The available width of a line is probed with the STRUT height,
   so a line that turns out taller than its strut can have been measured
   against too much room next to a float.
8. **`text-align: justify`** is parsed and behaves like `left`;
   `vertical-align` with a length or a percentage is not implemented.
9. **`position: fixed`** is treated like `absolute` against the initial
   containing block; `position: sticky` like `static`.
10. **No incremental layout.** Every run lays out the whole document.
    There is no dirty-bit, no reflow of a subtree.

---

## 7. The cross-check on real pages -- the number that hurts

The eight documents in `testdata/realweb/` were not written by the author
of the engine. Both sides get the same preparation (scripts and external
stylesheets removed, no request leaves the machine, the measuring font
forced on everything), so what is compared is the layout and nothing else.

```
                              boxes   to the bit   deviation
hackernews.html                 814            5     99.39 %
rustdoc_vec.html              16064          619     96.15 %
w3c_html52.html                4457            8     99.82 %
whatwg_parsing.html           13406            8     99.94 %
wikipedia_de_html.html         2723          125     95.41 %
wikipedia_en_linux.html        7993          110     98.62 %
wikipedia_en_rust.html         9237           66     99.29 %
wikipedia_en_www.html          5917           50     99.15 %

REALWEB 991 / 60611 boxes to the bit, deviation 98.36 %
   median 1239.69 px, 90th percentile 2748.02 px
```

**98.36 % off, and that is the honest number.** It is reported because the
gap should be a measurement and not an opinion. Where it comes from is not
a mystery; it was looked up box by box on `wikipedia_en_linux.html`:

* `input`, `img` and `button` come out as 0 x 0 (gap 2 above). On a page
  where every second paragraph carries an image, the first such box moves
  everything below it.
* `hackernews.html` is a table layout from top to bottom (gap 1). 99.4 %
  off is the expected answer for an engine without tables.
* The errors **accumulate downwards**. On a document of 19,000 pixels one
  box that is eight pixels too short at the top makes every box below it
  eight pixels wrong. The median of 1,240 px says how long the documents
  are, not how badly a single box is computed: on
  `wikipedia_de_html.html` the total height of the document differs by 39
  px out of 19,014 -- 0.2 %.

The right conclusion is not "the layout is wrong" and not "the layout is
right", but: **the layout is correct for what it implements, and what it
does not implement is what real pages are made of.** Tables and replaced
elements are the next two rounds, in that order.

---

## 8. Throughput, in instructions

Not with the wall clock: the run time of one and the same binary scatters
by more than ten per cent between runs on this machine.
`valgrind --tool=callgrind` counts the instructions really executed.
Measured per page in two modes -- `HTML -> DOM -> cascade` (the state
after round 60) and the same plus box tree and layout; the difference is
the share of round 61.

```
PAGE                            BYTES   ELEMENTS    CASCADE(Ir)       FULL(Ir) LAYOUT/ELEM LAYOUT/BYTE
-------------------------------------------------------------------------------------------------
hackernews.html                 34189        814      105250421      132357460       33301       792
rustdoc_vec.html               920065      16064     3119679929    15052535676      742832     12969
w3c_html52.html                153766       4457      532971845      932606882       89664      2598
whatwg_parsing.html            773933      13406     2690344332    10447136856      578606     10022
wikipedia_de_html.html         256577       2723      867113977     1141825091      100885      1070
wikipedia_en_linux.html        807054       7993    10341344185    12139268436      224937      2227
wikipedia_en_rust.html         878182       9237     9924532376    12419422916      270097      2840
wikipedia_en_www.html          629872       5917     6792173339     8130074701      226111      2124
-------------------------------------------------------------------------------------------------
TOTAL LAYOUT share: 26021817614 instructions for 60611 elements (123039 boxes) in 4453638 bytes
                    429325 instructions per element, 5842 per byte of the page
LAYOUT_PER_ELEMENT 429325
LAYOUT_PER_BYTE 5842
```

The layout costs **429,325 instructions per element** and **5,842 per byte
of the page**; on the whole corpus that is roughly two and a half times
what the cascade of round 60 costs. Both numbers are honest and both are
too high -- the reason follows.


### 8.1 The finding that matters more than the number

The layout scales **quadratically** with the size of the document, and so
does the cascade of round 60. Measured by multiplying the body of
`wikipedia_de_html.html` one, two, four and eight times:

```
            elements   time      per element   GC runs   live objects
x1              2723    236 ms      86.7 us          4         52,598
x2              5428    679 ms     125.0 us          6        103,082
x4             10838   2667 ms     246.0 us         10        211,186
x8             21658  16642 ms     768.4 us         17        442,864
```

Eight times the elements cost about seventy times the time. The cause is
not in `lib/layout/`: the same measurement with the layout switched off
(mode 0) gives 57.9 / 78.3 / 120.3 / 250.6 us per element -- also
quadratic. **The collector marks a heap that grows with the document, and
it does so a number of times that also grows with the document**: runs x
live objects goes from 210,000 to 7,500,000, a factor of 36 for a factor
of 8 in size.

The layout roughly **doubles** the number of GC objects per element (15.4
before, 28.2 after), and because the cost is quadratic in that number,
that alone is a factor of four. Two things were done about it in this
round, and both are small:

* the **item list of an inline formatting context is not kept**: after the
  last line it is working material and is dropped, instead of hanging on
  the box for the life of the document;
* the two vectors of a **float context are made only when a float really
  turns up** -- most formatting contexts never see one.

What would really help is a **generational or incremental collector**, or
an arena for the inline items so that a word does not cost an object. That
is a round of its own and belongs to the GC, not to the layout.

---

## 9. Language gaps found

Written down instead of fixed -- three other rounds work on the compiler
in parallel.

1. **`const` with a floating point type is rejected.**
   `const NO_FLOAT: f64 = 0.0` gives
   *"'const' supports only integer and bool types in stage 0"*.
   Workaround: a `fn` that returns the value, or a local `let`.

2. **An error union with an `f64` payload delivers a WRONG value.** This
   one cost half a day, because it fails silently:

   ```firn
   error AllocError { OutOfMemory }
   fn give() -> AllocError!f64 { return 6.0 }
   fn main() -> i32 {
       let a: f64 = give() catch 0.0
       if a != 6.0 { return 1 }     // TAKEN
       return 0
   }
   ```

   The same program with `u64`, `bool` or a `Gc[T]` payload is correct,
   and a plain `fn give() -> f64` is correct too. Only the error union
   with a float loses the value, in `release-fast` and in `--no-opt`
   alike. Workaround, used everywhere in `lib/layout/flow.fi`: the length
   leaves through an out-pointer and the function returns
   `AllocError!bool`.

3. **A `struct` cannot be the payload of an error union.**
   `fn floatctx_new() -> AllocError!FloatCtx` gives *"unknown type
   'FloatCtx'"* at the point of the definition. Workaround: the pattern
   the code base already uses -- `new()` makes the empty shell, `init()`
   fills it and returns `AllocError!bool`.

4. **A struct type from another module needs the module prefix, a `gc
   class` does not.** `*mut FloatCtx` is an unknown type in a foreign
   module, `*mut box.FloatCtx` is right; `Gc[Box]` works without a prefix.
   Not a bug, but an asymmetry that costs a compile error every time.

5. **Two statements on one line need a `;`.**
   `{ *out = X return true }` is a parse error, `{ *out = X; return true }`
   is right. Consistent, only unusual for a language that otherwise has no
   semicolons.

---

## 10. The acceptance, run in full

```
bash test.sh                  PASS 924/924        (base 905/905 + 6 tests
                                                   x 3 build stages + 1 section)
bash tools/layout/run.sh      705/705 own boxes, 705/705 equal to Chromium,
                              deviation 0.00 %, three build stages equal,
                              soak 84,100 rounds in 60 s, RSS growth 16 KiB,
                              counter check +90,076 KiB
bash tools/self_compare.sh    SAME BEHAVIOUR 252, DIFFERING 0, FAULTY 0
bash tools/english/check.sh   0 / 0 / 0 / 0 / 0
```

---

## 11. How to run it

```bash
export FIRNLIB=$PWD/lib
bash tools/layout/run.sh              # everything: cases, Chromium, soak
bash tools/layout/run.sh --fast       # without the three build stages
bash tools/layout/throughput.sh       # instructions per element
bash tools/layout/gc_layout.sh        # the soak run on its own
python3 tools/layout/realweb.py .layout-work/layout   # the real pages
```

A single case, engine against browser:

```bash
python3 tools/layout/harness.py .layout-work/layout --only 88_bfc
```

`tools/layout/run.sh` hangs in `test.sh` as section 23.

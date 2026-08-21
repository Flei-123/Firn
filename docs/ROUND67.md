# Round 67 -- what round 61 did not get to: positioning, floats, flexbox

Round 61 built the layout: the box model with margin collapsing, the
block flow and the inline flow with line boxes, measured against Chromium
and coming out at 705 of 705 boxes. It named nine things it had NOT done,
and this round does the three that carry the most weight, plus the fourth
group that was written down as "if there is room":

1. **`position`** -- `relative`, `absolute`, `fixed`, `sticky`, the
   containing block, `z-index` and the **paint order** with stacking
   contexts.
2. **`float` and `clear`** -- the displacement of line boxes at the right
   height, block formatting contexts, and the auto height of a formatting
   context root with floats in it.
3. **Flexbox** after css-flexbox-1, in full: both axes, `flex-grow` /
   `shrink` / `basis`, line breaking, `justify-content`, `align-items` /
   `align-self` / `align-content`, `order`, automatic margins and the
   automatic minimum size.
4. `overflow`, `box-sizing` and **percentage heights**.

Everything is written in Firn. The proof is the same one as in round 61
and not a softer one: the same cases go through a real Chromium in
headless mode and are compared box for box with
`getBoundingClientRect()`.

---

## 1. The result in numbers

| Measurement | Round 61 | Round 67 |
|---|---|---|
| own cases against **Chromium**, box for box | 705 / 705 | **1087 / 1087** |
| deviation from Chromium | 0.00 % | **0.00 %** |
| cases | 91 | **146** |
| **paint order** against `document.elementFromPoint` | -- | **5171 / 5171** probe points in 15 cases, **0.00 %** |
| own cases against the frozen expectation | 705 / 705 | **1087 / 1087** |
| the same quota in all three build stages | yes | **yes** (opt = --no-opt = dev-fast) |
| hand-computed checks in `tests/` | 36 boxes | **+ 5 programs, 33 checks** (`tests/1180`..`1184`) |
| `bash test.sh` | 961 / 961 | **974 / 976** -- see 1.1 |
| soak run, RSS growth | 16 KiB | **16 KiB** in 15,100 rounds (counter check strikes: +62,480 KiB) |
| cross-check on `testdata/realweb/` (60,611 boxes) | 991 to the bit | **994 to the bit**, deviation 98.36 % |
| throughput (callgrind, instructions per element) | 429,325 | **455,890** (+6.2 %) |

The number that is NOT in this table is a wall clock. On this machine the
run time of one and the same binary scatters by more than ten per cent
between runs; `valgrind --tool=callgrind` counts the instructions really
executed and is reproducible down to the single instruction.

### 1.1 The lines of `test.sh` that are not green

`test.sh` counts **976** checks in this tree (961 before, plus the five
new test programs times three build stages). Two runs were made; the
better one ends at **974 / 976**, the other at 971 / 976. Nothing about
them is hidden:

* **`tools/english/check.sh`** reports `0 0 0 0 828`. The first four
  checks -- identifiers, output texts, byte-field lengths, comments -- are
  clean, and no hit lies in a file this round wrote. The fifth walks the
  PATH NAMES of everything git tracks, and all 828 hits lie in
  `.js-work/t262/`: a checkout of test262, 32,962 files, committed by
  accident in the base commit `aa65fdc` itself (`git log -- .js-work`
  names it). The check fails exactly the same way on `main`, before a
  line of this round was written. The fix is one line --
  `git rm -r --cached .js-work` plus a `.gitignore` entry -- in a place
  this round was told not to touch.
* **`tests/860_thread_basic.fi`** and **`tests/834_arc_thread.fi`** are
  FLAKY on this machine, and it is not a difference between the two
  compilers. Six runs of the very same binary, alone, with nothing else
  on the machine:

  ```
  860_thread_basic   14 14 14  0  0 14
  834_arc_thread      9  9  0  9  0  0
  860 built from MAIN 0 14 14  0  0 14      <- unchanged tree, same picture
  ```

  They are rounds 47 and 49, not round 67, and the last line is the proof
  that this round did not cause them.
* **`tools/fixpoint.sh`** fails for the same reason and for no other: the
  fixpoint itself is reached (`stage 2 == stage 3, character-identical`,
  573,568 lines of assembly), and the corpus comparison names
  `tests/834_arc_thread.fi (firnc0: 9, firnc1: 0)` as the single
  deviation. Run on its own, when the thread test happens to come out the
  other way, it ends at **264 same behaviour, 0 differing, 0 faulty**.

`tools/self_compare.sh` is green in every run: **264 same behaviour, 0
differing, 0 faulty**.

Section 23 of `test.sh` -- the one this round is about -- reads:

```
LAYOUT OK: 1087 / 1087 own boxes, 1087 / 1087 equal to Chromium
           (deviation 0.00 %), paint order 5171 / 5171
```

and section 24, the formatter, confirms that every source of this round
is in canonical shape.

---

## 2. What was built

```
lib/layout/stack.fi        NEW -- the paint order: stacking contexts and
                           the seven steps of CSS 2.1, Appendix E
lib/layout/flow.fi         `position: fixed` and `sticky`, the initial
                           containing block, the line probe at the right
                           height, the float records that follow an
                           escaping margin, percentage heights through the
                           whole flow, and the flexbox rewritten in full
lib/layout/box.fi          the float marks, four new box flags
lib/layout/build.fi        blockification of flex items, the flex item
                           flag, the "depends on my height" bit
lib/css/cascade.fi         six new properties: `z-index`, `order`,
                           `justify-content`, `align-items`,
                           `align-self`, `align-content`; the shorthands
                           `flex-flow`, `place-content`, `place-items`,
                           `place-self`; `min-width`/`min-height` initial
                           value put right (`auto`, not zero)

tools/layout/stack.py      NEW -- the paint order against Chromium
tools/layout/stackcases/   NEW -- 15 cases for the paint order
tools/layout/cases/        55 new cases (91 -> 146)
tools/layout/run.sh        the paint order hangs in as step 3a
tests/1180..1184           five hand-computed test programs
```

---

## 3. `position`, and the three things that are easy to believe wrongly

### 3.1 `fixed` measures against the WINDOW, and nothing changes that

`position: absolute` looks for the nearest positioned ancestor.
`position: fixed` does **not**: its containing block is the viewport, and
a `position: relative` around it makes no difference at all (CSS 2.1,
10.1.3). Round 61 treated `fixed` like `absolute`, which is right exactly
as long as no positioned ancestor stands in between --
`tools/layout/cases/a0_fixed_offsets.html` is the case where it is not:
the box sits at (10, 20) although its parent stands at (30, 440).

### 3.2 The initial containing block is not the root box

Both are 800 pixels wide, and that is why the difference could hide for a
whole round: it is the **height** where the two part company. The initial
containing block is as tall as the **window**, the root box is as tall as
the **document**. A box with `position: absolute; bottom: 0` in a
document of 100 px therefore sits at y = 540 in a window of 600 -- and
round 61 put it at y = 40 (`a4_abs_icb`).

The same for `height: 50%` on an absolutely positioned box: 300, not
nothing.

### 3.3 `sticky` pulls back, it never pushes out

The sentence that everybody gets wrong: **`bottom: 0` does not push a box
to the bottom of the window.** A sticky box stays exactly where the flow
put it until the scrollport would leave it behind, and then it is pulled
BACK into view -- and never further than the edge of its containing
block. Measured against Chromium:

* a 50 px box at y = 0 in a 900 px document with `bottom: 0` **stays at
  y = 0**,
* the same box at y = 700 moves **up** to 550, because 750 would be past
  the 600 px window,
* `top: 100px` on a box in a containing block only 30 px tall moves it to
  y = 10 and no further -- the containing block wins (`a6_sticky_clamped`).

Two more things cost a measurement each:

* **The rectangle that is held in place is the BORDER box**, not the
  margin box. With `margin-top: 20px` under a `top: 60px` the box comes
  out at y = 60, not at y = 80 (`a9_sticky_margin`).
* **The margins that take the containing block in are the SPECIFIED
  ones**, not the used ones. A block with `width: 100px` in a 400 px
  containing block gets a used `margin-right` of 300 out of the
  over-constraint rule of CSS 2.1, 10.3.3 -- and taking the constraint
  rectangle in by that leaves the sticky box exactly no room to move.
  Every horizontal case came out unmoved until Chromium said otherwise
  (`a8_sticky_left`).

The scroll offset is zero here and the scrollport is the window: this
engine renders one still picture. Named gap, section 8.

---

## 4. The paint order -- and how you prove one without a picture

A layout can be proven with rectangles. A paint order cannot: there is no
`getPaintOrder()` in any browser, and a rectangle says nothing about who
lies on top of whom.

But there is one question whose answer **is** the paint order, and every
browser answers it:

```js
document.elementFromPoint(x, y)   // the topmost element at this point
```

So `lib/layout/stack.fi` prints the element numbers back to front,
`tools/layout/stack.py` reads that list **from the back** to find the
topmost box at a point, asks Chromium the same thing at the same points,
and compares. The points are not a blind grid -- a grid of 25 px would
mostly hit places where nothing overlaps and would prove nothing. Every
box contributes its own centre and four points just inside its corners,
exactly where an overlap is decided, and a coarse grid is added for the
empty places.

**Result: 5171 of 5171 probe points equal, deviation 0.00 %, in 15
cases.**

### 4.1 The rule that surprises everybody

A stacking context is an **atom**. Everything inside it is painted
together and nothing from outside can slip in between -- which is why a
`z-index: 999` deep inside a `z-index: 1` box can never come out over a
sibling with `z-index: 2`, no matter how large the number is
(`13_zindex_trapped`). In the code that is one line: `paint_context` never
descends into a nested context, it calls itself on it.

Two neighbours of that rule are in the cases because they are the ones
that get mixed up:

* `z-index: auto` does **not** make a stacking context, `z-index: 0`
  does. Both land in the same step 6, so the difference is invisible
  until something inside the box has a z-index of its own -- and then the
  one escapes and the other does not (`14_zindex_auto_escapes`,
  `1d_zero_vs_auto`).
* `position: fixed` and `position: sticky` **always** make a stacking
  context, with or without a z-index (`19_fixed_context`,
  `1a_sticky_context`), and so does a **flex item** with a z-index,
  although it is not positioned at all (`1b_flex_item_zindex`).

### 4.2 The seven steps, and what each of them is for

1. the background and borders of the element that forms the context
2. child stacking contexts with a **negative** z-index, most negative first
3. the in-flow, **non-inline-level**, non-positioned descendants
4. the non-positioned **floats**
5. the in-flow, **inline-level**, non-positioned descendants
6. the positioned descendants with `z-index: auto` or `0`, in tree order
7. child stacking contexts with a **positive** z-index, ascending

Steps 3 and 5 are the reason a float lies under the text of the paragraph
beside it but over the background of the block it sits in. Step 6 is the
reason a positioned box with no z-index at all still covers every
ordinary block (`15_positioned_over_block`).

The step that is easy to miss is E.2, step 8: a positioned box with
`z-index: auto` and a float are painted as ONE piece, but their
**positioned descendants belong to the parent context**. Without that,
a `position: absolute` inside a float disappears under a later sibling
(`18_abs_in_float`).

---

## 5. Floats: asking for the room at the RIGHT height

Round 61 named this as gap 7 and it cost a real number.

The room beside a float is asked for **at a height**, and the height of a
line is not known before the line is full. Round 61 asked with the STRUT
-- the line cannot be shorter than that -- and stopped there. A line that
turns out taller and reaches into a float the strut did not touch was
therefore measured against too much room.

The fix is to cut the knot the other way round: fill the line, **measure
what it would be**, ask the floats again with that height, and fill it
once more if the answer changed. Two rounds are enough in practice and
the third is refused on purpose, so that a line which grows on every
round cannot loop.

In `tools/layout/cases/ab_float_line_probe.html`: a 20 px float, a second
float cleared below it, and a 60 px word in a 20 px paragraph. With the
strut only the first float is in the way and the text starts at x = 50.
The line really is 60 px tall, the second float is in the way too, and
the answer is **x = 100** -- which is what Chromium says.

### 5.1 Two more things floats got wrong

* **A block whose only child is a float collapses through.** The rule is
  "no LINE BOXES", not "no children" (CSS 2.1, 8.3.1), and a float is not
  in-flow content. Until this round the engine looked at the number of
  children, and a negative margin after such a box did not escape where
  Chromium let it escape (`ad_float_collapse_through`).
* **A float record has to follow a margin that escapes.** A float is
  placed at the y the layout ESTIMATES for the block it stands in -- it
  has to be, because the float decides how wide the lines of that block
  may be. The estimate is wrong in exactly one case: when a margin
  escapes upwards afterwards and takes the whole block with it. The float
  BOX is put right by the placement pass; only its record in the float
  context stayed behind -- and that record is what the auto height of the
  formatting context root is read from. A root element came out 100 px
  tall instead of 150 (`ac_float_escape`).

---

## 6. Flexbox, in full

The flexbox of round 61 was one axis, one line, `align-items: stretch`.
This one is css-flexbox-1, section 9, and the piece of design that keeps
it from being two implementations is this: **the algorithm never says
"width" or "height", it says MAIN and CROSS.** Which physical axis each
of them is depends on `flex-direction` alone, so every access to a size, a
margin or an edge goes through one of the `ax_...` helpers with a
`vertical` flag. `flex-direction: column` is then not a second code path
but the same code with the flag turned around, and `row-reverse` is the
finished positions mirrored once at the end.

What was found by measuring, in the order it hurt:

* **The resolution loop (9.7) has to repeat.** Handing the free space out
  once and clamping afterwards loses what a clamped item gives back --
  that was already right in round 61 and stays right.
* **An unfrozen item starts every round again from its FLEX BASE SIZE**,
  not from what it had in the round before. And 9.7.6: the sign of the
  TOTAL violation decides which items are frozen, not the sign of each
  single one.
* **9.7.4b**: if the flex factors of the unfrozen items add up to less
  than one, only that fraction of the initial free space may be handed
  out. Without it `flex-grow: 0.25` on the only growing item eats
  everything instead of a quarter (`bj_flex_grow_fraction`).
* **`min-width: auto` on a flex item is not zero.** It is the automatic
  minimum size (4.5): the smaller of the content-based minimum and a
  definite size property. It is the reason a long word in a flex item
  does not shrink to nothing (`bl_flex_min_auto`), and `overflow: hidden`
  is the usual way out of it (`bm_flex_min_auto_off`).
  Two things had to be put right for it:
  * the **initial value** of `min-width`/`min-height` is `auto`, not zero
    (css-sizing-3, 5.1). Outside a flex container the two mean the same
    thing, which is why the difference stayed invisible until now.
  * the content-based minimum has to be computed with the box's own
    `width` **ignored**. An item with `width: 400px` in a 200 px flex
    container has to be allowed to shrink to its longest word, and asking
    the ordinary min-content width would hand back the 400 px it was told
    to ignore (`bn_flex_min_specified`).
* **An automatic margin in the main axis eats ALL the positive free
  space** before `justify-content` gets to see any of it (9.6), and an
  automatic margin in the cross axis beats `align-self` the same way
  (`bh_flex_auto_margin`, `bi_flex_auto_margin_cross`).
* **A single-line container with a definite cross size hands that size
  straight to its one line** (9.4). Only because of that does
  `align-items: stretch` make an item as tall as the CONTAINER instead of
  as tall as the tallest item.
* **`display: inline-flex` is a flex container whose box kind says
  "atomic inline".** The kind alone does not tell the two apart, so the
  box carries a flag; without it the children of an inline-flex never see
  the flex algorithm at all (`bt_flex_inline_flex`).
* **A flex item is blockified** (css-display-3, 2.7) and **always
  establishes its own formatting context**. A `span` in a flex row
  behaves like a `div`, and no margin escapes out of the first item.

### 6.1 Where the running position is snapped, and where it is not

`space-around` with three items divides by three. Chromium accumulates
the running position in double precision and lands it on the layout grid
of 1/64 px **once**, at the end. Snapping every step instead is off by a
whole layout unit on the third item -- 293.328125 against 293.34375. The
engine therefore keeps the running position unsnapped and snaps what
comes out of it (`b2_flex_justify_around`).

---

## 7. Percentage heights, and why they have to be handed down

`height: 50%` resolves against the height of the containing block **if
that is definite**, and is `auto` otherwise (CSS 2.1, 10.5). "Definite"
is not a property of the box that asks -- it is a property of the box
above it, and it can only be known by handing it down through the flow
the same way the width is handed down. That is one more parameter pair
(`cb_h`, `cb_h_ok`) through `layout_block`, `layout_content` and
`layout_block_children`, and it is the whole of section 4 of the round:

* a chain of percentages resolves as far as the definite height reaches
  (`c1_percent_height_chain`),
* against an automatic height it is nothing at all
  (`c2_percent_height_auto`),
* with `box-sizing: border-box` the containing block is the **content**
  box: 400 px with 2 x 10 padding and 2 x 5 border give 370, not 400
  (`c6_border_box_percent_height`),
* percentage `min-height` / `max-height` and a percentage `top` on a
  relatively positioned box resolve the same way
  (`c4_percent_minmax`, `c5_percent_relative_top`).

---

## 8. What is honestly missing

Named, not hidden. The list of round 61 shrinks by three entries and
grows by five new ones.

**Closed in this round:** the line probe at the wrong height (was 7),
`position: fixed` treated like `absolute` and `sticky` like `static`
(was 9), flexbox in one axis and one line (was 6).

**Still open from round 61**, unchanged: tables (1), replaced elements
with an intrinsic size (2), real fonts and real line breaking (3), quirks
mode (4), block-in-inline (5), `text-align: justify` and a
`vertical-align` with a length (8), incremental layout (10).

**New, and each of them a deliberate stop:**

11. **`position: sticky` measures against the WINDOW**, not against the
    nearest scroll container. A sticky box inside an `overflow: scroll`
    box is therefore constrained by the viewport instead of by that box.
    The scroll offset is zero: this engine renders one still picture and
    has nothing to scroll.
12. **The stacking context is made by `position`, `z-index` and flex
    items only.** `opacity` below one, `transform`, `filter`,
    `mix-blend-mode`, `isolation` and `will-change` also make one -- and
    none of the six is parsed by this engine, so none of them can.
    `transform` would also take the containing block of a `fixed` box
    away from the viewport; that is the same gap seen from the other side.
13. **`align-items: baseline` in a COLUMN container falls back to
    `flex-start`.** A baseline across a horizontal cross axis needs the
    first baseline of a whole line of items, which is a different
    computation from the one that exists.
14. **`flex-wrap: wrap` in a COLUMN container sizes items with `width:
    auto` by fit-content and stretches them to the line afterwards**,
    instead of measuring their content against the final line width. For
    `nowrap` -- the common case -- the line cross size IS the container
    width and the question does not arise.
15. **The hit test of `stack.hit_test` works on border boxes.** A browser
    hit-tests the fragments of an inline box and the actual painted area;
    for the block boxes the paint order is checked with, the two are the
    same, and the 15 cases are written with that in mind.

---

## 9. The cross-check on real pages -- the number that still hurts

The eight documents in `testdata/realweb/` were not written by the author
of the engine. Both sides get the same preparation, so what is compared
is the layout and nothing else.

```
                          round 61            round 67
                     exact   median px    exact   median px
hackernews.html          5      182.00        5      182.00
rustdoc_vec.html       619     1771.14      619     2575.34
w3c_html52.html          8     2521.00        8     2521.00
whatwg_parsing.html      8     1442.00        8     1442.00
wikipedia_de_html      125      238.34      128      222.98
wikipedia_en_linux     110     1417.03      110     1638.83
wikipedia_en_rust       66      809.12       66      756.00
wikipedia_en_www        50     1969.52       50     2479.95

REALWEB                991 / 60611        994 / 60611   (98.36 % off)
within  16 px         1673                1909
within  64 px         3383                4284
```

Three boxes more to the bit, and **236 more boxes within 16 px and 901
more within 64 px** -- but the MEDIAN error grew, from 1240 px to 1472 px.
Both numbers are reported because both are true, and the second one is
not a contradiction of the first: more boxes land near the truth, and the
tail of the ones that do not got longer. `rustdoc_vec.html` is where it
moves most, and that page is built out of flex containers -- their layout
really did change, and on a page that still has no tables and no images
the errors below them move with it.

The right conclusion is the one from round 61, unchanged: **the layout is
correct for what it implements, and what real pages are made of is what
it does not implement.** Tables and replaced elements are the next two
rounds, in that order, and no amount of work on flexbox will move this
number until they exist.

---

## 10. Throughput

```
PAGE                            BYTES   ELEMENTS    CASCADE(Ir)       FULL(Ir) LAYOUT/ELEM LAYOUT/BYTE
-------------------------------------------------------------------------------------------------
hackernews.html                 34189        814      105708339      135647376       36780       875
rustdoc_vec.html               920065      16064     3129153243    15791522216      788245     13762
w3c_html52.html                153766       4457      535607506      972694713       98067      2842
whatwg_parsing.html            773933      13406     2699707922    11048652100      622776     10787
wikipedia_de_html.html         256577       2723      867595861     1159176654      107080      1136
wikipedia_en_linux.html        807054       7993    10345334601    12194815430      231387      2291
wikipedia_en_rust.html         878182       9237     9932766259    12558681555      284282      2990
wikipedia_en_www.html          629872       5917     6794235817     8180886127      234350      2201
-------------------------------------------------------------------------------------------------
TOTAL LAYOUT share: 27631966623 instructions for 60611 elements (123038 boxes) in 4453638 bytes
                    455890 instructions per element, 6204 per byte of the page
LAYOUT_PER_ELEMENT 455890
LAYOUT_PER_BYTE 6204
```

The layout costs **455,890 instructions per element** and **6,204 per byte
of the page**. Round 61 measured 429,325 and 5,842 on the same corpus with
the same tool: **6.2 % more**, and the number moves in the wrong
direction on purpose. The flexbox of round 61 was one axis, one line and
one alignment; this one breaks lines, works in both axes and computes an
automatic minimum size -- and that last one needs the **min-content width
of every item**, which is a walk over the item's whole subtree. On
`rustdoc_vec.html`, a page built out of nested flex containers, the share
goes from 742,832 to 788,245 instructions per element; on
`hackernews.html`, which has no flex at all, from 33,301 to 36,780.

Two things were tried against it and both are in the code, honestly
labelled, because neither paid off on this corpus:

* **`F_H_DEPENDS`** -- a bit decided once while the box tree is built,
  saying whether anything under a box cares what its content height turns
  out to be. A stretched flex item is only laid out a second time when
  the answer is yes. It is the right rule and it saves a whole second
  pass wherever it strikes; on this corpus it does not strike often
  enough to be seen.
* **one intrinsic walk instead of two** -- the flex base size wants the
  maximum, the automatic minimum size wants the minimum, and both come
  out of the same walk, done on demand and cached on the item.

Measured before and after the two: 451,268 and 455,890 instructions per
element. The difference is a percent of book-keeping, and it is reported
rather than tuned away, because a number that is only true after picking
the better of two runs is not a measurement.

**The finding of round 61 stands unchanged and is the one that matters
more than this number:** the layout scales quadratically with the size of
the document, and the cause is not in `lib/layout/` -- the cascade of
round 60 does the same. Nothing in this round made that better or worse.

The 66,409 instructions per element that the brief for this round quotes
is not this measurement: it is the **cascade** of round 60
(docs/ROUND60.md, section 1), the path `HTML -> DOM -> cascade` without
any layout at all. The number the layout is measured by is the one round
61 reports, and it is the one carried forward here.

---

## 11. Language gaps

The compiler was not touched. What got in the way is written down here
instead.

**Gap A -- `E!f64` still delivers a wrong value, and it is still silent.**
Known since round 63 (docs/ROUND63.md, gap 2) and named again in round 61
for `lib/layout/flow.fi`: an error union over a floating point success
type compiles, runs, and hands back a number that is not the one that
went in. It cost half a day again in this round, because the symptom is
not a crash: every flex item simply came out **zero pixels wide**, and
the three functions that returned `AllocError!f64` looked perfectly
correct. Every length that crosses a `try` in `lib/layout/` therefore
leaves through a pointer, and the three new ones
(`flex_base_size`, `flex_min_main`, `flex_content_main`) carry a comment
saying why.

The fix belongs in the compiler and is sketched in docs/ROUND63.md: an
error union is the struct `{ __err: u32, __val: T }`, and for `T = f64`
the System V classification puts the second eightbyte in the SSE class
while stage 0 passes `f64` as a bit pattern in the integer registers.

**Gap B -- there is no `const` of type `f64`.** `lib/std/math.fi` says it
outright ("a `const PI: f64` cannot be said"). A limit that is a number
therefore has to be a function; `flex_no_limit()` in `lib/layout/flow.fi`
exists only for that reason.

**Gap C -- an unused function is not reported.** `content_min_width` was
left behind by a refactor and compiled without a word. Not a bug, but a
warning the language could give.

---

## 12. How to run it

```
bash tools/layout/run.sh          # everything: cases, Chromium, paint
                                  # order, real pages, soak run
bash tools/layout/run.sh --fast   # without the real pages
python3 tools/layout/stack.py .layout-work/layout    # only the paint order
bash tools/layout/throughput.sh   # instructions per element (callgrind)
bash test.sh                      # section 23 runs run.sh --fast
```

The five hand-computed programs run as ordinary tests:

```
tests/1180_layout_position.fi        fixed, sticky, the initial
                                     containing block
tests/1181_layout_stacking.fi        the paint order and the hit test,
                                     with a counter check that must fail
tests/1182_layout_float_probe.fi     the line probe at the right height
tests/1183_layout_flex_axes.fi       both flex axes, order, justify, align
tests/1184_layout_percent_height.fi  percentage heights and border-box
```

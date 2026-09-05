# Round FIRN-LUECKEN -- four gaps, and the one that was hiding behind them

Four holes were reported, all four found by writing a real program against
the library instead of against the test suite: `demos/choropleth/main.fi`,
a choropleth map of Europe drawn entirely in Firn. Everything below is
measured on this machine (AMD EPYC 7571, one core, Linux x86_64).

The round found a fifth hole on the way, and it was the worst of them.

---

## 0. The one that was hiding: the fixpoint was already broken

Before a line of it was written, the round could not do its own acceptance
test. `tools/fixpoint.sh` stopped at **stage 2**:

```
STAGE 2 FAILED (rc=2)
```

Stage 1 -- `firnc0` (Rust) compiles `bin/firnc1.fi` -- produced a compiler
that could not find a single module. Not "a module": ANY module. A one file
program compiled, `tests/110_module.fi` did not, and it said nothing at all
while failing.

`strace` showed the reason and made no sense of it:

```
access("tests/modules/math.fi", F_OK) = 0        <- found
access("/root/firn/lib/modules/math.fi", F_OK) = -1 ENOENT
access("/tmp/../lib/modules/math.fi", F_OK) = -1 ENOENT
+++ exited with 2 +++
```

It **found** the file and went on searching. `imports_collect` in
`bin/firnc1.fi` is a chain of `if !found { ... found = true }` -- and
`found` was false however it was set.

`--no-opt` produced a working stage 1. The pass is
`threading.rs::thread_bool_phis`, new in round SPEED (52a13e750), and the
25 lines that reproduce it are now `tests/1633_bool_phi_thread.fi`:

```
    var found: bool = false
    if is_file(k) { found = true }
    if !found { if is_file(k + 1) { found = true } }
    if !found { ...a loop with a break... }
    if !found { return false }
    return true
```

In FIR:

```
bb3: %21 = phi.bool [bb1 %6, bb2 %3] ; brcond %21, bb5, bb4
bb6: %20 = phi.bool [bb5 %21, bb9 %19] ; brcond %20, bb11, bb10
```

The pass threads bb3: both predecessors learn the answer on their own edge
and jump past the join, and both entries are struck from the phi. The phi
is then empty -- and `%21`, the value it defines, **still travels on into
the phi of bb6**. Round SPEED wrote the rule down itself ("a pass may
delete a block's control flow, but not a definition another instruction
still names") and applied it to the dead block, not to the dead value.

The fix is one set, `threading.rs`: a join may only be threaded if the
value of its phi is read by **nothing but the `brcond` of its own block**.
That leaves the case the pass was written for -- the short circuit of
`&&` / `||`, whose phi is read exactly once -- untouched, and the
measurement of round SPEED with it.

**Why nothing caught it.** The corpus has the shape "one bool, two
branches" many times over; it did not have "one bool, three branches, and
the bool read by the second join". The compiler's own source did, in the
one function every multi file compilation goes through. Since the fixpoint
had last been run before that round's last commit, the tree on `main` could
not compile itself.

---

## 1. `const` could not hold a floating point number

`compiler/src/sema.rs:702` said, word for word: *"'const' supports only
integer and bool types in stage 0"*. Anyone who needed an `f64` constant
wrote a function -- `demos/choropleth/main.fi` had four for its map window,
`lib/browser/sym.fi` in Certus has one and explains why in a comment.

The underpinnings were there the whole time: `lower.rs` carries the BIT
PATTERN of a float literal into FIR as a constant. What was missing was an
evaluator, and the note in the source said why it was missing -- *"an
evaluator that is only ALMOST IEEE-754 would be worse than none"*.

That is true, and it is exactly why this one may exist: it does not compute
in some private format. It computes in `f64` and in `f32`, the two formats
the machine has, with the machine's own round-to-nearest-even. `0.1 + 0.2`
folded in the compiler and `0.1 + 0.2` computed by `addsd` are the same 64
bits -- `tests/1630_const_float.fi` checks that bit for bit, including
`1.0 / 3.0`, `f32` arithmetic in `f32`, and the exact `f32 -> f64`
widening.

Both compilers learned it, and they had to: `compiler/src/sema.rs`
(`eval_const_float`, and `eval_static_float` now delegates to it, so a
`const` and a `static` cannot disagree) and `lib/firnc1/sema.fi`
(`const_float`, likewise for `static`).

What is deliberately refused, with three negative tests:

* an integer literal or an integer constant (`const A: f64 = 100`) -- that
  is the mistake `opt.rs::fold_cast` reports from round 20, where the
  constant 100 became a `const.f64` with the bit pattern 100, i.e. 5e-322;
* a cast, which is the same door with a friendlier name;
* an `f64` inside an `f32` constant: narrowing rounds, and a rounding the
  reader did not write down does not happen here.

Three places had to be told that a float constant is not a number:
`eval_const`, `literal_index` and `comptime.rs` -- otherwise `[0; SCALE]`
would have been an array of 4613937818241073152 elements.

---

## 2. and 3. The rasteriser could fill but not stroke

`grep stroke lib/` found nothing. A border was a quadrilateral per segment,
built by the caller.

`lib/paint/stroke.fi` (833 lines) is the answer: polyline and closed ring,
width, joins (miter with limit, round, bevel), caps (butt, round, square),
anti-aliased through the same rasteriser as the fill, no allocation at all
(the caller hands it memory, like `font.raster`), every function `#[no_gc]`,
one import.

Two things beyond SVG, and they are gaps 2 and 3 of the report:

* **ALIGNMENT.** `ALIGN_INNER` / `ALIGN_OUTER` / `ALIGN_CENTER`. Which side
  is the inside is read off the signed area of the ring, not asked of the
  caller -- the caller does not know which way round a GeoJSON ring runs.
  `tests/1631_stroke_outline.fi` checks that the same ring clockwise and
  anticlockwise gives the same band, to the exact square pixel.
* **`stroke_set_fit`.** Putting the band inside stops it from spilling out
  of the shape; it does not stop it from filling the shape completely.
  Malta is 17 square pixels on this map with a circumference of 17, so a
  border of 0.7 covers twelve of them. `fit` is a rule and not a special
  case: the border may take at most this fraction of the AREA, so
  `w <= fit * A / P`. For Germany (35642 square pixels, circumference 1161)
  that allows 15 pixels and changes nothing; for Gozo (4.3 and 8.1) it
  allows 0.27, and the island keeps its colour.

**The areas are exact**, and that is how the file is tested -- the sum of
the coverage over all pixels is the area of the shape, and
`lib/font/raster.fi` computes coverage analytically:

| shape | expected | measured |
|---|---|---|
| line 40 x 4, butt caps | 160 | 160.00 |
| the same, square caps | 176 | 176.00 |
| the same, round caps | 172.57 | 172.49 (inscribed polygon) |
| ring 40 x 40, width 4, centred | 640 | 640.00 |
| the same, inside | 576 | 576.00 |
| the same, the other way round | 576 | 576.00 |
| the same, outside | 704 | 704.00 |
| 8 x 8 ring, width 4, `fit = 0.5` | 28 | 28.00 |

### What is NOT true about the quadrilaterals, measured

Certus (`lib/svg/strich.fi`) says of the same construction that the overlap
of two neighbouring quadrilaterals is counted twice and gives a hard seam
on the inside of every bend. That is true of the arithmetic. On the shapes
this map has, it is **not measurable**:

* a ring of 400 segments, 0.7 pixels wide (a coastline): quadrilaterals
  140.74 square pixels, one outline 140.74, exact `2*pi*r*w` = 140.74;
* a zigzag of ten sharp bends, 3 pixels wide, per pixel against the same
  shape at eight times the resolution: worst pixel 0.304 with
  quadrilaterals, 0.282 with one outline, 40 pixels off by more than 2 %
  in both cases. The single outline crosses ITSELF at the inside of a
  bend, and that loop double counts in the same way.

What IS different is the other side of the bend: two quadrilaterals leave
the outer wedge empty, because a quadrilateral has no join. On that zigzag
it is 3.9 square pixels over ten bends, and it grows with the square of the
width -- invisible at 0.7 pixels, and at 8 pixels a symbol falls apart into
separate strokes. That, and everything a quadrilateral cannot do at all
(caps, miter limit, alignment, a width that gives way), is why the file
exists. The seam is the smallest of the reasons, and the claim is corrected
in the file's own header.

### One solution, not two

The constants are Certus's constants: `CAP_BUTT/ROUND/SQUARE` are 0/1/2
like `K_STUMPF/K_RUND/K_ECKIG`, `JOIN_MITER/ROUND/BEVEL` are 0/1/2 like
`S_GEHRUNG/S_RUND/S_ABGE`, the miter limit is SVG's and the construction is
the same. What is different is the customer: `strich.fi` writes a
`svg.pfad.Pfad` and needs `html.mem`; this file writes straight into the
rasteriser and allocates nothing, so it also works from a kernel.

**Not done, and named so it is not mistaken for done:** `lib/svg/strich.fi`
in Certus still exists beside it. The dash pattern lives there, and moving
it needs a round in that repository.

---

## 4. Clearing cost more than drawing

`raster_begin` zeroed the whole window: 2.5 million cells, 20 megaoctets,
for a picture of 1540 x 1665 -- per path. The map answered that by opening
a window of its own per polygon, computed from its bounding box. That is
library work done in the application.

`lib/font/raster.fi` now carries three things:

* `ux0/uy0/ux1/uy1`, the rectangle of cells really written, plus the
  `stride` it belongs to;
* `zeroed`, how many cells from the start of the buffer are known to be
  zero -- memory from `raster_attach` is not, so the first window of a
  given size pays for itself once and a later, larger one pays for the
  piece that grew;
* the invariant that carries it: after every `raster_begin`, every cell in
  `[0, zeroed)` is 0.0.

The rectangle is widened once per EDGE, not once per cell. `raster_finish`
runs over those rows only -- to the left of the rectangle every cell is
zero, so the running sum starts at zero there, which is not an
approximation but the same number the walk from column zero would reach --
and then narrows the rectangle down to the cells that really carry
COVERAGE. `canvas_fill_raster` walks the ink instead of the paper, and a
caller can ask "did anything land in the picture at all" without a bounding
box of its own.

`tests/1632_raster_used_rect.fi` checks the promise on memory deliberately
filled with `0xFF`: a window opened a second time behaves as if it had been
zeroed completely, a path entirely left of the window leaves an empty
rectangle, the rectangle is the ink, and a small window followed by a big
one does not read what was never cleared.

**Proof that it changes nothing:** the unchanged `main.fi` of before the
round, compiled against the new library, writes a PNG that is
**byte-identical** to the one from before.

---

## The second writer, and what it cost

`tools/paintb3/run.sh` -- the reference suite of round B3 -- reported two of
its seven frozen pictures as changed: `04_shadows.html` (6069 of 480000
pixels, and one of them a blue box that had become white) and
`05_text.html`.

`lib/paint/painter.fi::blur_pass` writes into the coverage cells DIRECTLY:
a box blur is a running sum over the finished coverage, in place, and it
spreads ink far beyond the rectangle the path touched. While
`raster_begin` cleared everything, nobody had to tell the rasteriser about
that. Now somebody does -- `raster_mark`, two lines at the end of
`blur_pass` -- or the shadow stays behind and turns up in the next path.

That is the general shape of the risk this round takes, and it is worth
naming: **the promise "cleared is what was written" only holds if
everything that writes says so.** `blur_pass` is the only such writer in
the tree (`grep raster_cov_row`), and it is now marked.

Afterwards: `B3 OK: 202 / 541 official reference tests, 393 glyphs against
an independent rasteriser, PNG both ways, text inside its boxes, 7 / 7 own
cases identical in three build stages` -- exactly the frozen limits of
`tools/paintb3/minquota.txt`, and the shadow case bit-identical to before
the round.

## The application, before and after

| | before | after |
|---|---|---|
| lines in `demos/choropleth/main.fi` | 588 | **533** |
| f64 constants as functions | 4 | 0 |
| own raster windows | 3 places, ~40 lines of bbox arithmetic | 0 |
| stroking | `seg_quad`, 30 lines | `paint.stroke`, 9 lines |
| rings drawn | 237 | 245 |
| rings with a border | only those over 16 px | **all of them** |
| Malta | no border | border, and it keeps its colour |
| run time (median of 9, interleaved) | 2.72 s | **2.98 s** |

**IT GOT SLOWER BY 0.26 SECONDS, 10 %,** and the first attempt to explain
that away was a measurement error worth writing down: a single run of the
old file with its border condition removed came out at 3.54 s, which would
have made the new stroker the faster one. Measured properly -- nine runs,
the three binaries interleaved, median -- that same file takes 2.72 s, the
same as the original. On this machine a single run scatters by 10 %, which
is exactly the size of the effect being measured, so a single run says
nothing at all.

Where the 0.26 s go, measured the same way:

| | median |
|---|---|
| old file, complete | 2.97 s |
| new file, WITHOUT the border pass | 2.80 s |
| new file, complete | 3.10 s |

So the fill path got *faster* -- and that is with the new file pushing all
1632 rings of the file through the rasteriser instead of the 237 the old
bounding box test let through. The whole of the difference, and a little
more, is the border: a real outline on every one of the 245 rings, with
joins, with an area and a circumference computed per ring for the width
limit, instead of a strip of quadrilaterals on the big ones only.

And two thirds of the run time is neither: 2.0 of the 3.0 seconds are the
JSON parser on 3 MB and `deflate` on the PNG, neither of which this round
touched.

The picture: 27 of 27 countries in their own colour, Malta with a border --
proved by painting the border colour red and counting: 14 pixels of border
ink on Malta afterwards, **0 before**.

---

## What was not achieved

* Certus keeps its own stroker (see above).
* The run time of the demo went up by 10 % (2.72 s -> 2.98 s, medians of
  nine interleaved runs), for more drawing. The JSON parser and `deflate`,
  which are two thirds of the cost, were not touched.
* Round caps are 0.05 % short of the exact area -- the arc is an inscribed
  polygon, the same deviation `raster.fi` writes down for its curves.
* **A bug found and NOT fixed**, because it belongs to another round: with
  `--no-opt`, a `str` returned by a function and used inside an
  interpolation comes out empty. `io.print_line(say(true))` is right,
  `f"{say(true)}"` is not. It is not a regression of this round -- the
  compiler at `main` does it too -- and the reproduction is four lines:

  ```
  fn say(b: bool) -> str { if b { return "found" }  return "MISSED" }
  fn main() -> i32 { io.fmt_print_line(f"{say(true)} {say(false)}")  return 0 }
  ```

# RUN.md -- build it, run it, measure it yourself

Everything here **has been run** exactly as it stands (2026-08-14, AMD EPYC
7571, Linux x86_64, rustc 1.99.0-nightly, binutils `as`/`ld`). Relative paths
only, everything inside this directory.

## 0. Prerequisites

* `cargo`/`rustc` (only to build the compiler and the yardsticks -- the
  compiler itself has **no** external crates)
* GNU `as` and `ld` (assembler and linker, **no** C compiler as a backend)
* `python3` (only for the workbenches: benchmarks, generators)
* `gdb` (only for the debugger proof)

## 1. Build the compiler

```sh
cargo build --release --manifest-path compiler/Cargo.toml
```

Expected: **zero warnings**, binary at `compiler/target/release/firnc`.

## 2. Compile and run a program

```sh
compiler/target/release/firnc -o /tmp/hello examples/hello.fi
/tmp/hello ; echo "exit=$?"
```

Further modes:

```sh
firnc --no-opt -o /tmp/a file.fi      # without the optimizer (same result!)
firnc --emit=asm file.fi              # x86_64 assembly (Intel syntax)
firnc --emit=fir file.fi              # own IR, readable
firnc --help
```

Several files into **one** binary (module system): name the root file,
`import path.module` resolves relative to its directory:

```sh
compiler/target/release/firnc -o /tmp/mod tests/110_module.fi
/tmp/mod ; echo "exit=$?"     # exit=60, as stated in line 1 of the file
```

## 3. The whole test suite

```sh
bash test.sh
```

Measured result for THIS state (re-run 2026-09-21, same machine):
**`FAIL 4/1592 failed`, exit code 1** -- 1588 of 1592 checks pass, four
sections fail. Which four, and why, is in **section 4d**; none of them
is a calculation of the UI library, and two of them are house-style
debt this tree takes on knowingly.

The number `PASS 485/485` stood here for a long time and is NOT what
`bash test.sh` prints today: the suite has grown from 485 to 1592
checks since, and it has ended non-zero since the rounds described in
4d. It is corrected rather than kept, because a reader who runs the
command in the box above has to read the same result here.

Of those 1592: all 533 `tests/*.fi` in four build stages (`opt`,
`noopt`, `devfast`, `safe`), the negative tests, and one section proof
each for the optimizer (`test_opt.sh`, 41 checks in its own right), the
result-location guarantee, the architecture guards, the symbol scheme
and the HTML5 tokenizer against html5lib. Runtime on a loaded machine
about 50 minutes, on an idle one a few minutes.

The UI library's own acceptance run is separate and IS green -- see
section 9: `sh tools/fui/run.sh --images` ends on `ALL CHECKS PASSED`.

Machine-readable (CI, goal 9 / ACCEPTANCE item 4 A):

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json > /tmp/firn.json
python3 -c "import json;d=json.load(open('/tmp/firn.json'));print(d['total'],d['passed'],d['failed'],d['rate'])"
# 337 337 0 1.0
```

(337 instead of 485: the runner contains neither the optimizer proof
`test_opt.sh` nor sections 6-9 of `test.sh`.)

## 4. The proofs one by one -- what the jury checks

| What | Command | Measured result |
|---|---|---|
| **Exhaustiveness check for `match`** | `firnc -o /tmp/m tests/neg/match_missing_variant.fi` | `error: 'match' is not exhaustive: ... not covered` **with line:column**, exit != 0 |
| **Jump table for 32 states** | `firnc --emit=asm -o /tmp/zm.s tests/230_state_machine.fi && grep -c "jmp qword ptr" /tmp/zm.s` | `1` -- one indirect jump through a `.quad` table, no comparison chain |
| **WTF-16, unpaired surrogate** | `firnc -o /tmp/s tests/300_str16_surrogate.fi && /tmp/s` | `3 97 55296 98 0 0 5 97 239 191 189 98 5 97 237 160 128 98 1 55296` -- `0xD800` is preserved, `to_utf8()` returns nothing, `to_utf8_lossy()` returns `EF BF BD` |
| **strtod/dtoa hard cases** | `firnc -o /tmp/h tests/304_strtod_hardcases.fi && /tmp/h` | 26 bit patterns, starting with `4591870180066957722` (= `0.1`); the expected values are given as `// expect_out:` in line 1 of the same file |
| **100,000 doubles there and back** | `bash tools/dtoa_vectors/run.sh 100000 4242` | `OK: 100000/100000 bit-identical on the way back, 100000/100000 shortest form like Rust` (7.9 s) |
| **Benchmarks against Rust `-O`** | `BENCH_RUNS=5 bash bench/run.sh` | median **3.36x** slower (range 1.57x-6.04x), table in `bench/RESULTS.md`. **Target <= 2x missed** |
| **The optimizer has an effect** | `bash test_opt.sh` | `PASS 41/41` (FIR before/after) |
| **The debugger shows `.fi` lines** | `firnc --no-opt -o /tmp/gdbdemo docs/gdb_example.fi && gdb -batch -ex "break summe" -ex run -ex bt /tmp/gdbdemo` | `Breakpoint 1, summe () at docs/gdb_example.fi:2` and `#1 ... main () at docs/gdb_example.fi:11` |
| **The generated Str tests are current** | `python3 tools/strlib/expand.py --check` | `expand.py: 0 files out of date` |
| **Cleanliness** | `grep -rn "todo!\|unimplemented!" compiler/src` | no hits |


## 4a. HTML5 tokenizer and error unions (round 3)

```sh
bash tools/tokenizer/run.sh
```

Builds the tokenizer from `lib/html/*.fi` in **three** build stages, runs all
**6,810** html5lib cases, checks that all three build stages produce the same
balance, and measures throughput against html5ever. **Two** rates are
reported: token stream only (left column) and, in addition, with the parse
error codes compared (right column, `harness.py --with-errors`). Measured
result (2026-08-14):

```
TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %
   noopt: 6810 without / 6809 with error codes -- equal
   devfast: 6810 without / 6809 with error codes -- equal
   -- corpus 'html5lib' (edge cases of the test suite, deliberately pathological)
      Firn      :     4.59 MB/s  (0.889 s for 4.08 MB, best of 3)
      html5ever :    11.22 MB/s  (0.363 s, best of 3)
      factor    : 2.45x slower than html5ever (acceptance goal <= 2.00x)
   -- corpus 'realweb' (eight real pages out of testdata/realweb/)
      Firn      :     7.44 MB/s  (0.632 s for 4.70 MB, best of 3)
      html5ever :    42.60 MB/s  (0.110 s, best of 3)
      factor    : 5.72x slower than html5ever (acceptance goal <= 2.00x)
```

Measurements are taken on **two** corpora: `html5lib` (the inputs of the test
suite, deliberately pathological -- almost nothing but edge cases, the worst
case) and `realweb` (eight saved real pages, `testdata/realweb/MANIFEST.md`).
Two further complete runs gave 2.25x / 2.79x (html5lib) and 7.72x / 7.84x
(realweb), a fifth 3.09x and 6.39x respectively; the range is therefore
2.25x-3.09x (html5lib) and 5.72x-8.31x (realweb). The balance was identical in
every run and in all three build stages, throughput varies by about 30 %.

Step 0 of `run.sh` proves that the expectations were not touched:

```sh
bash tools/tokenizer/verify_testdata.sh              # sha256 against the repo set
bash tools/tokenizer/verify_testdata.sh --against-upstream   # additionally against GitHub
```

Step 2b of `run.sh` is the **counter-check without the XML adaptation**:

```
python3 tools/tokenizer/harness.py .tokenizer-work/tokenize --no-xml-mode
TOTAL                            6807 /   6810    99.96 %
```

The XML adaptation (`xmlViolationTests`) is an optional mode of the driver
(job flag bit 0, `tools/tokenizer/LOG.md`); the harness enables it only for the
four cases from `xmlViolation.test`, the HTML path stays the same.

The html5ever yardstick has to be built once for this (a Cargo project of its
own, **not** a dependency of the compiler):

```sh
cargo build --release --manifest-path bench/tokenizer/Cargo.toml
```

Without it `run.sh` keeps running and reports the missing yardstick.

Individual proofs:

| What | Command | Measured result |
|---|---|---|
| **The tokenizer is Firn** | `wc -l lib/html/*.fi tools/tokenizer/harness.py` | 8,647 lines of `.fi` against 295 lines of harness; the state machine sits in `lib/html/tokenizer.fi` (1,516 lines) |
| **Jump table over 73 states** | `firnc --emit=asm -o /tmp/tok.s lib/html/tokenize_main.fi && grep -c "jmp qword ptr" /tmp/tok.s` | `1` -- indirect jump through `.Ltbl_tokenizer__tokenize_0` |
| **Character references one by one** | `python3 tools/tokenizer/check_entities.py` | `bestanden: 4657 / 4657` |
| **Error union: `catch` delivers the fallback** | `firnc -o /tmp/e tests/403_catch_replacement.fi && /tmp/e; echo $?` | `0` |
| **Error union: `try` propagates** | `firnc -o /tmp/e tests/401_try_chain.fi && /tmp/e; echo $?` | the value entered in line 1 as `// expect_exit:` |
| **A discarded `!T` is an error** | `firnc -o /tmp/e tests/neg/err_discarded.fi` | `error: the result must not be discarded: the type 'E!i32' is marked with #[must_consume]` with line:column |
| **`try` outside an error-returning function** | `firnc -o /tmp/e tests/neg/err_try_outside.fi` | `error: 'try' is only allowed in a function with an error union return type, this one returns i32` with `8:13` |

## 4b. Freestanding compilation: `profile kernel` (round 52)

```sh
bash tools/freestanding/run.sh
```

Measured result (2026-08-19): **41 passed, 0 failed** -- among them a real QEMU
boot of the kernel example with **both** compilers.

| What | Command | Measured result |
|---|---|---|
| **ELF object instead of a binary** | `firnc -o /tmp/k.o demos/freestanding/core.fi && readelf -h /tmp/k.o \| grep Type` | `REL (Relocatable file)` -- no `ld`, no `_start` |
| **No undefined symbols (except `osum_panic`)** | `nm -u /tmp/k.o` | empty, or exactly `osum_panic` if the program uses checked arithmetic (round 72, SPEC section 13) -- `demos/freestanding/start.s` defines it, resolved when the object is actually linked (next row) |
| **No system call in the code** | `objdump -d /tmp/k.o \| grep -c syscall` | `0` |
| **It boots** | `ld -n -T demos/freestanding/linker.ld --defsym=KERN_START=_F0.kern_start -o /tmp/k.elf /tmp/start.o /tmp/k.o && objcopy -O elf32-i386 /tmp/k.elf /tmp/k.mb && qemu-system-x86_64 -kernel /tmp/k.mb -serial stdio -display none` | `FIRN: profile kernel ist` / `freestanding.` |
| **`syscall` in the kernel profile** | `firnc -o /tmp/x tests/neg/free_syscall_in_kernel.fi` | `error: 'syscall' does not exist in profile 'kernel'` with line:column |
| **Floating point without `#[allow_fp]`** | `firnc -o /tmp/x tests/neg/free_float_without_allow_fp.fi` | `error: floating point (the type f64) is allowed in profile 'kernel' only with #[allow_fp] ...` |
| **`#[interrupt]` cannot be called** | `firnc -o /tmp/x tests/neg/free_interrupt_call.fi` | `error: 'ih' is an interrupt entry point and cannot be called` |
| **volatile holds** | `firnc --emit=fir tools/freestanding/volatile.fi \| grep -c 'asm.void "pause"'` | `3` -- three literally identical blocks, no CSE |

In detail in `docs/ROUND52.md`.

## 4c. fUi -- the UI library, and its acceptance run (round UI-WEB)

`lib/fui/*.fi` is the house UI library, written in Firn only: no C, no
libc, no foreign library. Round UI-WEB added the four modules that were
missing for the expressiveness of modern HTML/CSS:

| Module | What it is |
|---|---|
| `lib/fui/anim.fi` | Tween engine: linear, `cubic-bezier(x1,y1,x2,y2)` with Newton iteration, `steps(n)`, analytically solved spring; animator register with `anim_tick(ms)`; widget state transitions (colour in sRGB **and** OKLab, premultiplied alpha, radius, border, lift) |
| `lib/fui/flex.fi` | CSS flexbox: direction/justify/align/wrap/align-content, gap, grow/shrink/basis with min/max clamping and the redistribution that follows from it. Extends `lib/fui/layout.fi`, does not replace it |
| `lib/fui/effect.fi` | Separable box blur in three passes (running sum, O(1) per pixel) as a Gauss approximation, `drop_shadow`, `backdrop_blur`/glass, colour matrix |
| `lib/fui/transform.fi` | Affine 2x3 transforms with a push/pop stack. The matrix arithmetic comes from `lib/svg/matrix.fi` -- **no second matrix library**. Text goes through the matrix as a glyph outline, pictures via inverse mapping with bilinear sampling, hit testing via the inverse |

One thing the integration pass found and fixed: `lib/fui/editor.fi` (the
key handling of a text field) **never compiled**. In the branch for the
Backspace key the line `if ctrl && !tb_has_sel(t) {` was missing, so the
brace below it closed the function and the parser hit an `if` at top
level. Nobody noticed, because no program imported the file and the
acceptance run did not know it. The guard is back, and the file is now
built and measured by section 18 -- a check that fails with
`got 10 want 7` if the guard is wrong again.

The integration pass after the round found two more, both of the kind
that breaks nothing today and something later:

* **The five states stood in two files.** `lib/fui/theme.fi` declared
  `STATE_NORMAL .. STATE_DISABLED` a second time, with the same values
  as `lib/fui/style.fi`. The copies agreed only for as long as nobody
  touched one of them -- and `style.fi` had already grown a sixth
  state (`STATE_SELECTED`) that `theme.fi` never heard of. The copy is
  gone; `theme.fi` and its one outside caller
  (`tools/fui/preview_main.fi`) read `style.STATE_*`. Firn has no
  cross-module constant alias (`const A: u32 = style.A` is an error),
  so there is no way to keep a second name honest -- it had to go.
  Proof that nothing moved: the 22 evidence pictures are byte for byte
  identical before and after.
* **A reason that had stopped being true.** The head of
  `lib/fui/painter.fi` stated that `lib/svg` does not exist in the Firn
  tree. It did not when the file was written and does since `2b82d78e`
  -- `lib/fui/transform.fi` takes its matrix arithmetic from there. The
  paragraph now gives the reason that actually carries (this layer
  draws straight into the rasteriser, `svg.pfad` builds and keeps a
  path object) and says where the rule points instead.

### Round DECLARATIVE: describing a surface instead of painting it

Three modules were added in this round, and they are what separates a
UI library from a drawing library:

| Module | What it is |
|---|---|
| `lib/fui/viewport.fi` | The scroll viewport: a cut-out with its own size over content of any size, **real clipping** through `canvas.clip_push_rect` (no second clip), wheel and keyboard scrolling, scrollbars whose thumb length follows from cut-out/content (computed in `wave2.scrollbar_thumb`, the one place), kinetic fling as a plain `anim.Animation` (no second clock), `viewport_ensure_visible`, and a hit test that accounts for the offset |
| `lib/fui/scene.fi` | The tree: nodes with kind, `id`, classes, children and style, walked in four separated passes -- style, measure (`render.pref_of`), layout (`flex.flex_layout`), draw. No node may change its size while drawing; `scene_size_drift` counts every attempt and the check reads that number |
| `lib/fui/sheet.fi` | The style sheet, built **in source** (no CSS parser -- `lib/browser` already has one): rules "selector -> style values", selecting by kind, id, class, state and ancestry, merged by a specificity that is worked out (id 10000 &gt; class 100 &gt; kind 1, ties go to the later rule), plus inheritance of exactly four values (`INHERIT_MASK` = 36996: colour, font size, font id, line height) |

None of the three recomputes anything that already exists: flexbox from
`flex.fi`, time and easing from `anim.fi`, matrices from
`lib/svg/matrix.fi` through `transform.fi`, measuring and drawing from
`render.fi`/`wave2`/`wave3`.

Their checks are sections 18b, 18c and 18d of the acceptance run
(`viewport_main.fi`, `sheet_main.fi`, `scene_main.fi`), all numeric:
the cascade on deliberately contradictory cases, inheritance **and its
boundary**, the clipping counted pixel by pixel on a real canvas (zero
points outside the cut-out, 49632 inside, and the counter-test without
the clip reports 44720 spilled points in red), the thumb length from
the ratio, and the hit test under scrolling.

Section 19c counts, mechanically, what the round is for: the same tool
bar -- three buttons, a search field with `grow`, one accent button --
painted call by call in `demos/fuidemo/main.fi` (`fn
werkzeugleiste_gemalt`) against the same bar described in
`tools/fui/gallery9_main.fi` (tree plus its rules in the style sheet).
Counted are lines of code, without blanks and comments, in three cuts,
because a single number here would necessarily hide something:

| cut | painted | described |
|---|---|---|
| raw (everything inside the markers resp. the function) | 64 | 42 |
| A -- without the captions, subtracted on **both** sides | 54 | 40 |
| B -- additionally without the looks (fill, border, colour, radius) | 46 | 21 |

Cut B is the honest headline: what is left is the structure alone, and
there the description needs **21 lines where painting needs 46** -- the
distribution, the setting of every single rectangle and the own drawing
loop fall away entirely. In cut A the saving is small on purpose: a
style sheet writes colour and radius **once for the whole page**, and
this comparison still charges all of it to the described side. The run
stops if A ever exceeds 90 % or B ever exceeds two thirds of the
painted side. Shared helpers (`setze`, `item_von_widget`) are counted
on neither side, because `titelzeile` and `dialog` call them too --
charging shared lines to one side only would be talking the saving up.

**And the same comparison inside ONE file (section 19d).** 19c counts
across two files, and a reviewer may rightly ask whether that is still
the same piece of surface. So the same bar is counted a second time
where both versions stand next to each other and provably paint the
same picture: `demos/fuidemo/main.fi`, `fn werkzeugleiste` (markers
`>>> LEISTE BESCHRIEBEN` ... `<<< LEISTE BESCHRIEBEN`) against
`fn werkzeugleiste_gemalt` in **the same file** (markers
`>>> LEISTE GEMALT` ... `<<< LEISTE GEMALT`). `pruefe_leisten` in that
file holds their five rectangles against each other as integers, at 952
**and** at 260 points of width, where the 140-point clamp of the search
field really bites.

| cut | painted | described |
|---|---|---|
| raw | 52 | 51 |
| A -- without the captions | 42 | 41 |
| B -- the structure alone | 32 | **22** |

The first two numbers say something uncomfortable, and they say it as
numbers instead of as an excuse: for a **single** bar the description
is not shorter (51 against 52 lines) -- a style sheet for one bar does
not amortise. The structure is where it pays: 22 lines against 32,
that is 69 %, and the run stops above 75 % (and also if the described
version ever gets longer than the painted one in raw or in cut A).
Outside the markers stands, on both sides, only the check itself: the
handing out of the five rectangles and the message about an incomplete
tree. Nothing of the surface.

**One command checks all of it:**

```sh
sh tools/fui/run.sh              # the checks only
sh tools/fui/run.sh --images     # additionally paint the evidence pictures
```

It needs `compiler/target/release/firnc` (section 1) plus `objdump` and
`nm` for section 1 of the run. It ends on `ALL CHECKS PASSED`; every
single check exits non-zero on failure and `set -e` stops the run, so a
check cannot go missing silently -- that is the whole point of the file.

**And that is not just claimed here.** `.gauntlet-shots/run-log.txt`
holds the complete output of one such run (21.09.2026, sections 1 to
19b including the check of every written PNG, all `got X want Y` lines
verbatim, exit code 0, last line `ALL CHECKS PASSED.`). A reviewer who
does not start the run can read what the run says -- the same reason
the pictures are checked in next to it.

What the eighteen sections do, in short: section 1 compiles
`lib/fui/core.fi`, `style.fi` and `layout.fi` with `--profile=kernel` and
**counts** that not one `syscall` instruction and no foreign name besides
`osum_panic` is left in the objects (the core has to stay usable from
inside a kernel). Sections 2-11 are the earlier rounds (contrast,
icons, the three widget waves, pictures, the ported `lib/svg`, text).
Sections 12-15 are the new modules, sections 16-17 the interaction and
the line breaking, section 18 the operation of the text field
(`lib/fui/editor.fi`: Ctrl+A replaces instead of appending, the word
jumps, Ctrl+Backspace/Delete with and without a selection, undo/redo,
and the keys the field hands back to the program). The numbers are
checked **numerically** against
values worked out by hand ("got X want Y"): easings at their support
points, flex distribution in pixels, three box passes against the cubic
B-spline `(1,3,6,7,6,3,1)/27`, known point images under the transform.

The evidence pictures are written to `$W/belege` (with `W` defaulting to
`/tmp/fui-acceptance`), that is: **inside the working directory**, never
to a fixed system path -- a run that writes outside its own tree fails
for anyone without rights there, and only after twenty passed sections.
Set `BELEGE` to put them somewhere else. If that directory cannot be
created or written, the run says so and exits non-zero; it does not
print `ALL CHECKS PASSED` with pictures missing.

The same twenty-six files are checked in under `.gauntlet-shots/`,
light and dark for each, numbered in reading order. That is the whole
delivered evidence set: a reviewer who does not start the run sees in
this table which picture carries which point of the acceptance bar, and
which program writes it.

| File in `.gauntlet-shots/` | written by | what it has to show |
|---|---|---|
| `01/02-wave1-grundelemente-{hell,dunkel}.png` | `tools/fui/gallery_main.fi` | bar 5: button in five states, six style looks, label alignment and sizes, the box with the stretchy address field (the address stands there COMPLETE, measured against the field), the 6x2 grid, the window buttons from the core at 1x/2x/3x -- nothing overlaps, nothing is cut off, readable in light AND dark |
| `03/04-wave2-widgets-{hell,dunkel}.png` | `tools/fui/gallery2_main.fi` | bar 5: checkbox, radio, switch, slider, progress, spinner, tooltip, tabs, menu -- each at its measured size, the tooltip without the grey box behind it |
| `05/06-wave3-widgets-{hell,dunkel}.png` | `tools/fui/gallery3_main.fi` | bar 5: list, table, tree, card, badges, date and colour picker, modal dialog over the scrim -- and the dialog/menu shadow that `render` takes from `effect.drop_shadow_round` |
| `07/08-text-{hell,dunkel}.png` | `tools/fui/gallery4_main.fi` | bar 5: line breaking, ellipsis, outline and text shadow -- no text runs out of its box |
| `09/10-bild-svg-{hell,dunkel}.png` | `tools/fui/artshow_main.fi` | the round BILD+SVG: the same SVG re-rastered per size (12..64) instead of scaled, `currentColor`, pictures with alpha over four grounds |
| `11/12-anim-phasen-{hell,dunkel}.png` | `tools/fui/gallery5_main.fi` | bar 4: one movement as a phase series with a visibly NON-linear course -- six curves (linear, ease-in/out/in-out, steps, spring), nine phases each, plus the state transition of a widget |
| `13/14-flex-varianten-{hell,dunkel}.png` | `tools/fui/gallery6_main.fi` | bar 4: the flex variants side by side with correct gaps -- `justify-content` in all six forms, `align-items`/`align-self`, `wrap` with `align-content`, `grow`/`shrink`/`basis` with min/max clamping |
| `15/16-effekt-blur-schatten-glas-{hell,dunkel}.png` | `tools/fui/gallery7_main.fi` | bar 4: soft shadows with a visible gradient and NO hard edge (four blur radii, one with spread, painted with the library's unchanged `render.shadow_color`), glass/backdrop blur over a patterned ground with the shapes behind it still recognisable, colour matrix (grey, saturate, contrast) |
| `17/18-transform-rotate-scale-{hell,dunkel}.png` | `tools/fui/gallery8_main.fi` | bar 4: rotated, scaled and skewed widgets with clean edges (no stair-stepping), pictures under the inverse mapping with bilinear sampling, the hit test under rotation |
| `19/20-demo-anwendung-{hell,dunkel}.png` | `demos/fuidemo/main.fi` | bar 2: the three new modules have a caller OUTSIDE their own check -- title bar and tool bar distributed by `flex.flex_layout` (grow on the field, every basis measured through `render.pref_of`), the hover transition of a button driven by `anim.Animator` in seven labelled phases, the dialog shadow from `effect.drop_shadow_spread` |
| `21/22-preview-zustaende-{hell,dunkel}.png` | `tools/fui/preview_main.fi` | bar 5: the five button states, the text field at rest and focused with selection and caret, the same buttons under `shape_classic`, and the whole row at 150 % scale -- the proof that shape and scale are a theme decision |
| `23/24-deklarativ-scene-sheet-{hell,dunkel}.png` | `tools/fui/gallery9_main.fi` | round DECLARATIVE: a whole page that is **described**, not painted -- a scroll viewport carrying 28 rows on 1130 points of content in a 528 point cut-out (rows visibly clipped top and bottom, scrollbar length from the cut-out/content ratio), the cascade in the picture (class 100 &lt; two classes 200 &lt; id 10000), inheritance of font and colour but not of the background |
| `25/26-deklarativ-schmal-980px-{hell,dunkel}.png` | `tools/fui/gallery9_main.fi` | the same described page on 980 instead of 1240 points: the layout is really **computed**, not written down -- the right column gets narrower, the tool bar keeps its gaps, the viewport keeps its scrollbar, and still nothing overlaps and no text leaves its box |

Every one of these pictures is measured before it is written: the
programs check their own pixels (contrast against the ground it is
really painted on, tone distance of the shadows, no band left empty)
and exit non-zero instead of writing a picture that does not keep its
promise. On top of that `tools/fui/belegpruef_main.fi` reads every
written PNG back in and checks size, colour variety and that something
stands in each of the six horizontal bands.

The set is refreshed from the run with

```sh
sh tools/fui/run.sh --images
sh tools/fui/collect_shots.sh
```

The second script holds the table "which painted picture becomes which
delivered number" -- in ONE place, so that a new piece of evidence
cannot end up in the run and be missing from the tree (or the other way
round).

Single checks without the whole run, if something is to be looked at:

```sh
export FIRNLIB="$(pwd)/lib"
compiler/target/release/firnc --opt-level=dev -o /tmp/anim tools/fui/anim_main.fi
/tmp/anim
```

## 4d. The state of `bash test.sh` (re-measured 2026-09-21)

`test.sh` runs 59 sections and takes roughly an hour and a half. It has
to be started with **`bash`**, not `sh`: line 216 uses `set -o pipefail`.
Measured on the state of this branch, from a compiler rebuilt from
source:

```
FAIL 4/1592 failed:

  tools/fixpoint.sh failed (see .test-work/fixpoint.log)
  tools/js/run.sh failed (see .test-work/js.log)
  tools/english/check.sh reports German identifiers (see .test-work/english.log)
  tools/fmt/run.sh failed (see .test-work/fmt.log)
```

All 533 `tests/*.fi` pass in all four build stages (`opt`, `noopt`,
`devfast`, `safe`). Four sections fail. Not one of them is a
CALCULATION of the UI library -- no measured value in `lib/fui` is
wrong -- but two of them ARE made worse by this round, which the
previous version of this section got wrong. They are named here so
that nobody has to find that out twice:

| Section | Fails because |
|---|---|
| `tools/fixpoint.sh` | `lib/firnc1/gctext.fi` does not match `lib/gc/*.fi` (`tools/gen_gctext.sh` was not re-run) |
| `tools/js/run.sh` | `testdata/test262/subset.sha256` is missing from the tree |
| `tools/english/check.sh` | 383 German identifiers; 223 of the reported lines name `lib/svg` (a port, `2b82d78e`), the other **160 are fUi, `demos/fuidemo` and `tools/fui` of this round** |
| `tools/fmt/run.sh` | 54 files are not in canonical `firnfmt` shape -- **including files this round wrote** |

**Two of the four fail partly BECAUSE of this round, and an earlier
version of this section wrongly claimed that none of them did.** The
claim was "every one of them names files that the round never touched";
that is false, and the logs say so:

* `.test-work/english.log` ends on `German identifiers: 383`. Counted
  out of the log, 223 of those lines mention `lib/svg` and the
  remaining **160 are fUi, `demos/fuidemo` and `tools/fui` alone**
  -- `zyklus` in
  `lib/fui/anim.fi`, `LUECKE` in `tools/fui/anim_main.fi`, `SCHRITT`
  in `demos/fuidemo/main.fi`, and so on. The round was told to take
  over the style of the modules around it, and those modules are half
  German; it did, and the counter went up.
* `.test-work/fmt.log` names `lib/fui/anim.fi`, `effect.fi`, `flex.fi`
  and `demos/fuidemo/main.fi`. Counted with `firnfmt -c`, about 47 of
  the 54 are fUi or `lib/svg` files.

Neither is a wrong number in a picture or a check that does not hold --
`sh tools/fui/run.sh --images` passes in full. They are house-style
debts, and they are written down here rather than rounded off.

It is left as debt on purpose, and the reason is checkable in ten
seconds:

```sh
compiler/target/release/firnc --opt-level=dev -o /tmp/firnfmt tools/fmt/firnfmt.fi
/tmp/firnfmt lib/fui/flex.fi | diff -u lib/fui/flex.fi -
```

The formatter pulls aligned trailing comments up against the field
(`basis: i64, // flex-basis ...` instead of a column) and **de-indents
continuation lines** -- a wrapped expression comes back at the
indentation of the statement above it, which reads like a new
statement. On `flex.fi` that is the whole diff: 24 lines, not one of
them an improvement. Running `firnfmt -w` over the tree would trade
readable code for a green check, and this tree values the first more.
That is a decision about the formatter, not about the code, and it is
why long-standing files like `lib/fui/core.fi` have never been
formatted either. The fifth failure, round 95, **is** fixed -- see
section 4e.

## 4e. Rebuilding the Unicode table

```sh
bash tools/ucd/build.sh --verify
```

Expected: `identical, octet for octet (100862 octets)`. Without
`--verify` the script **overwrites** `lib/generated/unicode_tables.fi`
(`generated/` is a symlink to it); with `--verify` it only compares.

This is the check that section 54 of `test.sh` runs. It was failing:
the licence sweep `0cb03e98` put `// SPDX-License-Identifier: MPL-2.0`
into the generated file by hand but not into the template
`tools/ucd/table_head.fi.in`, so the second build could never match the
first line again. The template carries the line now; the generated file
itself is unchanged, octet for octet.

## 5. What does NOT work, because it was not built

Honestly and completely (in detail in `ACCEPTANCE.md`):

* **Constant time -- partly implemented, the item stays open.** Built are the
  three primitives (`compiler/src/ct.rs`): `select(b, a, c)` -> `cmov` without a
  conditional jump, `barrier(x)`, `secure_zero(p, n)` (survives the
  optimizer). Proof: `tests/430_ct_select.fi` ... `tests/433_ct_secure_zero.fi`
  in three build stages, `tests/neg/ct_*.fi` (5 negative tests).
  **Not** implemented: `secret[T]`, propagation of the marking, `declassify`,
  `u128`, `mul_wide`, any effect of `#[constant_time]`. Without `secret[T]`
  there is no type check for secret data. Verifiable:
  `firnc -o /tmp/x tests/neg/int_secret_not_implemented.fi` reports
  `error: 'secret[T]' is not implemented in stage 0` with line/column.
  See `ACCEPTANCE.md` item 6.
* **GC, `Rc`/`Gc`, DOM prototype, RSS soak test** -- not implemented. Verifiable:
  `tests/neg/int_gc_not_implemented.fi`.
* **HTML5 tokenizer: built.** html5lib cases passed:
  **6,810 of 6,810 (100.00 %)** in the token stream comparison and
  **6,809 of 6,810 (99.99 %)** when the `errors` entries of the suite (parse
  error code, `line`, `col`) are compared as well
  (`harness.py --with-errors`, step 2a of `run.sh`). The single failure is
  `xmlViolation.test #0`. The XML adaptation of the four `xmlViolationTests` is
  implemented as an optional mode (counter-check `--no-xml-mode`: 6,807).
  The speed target of <= 2x is **missed**: range 2.25x-3.09x (corpus
  `html5lib`) and 5.72x-8.31x (corpus `realweb`). See section 4a.
* **`defer` / `errdefer`, inferred error set `!T`, `catch |e| { block }`**
  -- not implemented, see `SPEC.md` 14.1.error_unions F1-F10.
* **Self-hosting, package management, `comptime`/UCD table** -- open,
  see `docs/SELF_HOSTING.md` and `ACCEPTANCE.md` items 1, 5, 6.

## 6. Cleaning up

All working directories are disposable and listed in `.gitignore`:

```sh
rm -rf .test-work .opt-work .strwork .dtoa-work .testrunner-work .tokenizer-work bench/.work
```
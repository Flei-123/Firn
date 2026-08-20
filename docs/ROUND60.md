# Round 60 -- CSS: syntax, selectors, cascade

The third large piece of the browser engine, after the HTML5 tokenizer
(round 40 to 51) and the tree construction with the DOM core (round 54):
**from CSS text to the computed style per node** -- the step that comes
before layout and painting.

Everything in this round is written in Firn. The proof runs against
**foreign test data** (`css-parsing-tests` of SimonSapin, the suite behind
tinycss2, rust-cssparser and Servo) and against an **independent
implementation** (`cssselect2`), never against expectations of our own
alone.

---

## 1. The result in numbers

| Measurement | Result |
|---|---|
| `css-parsing-tests` (ten files of CSS Syntax Level 3) | **305 / 305** cases |
| own cases (selectors, cascade, inheritance, error tolerance) | **109 / 109** checks |
| selectors against `cssselect2`, 14 documents, 61,459 elements | **840 / 840** match sets equal |
| specificity against `cssselect2` | **840 / 840** equal |
| soak run, 60 s | RSS **flat**, counter check strikes |
| throughput on `testdata/realweb/` (callgrind) | **66,409 instructions per element**, 826 per byte |

All three build stages (`release-fast`, `--no-opt`, `dev-fast`) deliver the
same quotas; `tools/css/run.sh` compares them and stops on a difference.

---

## 2. What was built

```
lib/css/tok.fi            the CSS tokenizer      (css-syntax-3, section 4)
lib/css/cv.fi             the parser             (css-syntax-3, section 5)
lib/css/anb.fi            the An+B micro syntax
lib/css/decode.fi         the input byte stream  (css-syntax-3, 3.2)
lib/css/encoding_data.fi  generated tables for ISO-8859-2 and ISO-8859-5
lib/css/sel.fi            selectors and the matcher (selectors-4)
lib/css/cascade.fi        cascade, inheritance, computed values
lib/css/write.fi          the JSON form of css-parsing-tests
lib/css/parse_main.fi     the driver of the syntax proof
lib/css/style_main.fi     HTML + CSS -> computed style per node
lib/css/bench_main.fi     the measuring version without output
lib/css/soak_style.fi     the endurance run
```

### 2.1 Syntax (`tok.fi`, `cv.fi`)

Every algorithm of section 4.3 and every entry point of section 5.3 is
implemented, including the parts that are easy to skip: `unicode-range`
tokens, the `url()` special case, escapes with up to six hex digits, the
match tokens `~= |= ^= $= *=` and `||` (which exist only without a gap in
between), and the four error kinds `bad-string`, `eof-in-string`,
`bad-url`, `eof-in-url`.

**Error tolerance is the specification, not an extra.** Where the standard
says "this is a parse error", a node of kind `CV_ERROR` takes the place of
the value and the parser carries on -- the same way tinycss2 and cssparser
keep the error in the tree. That is what makes a quota against the foreign
data measurable in the first place.

Two details the test data check and which a re-implementation gets wrong
first:

* `!important` does **not** trim whitespace afterwards. The declaration
  value keeps every token, and when the important flag is recognized the
  value is **truncated at the position of the `!`**. `foo: 9000  !x` and
  `foo: 9000  !important` therefore differ in exactly one place.
* A declaration whose value contains a top-level `{}`-block together with
  other content is **invalid** inside a block's contents. That is the only
  reason `a:hover {c:1}` is a qualified rule and not a declaration with the
  value `hover {c:1}`.

### 2.2 The input byte stream (`decode.fi`)

`stylesheet_bytes.json` checks the whole chain: protocol encoding, byte
order mark, `@charset` read off the BYTES, environment encoding, UTF-8 as
the fallback -- and a BOM beats every one of them. Supported are UTF-8,
UTF-16LE, UTF-16BE, ISO-8859-2 and ISO-8859-5 with their labels from the
Encoding Standard; the two single-byte tables are generated out of the
codecs of Python (`tools/css/gen_encodings.py`), so no number is typed by
hand.

### 2.3 Selectors (`sel.fi`)

Type, `*`, `.class`, `#id`, attributes with all seven comparisons and the
`i` flag, the four combinators, grouping, and the pseudo-classes `:hover`,
`:first-child`, `:last-child`, `:only-child`, `:nth-child(An+B)`,
`:nth-last-child(An+B)`, `:root` and `:not(...)`.

**The matcher runs from right to left**, the way every engine does it: the
rightmost compound selector is tested against the element first, and only
if it fits does the walk to the left over the combinators start. The chain
therefore points to the LEFT (`Cpd.left`), and `a > b c` is stored as
`c -> (descendant) b -> (child) a`.

**What is not supported is INVALID**, not silently non-matching: a
pseudo-element, a namespace or an unknown pseudo-class makes the whole
selector fail, and with it its rule. The difference is measurable -- a
selector that quietly matches nothing would change the cascade.

`:hover` has no bit on the node. The context carries the hovered node, and
`:hover` fits every element on the path from it to the root.

### 2.4 Cascade and computed values (`cascade.fi`)

Sorted by origin and importance, then specificity, then order of
appearance. The reversal for `!important` is in there:

```
user agent  <  user  <  author  <  author !important  <  user !important  <  user agent !important
```

Inherited are `color` and `font-size`; `inherit` and `initial` work on
every property. The font size is computed FIRST, because the `em` of every
other property hangs on it: `%` of `font-size` refers to the PARENT, `em`
to the element itself, `rem` to the root.

The property set is deliberately small and honest -- 23 longhands:
`display`, `color`, `font-size`, the four margins, the four paddings and
the border in width, style and colour. The shorthands `margin`, `padding`,
`border-width`, `border-style`, `border-color` and `border` are expanded
into longhands at parse time, which is why a later `margin-left` beats
only the left value of an earlier `margin`.

### 2.5 Everything in the GC heap

The parse tree of a stylesheet, the selectors and the style table lie
completely in the GC heap. The style table is a **`GcMap` from node to
`Style`** with traced keys AND traced values -- a table like that keeps a
whole document alive as long as it is reachable itself, and a reference
count would never resolve that. `lib/css/soak_style.fi` builds the whole
chain in a loop and drops it; `tools/css/gc_style.sh` measures the RSS over
60 seconds and drives a deliberately leaking counter check alongside, which
MUST strike.

---

## 3. The proof

```
bash tools/css/run.sh          # the whole balance
bash tools/css/verify_testdata.sh --against-upstream   # the data against GitHub
bash tools/css/throughput.sh   # the instruction count per page
```

* **`testdata/css-parsing-tests/`** -- the ten files of the suite that test
  CSS Syntax Level 3, at the upstream commit
  `203ce36bffd617db7f118c551e32794561fb273d`, with the sha256 sums in
  `tools/css/testdata.sha256`. Nothing is filtered: every one of the 305
  cases counts, unsupported ones as a FAILURE. The `color_*.json` files of
  the same repository test the `<color>` grammar of CSS Color 3/4/5 -- a
  different specification and a different component; they are not stored
  here at all, so no case of a stored file is skipped.
* **`tools/css/harness_select.py`** -- the cross-check of the matcher
  against `cssselect2` (Python, from the authors of WeasyPrint). Compared
  are the SETS of matched elements and the specificity, **on the same
  tree**: the binary written in Firn prints its element table, the runner
  rebuilds exactly that tree in Python. A deviation in the tree
  construction can therefore not pretend to be a deviation in the selector
  engine -- the tree construction has its own proof
  (`tools/html/run.sh`).
* **`tools/css/cases/*.txt`** -- what `cssselect2` cannot check: `:hover`,
  the cascade over three origins, the reversal by `!important`,
  inheritance, computed values and the error tolerance on broken CSS.
* **`tests/910` to `tests/915`** -- the same decisions as programs in
  `test.sh`, so that a mistake shows up in the suite and not only in the
  tool.

### 3.1 Throughput

Measured with `valgrind --tool=callgrind`, not with the wall clock: the run
time of the same binary scatters by more than 10 % on this machine, which
cannot carry a comparison between rounds. Per page of `testdata/realweb/`
the CSS share is the difference between the full run and a run that only
parses the HTML.

| page | bytes | elements | HTML (Ir) | full (Ir) | CSS per element | per byte |
|---|---:|---:|---:|---:|---:|---:|
| hackernews.html | 34,320 | 817 | 45,902,927 | 101,591,869 | 68,162 | 1,622 |
| rustdoc_vec.html | 951,960 | 16,074 | 3,485,560,951 | 4,859,994,283 | 85,506 | 1,443 |
| w3c_html52.html | 154,701 | 4,467 | 659,833,556 | 895,727,098 | 52,808 | 1,524 |
| whatwg_parsing.html | 774,608 | 13,414 | 2,745,939,359 | 3,505,363,886 | 56,614 | 980 |
| wikipedia_de_html.html | 266,414 | 2,747 | 744,712,302 | 903,567,448 | 57,828 | 596 |
| wikipedia_en_linux.html | 978,810 | 8,293 | 5,583,928,457 | 6,052,595,000 | 56,513 | 478 |
| wikipedia_en_rust.html | 1,009,516 | 9,453 | 5,088,588,908 | 5,672,204,132 | 61,738 | 578 |
| wikipedia_en_www.html | 761,483 | 6,141 | 3,566,135,057 | 4,007,480,574 | 71,868 | 579 |
| **total** | **4,931,812** | **61,406** | | | **66,409** | **826** |

The stylesheet is the same for every page: `tools/css/ua.css` (a small user
agent sheet) plus `tools/css/bench.css` (about fifty rules built the way a
real page builds them).

**That number is high, and the reason is named instead of hidden: there is
no rule index.** Every element is tested against every rule of the
stylesheet; with about ninety selectors that is ninety attempts per
element. Engines bucket the rules by the rightmost compound (by id, by
class, by tag) and thereby look at a handful instead of all of them. That
is the single biggest lever for the next round -- and it is a pure
optimization: the result must not change, so the cross-check against
cssselect2 stays the measure for it.

---

## 4. Gaps in the language, found while building

The compiler was not touched in this round (two other rounds are working
there). What turned up is recorded here:

1. **The unary minus does not work on `f64`.** `sign = -1.0` is rejected
   with *unary '-' expects an integer type, found f64*, although SPEC 14.1
   lists the sign for `f64`. Written as `0.0 - 1.0` in
   `lib/css/tok.fi::tok_number_value`.
2. **An expression may not begin a line with an operator.** `a\n && b`
   ends the statement at the newline; the operator has to stand at the end
   of the previous line. That hits `&&`, `||` and `|` and cost three
   compile rounds.
3. **Two statements on one line are not accepted.**
   `if c { *out = X   return true }` is a syntax error; every statement
   needs its own line. With a table of forty keywords that is forty times
   five lines instead of forty times one.
4. **String literals carry their length twice.** `var w: [u8; 9] = "important"`
   plus the `9` at the call site. Every change of a text has to be made in
   two places, and `tools/english/check_lengths.py` exists exactly because
   of that. A `len_of(w)` at compile time would remove the whole class of
   mistakes. It hit about fifteen times in this round.
5. **No multiple return values.** Every function that delivers two things
   needs an out pointer (`unit_factor(..., ok: *mut bool)`,
   `parse_length(..., out: *mut f64, unit: *mut u32)`).
6. **Enumerations may not lie in a struct by value** (SPEC 14.1.types T2),
   which is why all the kinds in `lib/css/` are `u32` constants and not
   `enum`s -- and the exhaustiveness check of `match` cannot help there.
7. **The exit code of a program is masked to eight bits** (SPEC 14.1 item
   8). Known, but during debugging with numbers as exit codes it cost half
   an hour: 2,790 arrived as 230.

What worked without trouble and is worth naming: `gc class` with `GcVec`
and `GcMap` fields, the generic accesses `gcvec_append[Cv]` and
`gcmap_set_value[Style]`, `try`/`catch` on `AllocError`, and that gc class
names are program-wide -- `lib/css/` names `Gc[Chain]` and `Gc[Node]` of
`lib/browser/node.fi` without being able to write `Gc[node.Chain]`.

---

## 5. What is honestly missing

**Syntax**

* `@media`, `@supports` and `@import` are parsed as at-rules, but not
  evaluated: rules inside a conditional group do NOT apply. Everything the
  test data ask about the SYNTAX of at-rules passes; the semantics is a
  round of its own.
* Custom properties (`--x`) and `var()` are parsed as ordinary
  declarations and values, but not substituted.
* `@layer` does not exist; the cascade knows origins and importance, not
  layers.

**Selectors**

* No namespaces (`ns|a`), no pseudo-elements (`::before`), no `:is()`,
  `:where()`, `:has()`, no `:nth-of-type` and relatives, no `:empty`, and
  of the state pseudo-classes only `:hover`.
* `:not()` takes compound selectors, not complex ones with combinators.
* No rule index -- see 3.1.

**Cascade and values**

* 23 longhands. Missing is everything layout will need first: `width`,
  `height`, `float`, `position`, `font-family`, `font-weight`,
  `text-align`, `line-height`, `background`.
* Colours: 18 keywords, `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`, `rgb()` and
  `rgba()`. No `hsl()`, no `lab()`/`oklch()`, no `currentColor`.
* Lengths: `px pt pc in cm mm em rem ex ch`, `ex` and `ch` as a factor of
  0.5 instead of out of the font metrics (there are no fonts yet).
  Percentages stay percentages -- they are resolved by layout.
* No `unset`, no `revert`.
* The `style=""` attribute is not read; only stylesheets are cascaded.
* The style table has no invalidation: a change in the DOM does not mark
  anything dirty, the run computes everything anew.

**The number value in the output**

`lib/css/write.fi` writes the value of a number out of its
REPRESENTATION, normalized into a JSON number, instead of going through a
binary double and back. That is exact -- the value of a CSS number is by
definition the value of its decimal representation, and both sides of the
comparison are read by the same JSON parser. What is missing for the way
through the double is a shortest round-trip printer (Ryu/Grisu); the
computed values inside the engine ARE `f64` and are printed with three
decimal places.

---

## 6. Reproducing

```
cd /path/to/firn
export FIRNLIB=$PWD/lib
bash tools/css/run.sh              # the whole balance, about two minutes
bash tools/css/throughput.sh       # callgrind, about eight minutes
bash test.sh                       # the suite including section 9c
```

The cross-check against `cssselect2` needs the Python package of the same
name (`pip3 install cssselect2`). Without it `tools/css/run.sh` says so and
counts the comparison as not run -- it does not pretend it passed.

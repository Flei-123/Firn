# Round 69 — the comfort layer of the standard library

Branch `r69-comfort`, base commit `aa65fdc`. Nothing outside `lib/std/`,
`tools/strlib/`, `demos/`, `tests/` and `test.sh` was touched — the
translator (`compiler/src/`, `lib/firnc1/`) stayed shut, because two other
rounds are working in it at the same time.

## 0. What started this

Reading one number looked like this:

```firn
var input: rt.Buf = rt.buf_new()
if !io.read_stdin(&input) { return 1 }
let s = str.span(rt.buf_ptr(&input) as *mut u8, rt.buf_len(&input)).trim()
var x: i64 = 0
if !num.text_to_i64(s.p, s.n, &x) { ... }
```

In C# that is `var text = Console.ReadLine().Trim();`. Everything needed was
in the library already. It was uncomfortable, and it disagreed with itself
at the seams: `rt` treats an address as a `u64`, `str` and `num` treat it as
a `*mut u8`, so every crossing cost the caller a cast; a text was a `Span`
in one half of the library and a loose `p, n` pair in the other; and a
parser reported its result through an output pointer although Firn can
return structs.

Afterwards:

```firn
let n: num.ParsedI64 = io.ask_i64(f"Enter a number: ")
if !n.ok { ... }
```

## 1. The numbers first

| measurement | before (`aa65fdc`) | after (`r69-comfort`) |
|---|---|---|
| `bash test.sh`, cases in total | 961 | **980** |
| — of which passed on this machine | 958 | **977** |
| — of which failed | 3 (see below) | 3 (**the same three**) |
| — new test programs (6 × opt/noopt/dev-fast) | — | +18 |
| — new section in `test.sh` | 1…26 | + section **27** |
| `bash tools/self_compare.sh` | 0 differing / 0 faulty | **265 same behaviour, 0 DIFFERING, 0 FAULTY** |
| `bash tools/fixpoint.sh` | stage 2 = stage 3 | **identical, character for character** (3 375 056 octets, 573 656 lines of assembly) |
| `bash tools/english/check.sh` | 0 0 0 **828** 0 | 0 0 0 **828** 0 (see 8.) |
| — the same without the wrongly committed `.js-work/` | 0 0 0 0 0 | **0 0 0 0 0** |
| `firnfmt -c` over every new/changed source | — | **canonical, 0 complaints** |
| `demos/number_check.fi`, total lines | 117 | **51** |
| `demos/number_check.fi`, lines of code | 95 | **35** |
| — of which in `main` | 77 | **24** |
| — pointer casts (`as *mut u8` / `as u64`) in it | 17 | **0** |
| — `[u8; N]` text literals in it | 12 | **0** |
| — `rt.Buf` handling in it | 7 places | **0** |
| input soak, 200 000 lines through `io.read_line` | — | RSS **34 -> 34 pages**, drift 0 |
| the counter-check (the same program without `free`) | — | RSS **10 034 -> 40 034 pages**, +1 page per line |

**The three failures are not this round's, and they are the same three
before and after.** Both were reproduced with the untouched base commit:

* `tests/860_thread_basic.fi` [opt] and [dev-fast] — its check C says "a
  counter WITHOUT a lock HAS to lose increments, otherwise the threads did
  not really run at the same time". With three rounds building in parallel
  the load average sat at 10.7 on 8 cores, the four threads got serialised,
  the racy counter came out exact and the counter-check reported the
  measurement as broken (exit 14). Measured side by side, 40 runs each, at
  the same moment: **branch 25/40 failed, untouched `aa65fdc` 30/40 failed**
  — the same binary behaviour, the flake belongs to the machine, not to the
  change. (This round does not touch threads; `tests/860` imports no `std`
  module at all.)
* `tools/english/check.sh` — 828 German path names, all of them out of the
  `.js-work/` work directory that commit `aa65fdc` swept into git. Section 8
  has the details and the one-line fix. `check.sh` exits 1 at `aa65fdc`
  itself.

Library growth: `lib/rt/rt.fi` +33 lines (the typed address twins),
`lib/std/io.fi` +295, `tools/strlib/src/num_comfort.fi` 160 new,
`tools/strlib/src/std_num.fi` +8. New test programs: 888 lines.
New tool: `tools/strlib/comfort/` (soak 95, run.sh 157).

## 2. One address form: `*mut u8`

**The decision: outward an address is a `*mut u8`, never a bare `u64`.**

Three reasons, in this order.

1. **The bigger half of the library is already like that.** `str` (Span,
   Bytes, every search and trim) and `num` (every `read_*`, `text_to_*`)
   take `*mut u8`. Only `rt` uses `u64`, and `rt` is one file. Choosing
   `u64` would have meant rewriting `lib/str` and `lib/num` — and those two
   are include libraries that four other modules pull in textually.
2. **A type can be wrong, a number cannot.** `*mut u8` says WHAT lies at the
   address; `u64` says "some 64 bits". Handing a length where an address
   belongs is a compiler error in the first spelling and a segmentation
   fault in the second.
3. **It is the form the caller already holds.** `&literal[0]` is a
   `*mut u8` (no cast needed — that is worth knowing, several places in the
   tree write `(&x[0]) as *mut u8` for nothing), and `Span.p` is a
   `*mut u8`. Every extra `as u64` in application code existed only because
   `rt` asked for it.

`rt` therefore gained three functions and lost none:

```firn
fn ptr_of(a: u64) -> *mut u8        // number  -> typed address
fn addr_of(p: *mut u8) -> u64       // typed address -> number (syscall!)
fn buf_data(b: *mut Buf) -> *mut u8 // the contents of a buffer, typed
```

`rt.buf_ptr` stays and is now a **thin shell** over `buf_data`
(`return addr_of(buf_data(b))`) — every one of its existing callers keeps
working, and there is no second place where the address of a buffer is
read. The `u64` forms of `print`, `read_file` and friends stay untouched
next to the new `Span` forms.

`io` is the only module that sees both worlds, so **every crossing happens
there** — `io.span_of_buf`, `io.write`, `io.write_line`, `io.write_err`,
`io.write_err_line`, `io.write_c`, `io.write_c_line`, `io.fmt_span`. Not one
of them asks the caller for a cast.

`lib/str` was not touched at all. That is the point of the decision: the
side that was already right did not have to move.

## 3. Input: who owns the octets

Firn has no global variables (SPEC 14.1, item 5) and the std facade has no
garbage collector. A `read_line()` that returned a bare `Span` would
therefore be a **lie**: nobody could release the memory behind it, and after
a `trim()` the span does not even point at the start of the block any more,
so it cannot be handed to `heap_free`. So the owner is returned:

```firn
struct Text { b: rt.Buf, ok: bool }
```

* **`Text` owns.** It is the only thing that has to be released.
* **`Span` owns nothing.** It is a view INTO a `Text` and is valid exactly
  as long as that `Text` lives and is not written to again.

```firn
var line: io.Text = io.read_line()
defer line.free()
let s: str.Span = line.span().trim()
```

`ok` is false only at the END OF INPUT. An empty line is `ok` with length 0
— that is the `Console.ReadLine() == null` of C#, and without the
distinction a program cannot tell "" apart from "the input is over".

New in `lib/std/io.fi`:

| function | does |
|---|---|
| `read_line() -> Text` | one line, without `\n`, a `\r` in front of it dropped |
| `read_all() -> Text` | everything up to the end |
| `prompt(f: Fmt) -> Text` | write the question (no line feed), read a line |
| `text_of(f: Fmt) -> Text` | an `f"..."` as an owned text — the way to a `Span` from a literal |
| `text_span` / `text_length` / `text_is_ok` / `text_free` | plus the methods `.span()`, `.length()`, `.is_ok()`, `.free()` |
| `read_i64()` / `read_f64()` | read, trim, parse, release — the caller owns nothing |
| `ask_i64(f)` / `ask_f64(f)` | the same, with a question in front |

**`read_line` does one `read(2)` per octet, and that is deliberate.** A
buffered reader would have to keep the octets it read too much somewhere,
and without global variables there is no such place — it would swallow input
that belongs to whoever reads next. Correctness before speed at this seam;
whoever reads MANY lines takes `read_all()` plus the `LineReader` cursor,
which reads in 64 KiB blocks. The soak run below shows what it costs:
200 000 lines in a fraction of a second, with a flat RSS.

## 4. Numbers without an output parameter

`tools/strlib/src/num_comfort.fi` (new, generated into `lib/std/num.fi`):

```firn
struct ParsedI64 { ok: bool, overflow: bool, value: i64 }
struct ParsedU64 { ok: bool, overflow: bool, value: u64 }
struct ParsedF64 { ok: bool, value: f64 }

fn parse_i64(s: str.Span) -> ParsedI64
fn parse_u64(s: str.Span) -> ParsedU64
fn parse_u64_base(s: str.Span, base: u32) -> ParsedU64
fn parse_u64_auto(s: str.Span) -> ParsedU64      // 0x / 0b / 0o
fn parse_f64(s: str.Span) -> ParsedF64
fn i64_or / u64_or / f64_or (+ the method .value_or(fallback))
fn parsed_i64_none / parsed_u64_none / parsed_f64_none
```

**Nothing is reimplemented.** `parse_*` calls exactly the `read_*` core that
`text_to_*` calls; `tests/1281` holds both against each other on the same
inputs so that the two cannot drift apart. The overflow honesty of the old
forms is kept — it just moved from a second output pointer into a field.

For this, `std.num` now **imports `str`**: a piece of text is a `Span`, not
two loose parameters. `std.io` imports `str` and `num`. That is the honest
dependency direction (`io` -> `num` -> `str`, no cycle), and it is also what
item 4 needs: an `io` that is supposed to print an `f64` has to be able to
reach `dtoa`.

Price, measured: a program with `import std.io` alone compiles in 52 ms; with
the whole chain pulled in it is 359 ms. That is the cost of the convenience
and it is paid by the sixteen files in the tree that use `std.io`; the
self-hosted compiler does not use `std.io` and is not affected at all.

## 5. Text literals as a Span — and what only the compiler can do

Best available **without** a compiler change, in this order:

1. **For output there is no problem at all: `f"..."`.** An interpolation
   literal turns into `fmt_text` calls at compile time — no array, no
   length, no cast. `io.fmt_print_line(f"parity    : even")` is the whole
   line. Round 69 uses that everywhere and adds `io.prompt(f"...")`,
   `io.ask_i64(f"...")`, `io.fmt_span`, `io.fmt_f64` so that `f"..."` is
   also the way to pass a text INTO a function.
2. **For a Span from a literal: `io.text_of(f"...")`.** One call, one
   allocation, and `.span()` gives the view. `tests/1281` uses it for two
   dozen literals and it reads well.
3. **Allocation-free: `str.span_of_c(&literal[0])`.** Worth writing down,
   because the tree does not know it: `&a[0]` ALREADY has the type
   `*mut u8`, the `as *mut u8` that several places add is superfluous. What
   remains is the `var a: [u8; N] = "...\0"` with its hand-counted length.

**What only the compiler can fix** (found while doing this, all verified,
none of it touched):

* **A string literal has no inferable type.** `var a = "abc"` is rejected
  with "the type of the array literal cannot be inferred". As long as that
  holds, every literal Span needs a `var` line with a counted `[u8; N]`.
  The smallest useful change would be to infer `[u8; N]` for a string
  literal in a `let`/`var` without a type.
* **A string literal is not an expression of its own.** It only exists as an
  array initialiser, so `f(&"abc"[0])` is impossible (a literal is not an
  lvalue). The real fix is the one the SPEC already sketches: literals in
  `.rodata` and a literal type that IS a `Span`.
* **`f"{x}"` casts everything to `i64`** (`compiler/src/parser.rs`, section
  interpolation). `{w}` for an `f64` prints a truncated integer, `{b}` for a
  bool prints 0/1. The library side is now ready: `io.fmt_f64` and
  `io.fmt_bool` exist, are exported and are tested (`tests/1280`) — the
  compiler round only has to pick the right one by the static type of the
  expression instead of `fmt_number`. Until then the honest spelling is
  `io.fmt_print_line(io.fmt_f64(f"root      : ", w))`.
* **An `f"..."` may not stand directly in the CONDITION of an `if`.** Its
  hidden `_fsegN` arrays are hoisted IN FRONT OF the statement while the
  condition is evaluated before the body, so `if as_i64(f"12x").ok {` fails
  with "unknown name '_fseg236'". Known since round 42
  (`docs/ROUND42.md`); round 69 makes it common, because `f"..."` is now an
  argument all the time. An intermediate `let` works around it.

## 6. `io.fmt_bool` says `true`/`false`

It said `wahr`/`falsch` — a leftover from before the English migration. A
library that prints a bool in a different language than the source is
written in is a seam, not a feature. `tests/806_std_io_core.fi` follows
(the expected length 69 became 68, the octet offsets shifted by one, and the
first octet of the word is now `t` instead of `w`).

## 7. Leak proof for the new input layer

`tools/strlib/comfort/soak.fi` reads lines with `io.read_line()` and reads
its own RSS out of `/proc/self/statm` — with the comfort layer itself
(`io.span_of_buf`, `str.divisor_whitespace`, `num.parse_u64`), so the soak
run tests the thing it measures with.

**The counter-check is in the same program.** The first line of the input
chooses `free` (every `Text` is released) or `keep` (none is). One loop, one
difference: the missing `free`. A measurement that never strikes proves
nothing.

```
   demo: demos/number_check.fi, 4 inputs, output identical
   soak: 200000 lines, RSS 34 -> 34 pages (drift 0, limit 64)
   counter-check strikes: without free RSS 10034 -> 40034 pages (+30000 over 40000 lines)
```

In `test.sh` the short version runs (60 000 / 20 000 lines): `RSS 34 -> 34`,
counter-check `10033 -> 20033`.

Flat over 200 000 lines; the leaking variant grows by exactly one page per
line (the smallest `Buf` is one `mmap`). `tests/1284` runs the cheap version
of the same idea inside the test suite: 2 000 rounds of `text_of`/`free` and
200 rounds of `read_line`/`free` have to land on the SAME address as the
first one — with a leak the mapping would wander off.

`tools/strlib/comfort/run.sh` also runs `demos/number_check.fi` itself with
four inputs and compares its WHOLE output. The demo reads from the real
standard input and can therefore not live in `tests/`; this is where it gets
proven. Both hang in `test.sh` as **section 29** (numbers 1…26 were taken;
checked before hanging it in, as asked).

## 8. The 828 that are not mine

`tools/english/check.sh` reports `German path names: 828` — **in the base
commit `aa65fdc` just as much as on this branch**. The cause has nothing to
do with this round: the commit `aa65fdc` swept the work directory
`.js-work/` into git (32 962 files, 140 MB — the foreign test262 checkout
plus comparison artifacts of the JavaScript rounds), and `.js-work/` is
missing from `.gitignore`. `check_names.py` then reads names like
`elements-added-after.js` and finds the morpheme `element` in them.

Measured with the directory excluded — exactly the way `testdata/` is
already excluded as foreign data — the check gives **0 0 0 0 0**, on this
branch as well. **This branch adds zero hits**; the two comment lines that
this round did produce (they quoted the German words `wahr`/`falsch`) are
fixed.

The fix is one line and does not belong in this round, because a JavaScript
round is working in exactly that place right now:

```sh
printf '.js-work/\n' >> .gitignore && git rm -r --cached .js-work
```

## 9. What is new, file by file

| file | |
|---|---|
| `lib/rt/rt.fi` (= `lib/std/rt.fi`) | `ptr_of`, `addr_of`, `buf_data`; `buf_ptr` becomes a shell |
| `lib/std/io.fi` | the whole comfort layer, see 2. and 3.; `fmt_bool` in English |
| `tools/strlib/src/num_comfort.fi` | new — the `Parsed*` structs and `parse_*` |
| `tools/strlib/src/std_num.fi` | `import str` + the new include |
| `lib/std/num.fi` | regenerated (`tools/strlib/expand.py --all`), never edited by hand |
| `demos/number_check.fi` | rewritten, see 10. |
| `tests/1280_std_io_span.fi` | the bridge, output as a Span, `fmt_span`/`fmt_f64`/`fmt_bool` |
| `tests/1281_std_num_parse.fi` | `parse_*` at its edges + the counter-check against `text_to_*` |
| `tests/1282_std_io_read_line.fi` | `read_line`/`read_all`, DOS line ends, empty line vs. end of input |
| `tests/1283_std_io_ask.fi` | `prompt`/`ask_i64`/`ask_f64`/`read_i64` end to end |
| `tests/1284_std_io_text_owner.fi` | the ownership contract + the cheap leak indicator |
| `tests/1285_std_io_number_check.fi` | the demo's chain over six numbers, text compared |
| `tests/806_std_io_core.fi` | follows the `fmt_bool` change |
| `tools/strlib/comfort/soak.fi` | the endurance run with its counter-check built in |
| `tools/strlib/comfort/run.sh` | demo + soak + counter-check, hangs in `test.sh` as 27 |

Test numbers 1280–1285 out of the reserved 1280–1329; 1286–1329 stay free.
Section numbers in `test.sh`: 1…26 were taken, 27 is new.

## 10. Before and after

**Before** (`demos/number_check.fi` at `aa65fdc`, 117 lines / 95 lines of
code, `main` 77 lines of code, 17 pointer casts, 12 `[u8; N]` literals):

```firn
fn trim(p: u64, n: usize, ap: *mut u64, an: *mut usize) {
    var a: usize = 0
    var e: usize = n
    while a < e && (rt.ld8(p, a) == 32 as u8 || rt.ld8(p, a) == 9 as u8
        || rt.ld8(p, a) == 10 as u8 || rt.ld8(p, a) == 13 as u8) {
        a = a + 1
    }
    ...
}

fn main() -> i32 {
    var question: [u8; 16] = "Zahl eingeben: \0"
    io.print_c((&question[0]) as u64)

    var ins: rt.Buf = rt.buf_new()
    if !io.read_stdin(&ins) {
        var f1: [u8; 15] = "Eingabe fehlt.\0"
        io.print_c((&f1[0]) as u64)
        io.print_line()
        return 1
    }

    var p: u64 = 0
    var n: usize = 0
    trim(rt.buf_ptr(&ins), rt.buf_len(&ins), &p, &n)

    var x: i64 = 0
    if n == 0 || !num.text_to_i64(p as *mut u8, n, &x) {
        var f2: [u8; 20] = "..."     // a German text literal, 20 counted octets
        io.print_c((&f2[0]) as u64)
        io.print_line()
        return 1
    }
    ...
    if even {
        var t: [u8; 20] = "Parität   : gerade\0"
        io.print_c((&t[0]) as u64)
        io.print_line()
    } else {
        var t2: [u8; 22] = "Parität   : ungerade\0"
        io.print_c((&t2[0]) as u64)
        io.print_line()
    }
    ...
    var b: num.Bytes = num.Bytes { ptr: 0 as *mut u8, len: 0, cap: 0 }
    num.write_f64(&b, w)
    io.println(b.ptr as u64, b.len)
    num.bytes_free(&b)

    rt.buf_free(&ins)
    return 0
}
```

**After** (51 lines / 35 lines of code, `main` 24 lines of code, 0 pointer
casts, 0 `[u8; N]` literals, 0 `rt.Buf`): the full text is in
`demos/number_check.fi`; its core is

```firn
fn main() -> i32 {
    let n: num.ParsedI64 = io.ask_i64(f"Enter a number: ")
    if !n.ok {
        io.fmt_print_line(f"That is not a whole number.")
        return 1
    }
    let x: i64 = n.value
    let size: u64 = math.abs(x) as u64

    io.fmt_print_line(f"number    : {x}")
    io.fmt_print_line(io.fmt_append(f"parity    : ",
            either(x % 2 == 0, f"even", f"odd")))
    io.fmt_print_line(io.fmt_append(f"sign      : ",
            either(x > 0, f"positive", either(x < 0, f"negative", f"zero"))))
    io.fmt_print_line(io.fmt_append(f"over 100  : ",
            either(x > 100, f"yes", f"no")))
    io.fmt_print_line(f"digits    : {num.digit_count(size, 10)}")
    io.fmt_print_line(f"square    : {x * x}")
    if x < 0 {
        io.fmt_print_line(f"root      : not real (number < 0)")
    } else {
        io.fmt_print_line(io.fmt_f64(f"root      : ", math.sqrt(x as f64)))
    }
    return 0
}
```

The only `as` left over turns a number into a number.

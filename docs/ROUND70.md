# Round 70 -- the comfort round: `str`, the second spelling, the default type, `f"..."`

Branch `r70-str`, base commit `d4317c1c` (the merge of round 65 and round 68).

## Why this round exists

Justin looked at `demos/number_check.fi` and complained, rightly, that Firn is
more long-winded than C# for the same program. He was right: reading one line
and describing it took 117 lines, of which about 40 were nothing but
bookkeeping -- byte arrays with a length counted by hand, a `trim` written out
by hand, `print_c` with a null octet.

The same program now takes 55 lines and reads like C#. **Without the language
losing any of its closeness to the machine**: a `str` is still exactly two
words, and whoever wants the pointer and the length writes `s.p` and `s.n`.

## The numbers of the acceptance

Everything below was really run, in this worktree, on this machine.

| what | measured |
|---|---|
| `bash test.sh` | **PASS 1052 / 1052**, 0 failed (base: 977 / 977) |
| `bash tools/self_compare.sh` | 281 same behaviour, **0 differing, 0 faulty**, CODEGEN MISSING **0** |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical, 602936 lines of assembly** (3539464 octets each) |
| lexer comparison (11) | SAME 606, DIFFERENT 0, 862260 tokens |
| parser comparison (12) | SAME 356, DIFFERENT 1 (known and named: 1) |
| layout/ABI comparison (13) | SAME 303, DIFFERENT 0 |
| type check comparison (14) | SAME 173, DIFFERENT 0 |
| lowering comparison (15) | SAME 170, DIFFERENT 1 (known and named: 1) |
| `bash tools/kernel/run.sh` | **174 passed, 0 failed** |
| `bash tools/freestanding/run.sh` | **41 passed, 0 failed** |
| `bash tools/english/check.sh` | **0 0 0 0 0** |
| `bash tools/lexnum/run.sh` | 4044 float + 1250 integer literals, 0 differing everywhere (unchanged) |
| `bash tools/fmt/run.sh` | 649 files, 0 changed by the shape, 0 tree deviations |
| `bash tools/packages/run.sh` | 21 passed, 0 failed |
| `bash tools/dwarf/run.sh` · `tools/lsp/run.sh` | 48 / 0 · 25 / 0 |
| `bash tools/layout/run.sh` | 705 / 705 boxes, deviation 0.00 % against Chromium |
| `bash tools/strsoak/run.sh` | see below |

**No new deviations** in the five comparison tools: the one in the parser
comparison and the one in the lowering comparison are the ones that were
already known and named before this round.

### The leak proof (`tools/strsoak/run.sh`, test.sh section 31)

200000 rounds with 8 concatenations each = 1.6 million short lived strings,
4000000 octets built, in BOTH compilers:

```
firnc0 collected : rss_peak_kib=1452    runs=219  heap_kib=1280
firnc0 leaking   : rss_peak_kib=221588  runs=0    heap_kib=225280
firnc1 collected : rss_peak_kib=1620    runs=219  heap_kib=1280
firnc1 leaking   : rss_peak_kib=221760  runs=0    heap_kib=225280
```

With the collector the resident memory stays at **1.4 MiB** and does not
move; with the collection threshold set to infinity the same loop grows to
**217 MiB**. The counter-check is not decoration -- it is what makes the
green result worth anything, and the script fails if the counter-check stays
flat too.

## 1. `str` -- the language type

### What it is

Two machine words, `p: *mut u8` and `n: usize` -- the layout of `str.Span`
down to the octet. That is the whole trick of this round: the string library
of round 42 (135 functions, `lib/std/str.fi`) does not have to be written a
second time. `str` is the name the LANGUAGE gives to the view the LIBRARY
already had.

```firn
let text: str = io.read_line().trim()
if text == "quit" { ... }
let greeting: str = "hello, " + text
```

* **Immutable.** No operation writes into the octets behind a `str`.
* **Substrings cost nothing.** `trim`, `part`, `ab`, `to` move two words.
* **`==`/`!=` compare the content**, `+` concatenates.
* **All 135 library functions work as methods** -- `s.trim()`, `s.length()`,
  `s.starts_with(...)`, `s.find(...)`, `s.contains(...)` -- through the
  existing `impl Span` blocks (round 45), without a conversion function and
  without a copy.

### The literal -- and why nothing breaks

About a thousand places in this source tree say `var t: [u8; 20] = "...\0"`.
Not one of them changed. The rule is: **the context decides.**

* an array type is wanted -> it is the array literal it has been since round
  39, with the same length check and the same messages;
* nothing else says anything -> it is a `str`.

That cannot change the meaning of any existing program, and the reason is
worth writing down: a text literal WITHOUT an array context is an **error**
today ("the type of the array literal cannot be inferred"). There is nothing
that could break.

`b"..."` and `u"..."` stay array literals: `str` holds octets, and a sequence
of `u16` is not that (`tests/neg/str_wide.fi`). `""` is a valid, empty `str`;
as an array literal it stays an error (`tests/neg/str_empty_array.fi`).

### Where the octets live -- what is copied and what only points

| origin | storage | copy? | freed by |
|---|---|---|---|
| literal `"hello"` | frame of the enclosing function | no | the frame |
| `a + b` | GC heap (`__str_concat`) | **yes**, both sides | the collector |
| `s.trim()`, `s.part(..)` | unchanged, where they were | **no** | the owner |
| `Span`/`Bytes` -> `str` | unchanged | **no** | the owner |
| `str` -> `Span` | unchanged | **no** | the owner |
| `io.read_line()` | GC heap (`__str_copy`) | **yes**, once | the collector |

There are exactly **two** places in the whole round where octets are copied:
`__str_concat` and `__str_copy`. Everything else moves two words.

`str` and `Span` may be used for each other because they have the same shape:
`sema::compatible` lets exactly the pair "builtin `str`" and "a struct
`{ *mut u8, usize }`" pass (`strtype::same_view`). Two ordinary structs of that
shape stay separate from each other -- only `str` gets the privilege.

### Why the collector, and when it is pulled in

The result of `a + b` outlives both operands, so it needs an owner. The only
owner in this language that nobody has to name is the collector (SPEC 3.5).
`__str_eq`, `__str_copy` and `__str_concat` therefore live in `lib/gc/gc.fi`,
next to `__gc_alloc_raw`; the octets get a class of their own (`gc class
StrBytes`) that has no field of pointer type, so the collector traces nothing
inside such a block.

The runtime is pulled in automatically. The signal is read off the TOKENS,
exactly like the one for `gc class`:

1. the identifier `str` NOT next to a `.` -- that is the type name
   (`let s: str`, `-> str`, `str { … }`). `import std.str` and `str.trim(x)`
   are excluded by the dot;
2. a text literal directly next to `+`, `==` or `!=`.

What deliberately does NOT trigger: `var t: [u8; 20] = "…"` and `asm("…")`.
That is why **the kernel and the freestanding profile see nothing of this
round** -- they use text literals only as arrays.

**The honest limit of this trigger:** a program that concatenates two `str`
variables without ever writing the type name and without a literal next to the
operator is not seen at the token level. It then gets a message that says what
to do about it ("write the type down once, e.g. `let s: str = …`"), not a
mysterious linker error. In practice that case does not occur, because a `str`
has to come from somewhere.

**`gc_init()` before the first concatenation.** `==` does not allocate and
needs nothing; `+` and `io.read_line()` do, and they follow the same rule as
every other allocation in this language.

## 2. The second spelling of the primitive types

`sbyte short int long byte ushort uint ulong double` are a **second name, not
a second type**. `int` and `i32` pass into each other without a cast, and
`impl Ord for int` creates the very same `i32__less` as `impl Ord for i32` --
the name is folded onto the canonical one at the ONE place where it enters a
function name (`impls::impl_decl`, `parser.fi::canon_alias`).

The canonical form in this repository stays `i32`/`i64`/`u8`; the ~47000
existing places were not renamed, and error messages keep naming the canonical
form.

Two promises, written into SPEC 13 with their reasons:

* **`int` is ALWAYS 32 bits, `long` ALWAYS 64.** In C/C++ `long` is 32 bits on
  Windows and 64 on Linux; the same source text computes differently depending
  on where it is translated. That very trap is the reason why Firn writes
  `i32`/`i64`. The second spelling inherits the fixed width, not the
  ambiguity.
* **`byte` is UNSIGNED (0..255), `sbyte` is `i8`.** A byte is a storage unit --
  an octet --, not a number one calculates with. C#, Go, Rust and Zig see it
  that way; Java's signed `byte` is the outlier and came out of a lack, not out
  of a decision (Java has no unsigned types). The stock says the same: `u8`
  occurs 6992 times, `i8` 13 times.

`float` is deliberately not given out. `f32` arrives in round 71; only then
does `float` mean something that can be kept.

## 3. The default type of an integer literal

`var x = 5` was the most often named piece of discomfort. From now on: where
the context says something, that holds; where nothing at all says anything,
`i32` holds -- as in C#, Java and Go.

Two things deliberately did NOT change:

* the literal still adapts itself to a demanded type: `let y: long = 5` is a
  64-bit value;
* the overflow check does not soften. `let x = 5000000000` is an error, with
  the same message as at an explicit `i32` plus the note that the wider type
  has to be written down (`tests/neg/literal_default_overflow.fi`).

Where the context is no integer type at all, the message stays "the type of
the integer literal cannot be inferred" -- the default must not jump in there,
otherwise `let p: *mut u8 = 5` would turn into a confusing follow-up error
(`tests/neg/untyped_literal.fi`).

## 4. `f"..."` with all types

Until now the interpolation cast EVERY inserted value to `i64`
(`parser.rs: io.fmt_number(chain, expr as i64)`). An f64 was therefore
truncated (`1.5` -> `1`), a `bool` became `0`/`1`, and a `u64` above
`i64::MAX` wrapped around into the negative.

The parser now writes `io.fmt_value(chain, x)`, and the TYPE decides which
builder step is really taken:

| type | step |
|---|---|
| signed integer | `io.fmt_number` (widened to i64) |
| unsigned integer | `io.fmt_u64` |
| `bool` | `io.fmt_bool` |
| `f64` | `io.fmt_f64` (shortest decimal text with a round trip guarantee) |
| `str` | `io.fmt_str` |

The type check and the lowering derive the target from the SAME material
(`sema::fmt_target`, `lower::fmt_target`) -- no side table between the phases,
the same build as for the method resolution of round 45.

One widening had to be allowed for it: an `i32` argument into the `i64`
parameter of `io.fmt_number`. It holds for exactly that one argument of
exactly that one call (`Checker::widen`), so no general implicit conversion
comes into being through the back door.

**Round 69 (`r69-comfort`) had no commits at the start of this round**
(`git log r69-comfort` = the state of the base), so there was nothing to
cherry-pick; `io.fmt_f64` was built here.

## What was changed in `lib/std/` -- for the merge with round 69

Round 69 owns `lib/std/`, `lib/str/` and `tools/strlib/`. The intervention was
kept as small as possible; it touches exactly ONE file:

**`lib/std/io.fi`** -- four additions and one correction:

1. `import num` at the top. `fmt_f64` needs the shortest decimal text with a
   round trip guarantee, and that is `num.write_f64` (dtoa), not something
   written a second time.
2. `fmt_value(f: Fmt, v: i64) -> Fmt` -- the placeholder of the interpolation.
   The body is what remains when somebody calls the name by hand with an
   integer.
3. `fmt_str(f: Fmt, s: str) -> Fmt`, `fmt_f64(f: Fmt, x: f64) -> Fmt`,
   `print_str(s: str)`, `println_str(s: str)`.
4. `read_line() -> str` -- one line from standard input without the line
   break. The octets are copied into the GC heap (`__str_copy`), so the result
   survives the buffer and nobody has to free it.
5. **The correction:** `fmt_bool` printed `wahr`/`falsch`. It was the only
   place in the whole project where a German word came OUT of a Firn program;
   it was never visible, because until now the interpolation cast every value
   to `i64` and never reached the function. It now prints `true`/`false`.

The export list grew by exactly these names. Nothing else in `lib/std/` was
touched.

**Where it can clash on merging:** the import line at the top of `io.fi`, the
export block, and the end of the file (the new functions are appended). If
round 69 has built its own `io.fmt_f64` in the meantime, keep ONE of the two
and delete the other -- the callers only need the name.

Beyond that: `lib/gc/gc.fi` (the three `str` runtime functions, `gc class
StrBytes`, `gc_set_limit`, the state slot `S_STR_TID = 2144`) and, generated
from it, `lib/firnc1/gctext.fi` (`tools/gen_gctext.sh`).

## 5. Compound assignment and the step operators

An addendum to the round, asked for while it was running -- and it fits: it
is pure parser work on the same theme.

```firn
x += 5   x -= 5   x *= 5   x /= 5   x %= 5
x &= m   x |= m   x ^= m   x <<= 3  x >>= 3
x++      x--
```

### `x op= e` is exactly `x = x op e`

Not almost, exactly. Both go through the SAME type check: the rules that
`binary` used to carry inside itself now live in `sema::binop_type`
(`binop_type` in `sema.fi`), and both callers compute with them. That is why
`x %= 2.0` on an f64 gives the very same message as `x = x % 2.0`, and why
`x += y` with i32 and i64 fails at exactly the same place.

Lowering produces the same FIR operation as well -- so the overflow
behaviour is not "the same by intent" but the same instruction.

### The left side is evaluated ONCE

This is the classic mistake of the extension: whoever implements
`a[f()] += 1` by rewriting it in the parser into `a[f()] = a[f()] + 1` gets
a program that calls `f()` twice. `tests/1338_assign_op_once.fi` counts:

| written | calls of `f()` |
|---|---|
| `a[f()] += 5` | **1** |
| `a[f()]++` | **1** |
| `*f() += 11` (a function that hands out a pointer) | **1** |
| `a[f()] = a[f()] + 5` (the counter-check) | **2** |

The last line is not decoration: without it the test would stay green even
if the counting were broken.

`str` needed a small mechanism for it. `s += x` passes its target to
`__str_concat` as an ARGUMENT and writes the result back into the same
place; without a pin the address would be computed a second time for the
argument. That is what `pinned` (`lower.rs`) resp. `pin_set` (`lower.fi`)
is for -- an lvalue whose address is already known.

### `++` and `--` are statements, never expressions

`y = x++` does not exist here, and neither does `a[i++]`. Written on a line
of its own, `x++` is unambiguous; inside an expression it is one of the most
productive sources of error in C -- `*p++` versus `(*p)++` -- and in C++ the
order of evaluation around it is even undefined (`i = i++ + 1` has no
meaning). The parser reports it directly instead of leaving a puzzle:

```
error: '++' is a statement, not an expression
  --> tests/neg/step_in_expression.fi:8:19
   = note: write it on a line of its own; there is no 'y = x++' here,
           because prefix and postfix inside an expression are a source of error
```

Both forms are caught -- the postfix one at the end of the statement, the
prefix one where an expression should begin.

### What the lexer had to learn

Twelve new tokens, and `<<=`/`>>=` need THREE characters of lookahead -- read
two at a time they would fall apart into `<<` and `=`. `++`/`--` are read
greedily; `a - -b` keeps two tokens because of the blank between them. The
whole existing source tree was checked for `--` outside comments and strings
before: there is none.

### The formatter knows the new shapes

`tools/fmt/fmt.fi` has a scanner of its own and would otherwise have split
`+=` into `+` and `=` and printed `x + = 1` -- a different token stream that
does not scan any more. It now knows the twelve signs, sets blanks around
`+=` like around `=`, and lets `x++` stick to its operand. **It does not
rewrite anything**: `x += 1` stays `x += 1`, and the canonical form
(`ast_canon.rs`, `print.fi`) prints `(opassign + (id x) (int 1))` and
`(step ++ (id x))` instead of hiding the difference behind `zuw`.

### What is deliberately not in it

`+=` and `++` inside a `comptime` block. The interpreter of `comptime.rs`
knows only assignments to a local variable; instead of computing something
wrong it refuses them with a message.

## What this round found in passing

* **`bin/astdump.fi` cannot parse `size_of[T]`.** The first version of
  `tests/1334_type_aliases.fi` proved the widths with `size_of`, and all four
  tree comparison tools reported a deviation. The gap is older than this round
  -- no test in the corpus had used `size_of` before. The test now proves the
  widths by truncation, which says more anyway; the gap in `astdump` stays
  open and is written down here.
* **`io.fmt_bool` printed German.** `wahr`/`falsch` was the only place in the
  whole project where a German word came OUT of a Firn program. It was
  invisible until now, because the interpolation cast every value to `i64` and
  never reached the function. Now it says `true`/`false`; four expected
  outputs in the corpus were pulled along.
* **`bin/lexdump.fi` carries the token names as a byte array of FIXED
  length.** The twelve new tokens of part 5 came out of the lexer in Firn
  without a name, and the lexer comparison struck immediately (8 deviations).
  The table went from 460 to 553 octets -- exactly the trap this project has
  written down since round 65.
* **The collector runtime lives in the root namespace.** As soon as a program
  pulls it in, the constants of `lib/gc/gc.fi` are program wide.
  `tests/806_std_io_core.fi` declared `SYS_CLOSE` itself and collided with it;
  the test now names its three syscall numbers differently. That is a
  pre-existing property of the runtime, made visible by this round because
  `std.io` now hands out `str`.

## The demo, before and after

`demos/number_check.fi`: **117 lines -> 55 lines.** The full text of both
versions stands in the summary of the round.

## What is deliberately not in it

* **`str` in a `gc class` field.** The collector would have to trace the
  pointer inside it; that is a decision about the type table and belongs in
  the round that also does `Gc[str]`.
* **`.rodata` for text literals.** The octets of a literal land in the frame,
  as they have since round 39 (SPEC 14.1, S8). `str` changed nothing about
  that -- it only puts the address of that array into `p`.
* **Interpolation of a struct.** `f"{point}"` is an error with a clear
  message. There is no `Display` interface yet, and inventing one on the side
  would be worse than the message.
* **`float`.** Round 71.

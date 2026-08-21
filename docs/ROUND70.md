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

| what | measured |
|---|---|
| `bash test.sh` | see the section "Acceptance" below |
| `bash tools/self_compare.sh` | 0 differing / 0 faulty / CODEGEN MISSING 0 |
| `bash tools/fixpoint.sh` | stage 2 == stage 3, character-identical |
| lex/parser/types/sema/fir_compare | no new deviations |
| `bash tools/kernel/run.sh` | 174 / 0 |
| `bash tools/freestanding/run.sh` | 41 / 0 |
| `bash tools/english/check.sh` | 0 0 0 0 0 |
| `bash tools/lexnum/run.sh` | unchanged |
| `bash tools/strsoak/run.sh` | flat with the collector, growing without it |

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

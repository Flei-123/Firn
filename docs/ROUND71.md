# Round 71 — `f32`, and a number reader that was wrong

Branch `r71-f32`, base `3cde1203`.

## The numbers first

| measurement | base `3cde1203` | round 71 |
|---|---|---|
| `test.sh` | FAIL 15/1104 | **FAIL 15/1122** — the SAME fifteen, cause for cause |
| `tools/self_compare.sh` | 293 same / 0 differing / 0 faulty / CODEGEN MISSING 0 | **297 same / 0 differing / 0 faulty / CODEGEN MISSING 0** |
| `tools/fixpoint.sh` | character-identical | **character-identical** (614564 lines of assembly) |
| `tools/lex_compare.sh` | 636 same / 0 different | 645 same / 0 different |
| `tools/parser_compare.sh` | 375 same / 1 known | 382 same / 1 known |
| `tools/types_compare.sh` | 322 same / 0 different | 329 same / 0 different |
| `tools/sema_compare.sh` | 174 same / 0 different | 177 same / 0 different |
| `tools/fir_compare.sh` | 170 same / 1 known | 173 same / 1 known |
| `tools/lexnum/run.sh` | f64 only | **4044 f64 and 5758 f32 literals, four readers each, 0 deviations** |
| `tools/abi/run.sh` | did not exist | 64 checks, 0 differing (against GCC, both directions) |
| `tools/f32data/run.sh` | did not exist | **176 values** out of a real WAV and a real glTF, both compilers, identical |
| `tools/kernel/run.sh` | 174 / 0 | **174 / 0** |
| `tools/freestanding/run.sh` | 41 / 0 | **41 / 0** |
| `tools/english/check.sh` | 0 0 0 0 0 | 0 0 0 0 0 |
| module tests of `firnc0` | 195 | 196 |

The fifteen failures are the fifteen of the base, and each one was held
against the base log by its cause, not by its count:
`tests/1280_std_io_span.fi`, `1282_std_io_read_line.fi`, `1283_std_io_ask.fi`
and `1284_std_io_text_owner.fi` do not compile there (`constant 'SYS_OPEN'
is already declared`, a leftover of the round 69/70/73 merges in
`lib/gc/gc.fi` next to `lib/std`), and `tools/fmt/run.sh`,
`tools/strlib/comfort/run.sh` and `tools/core/run.sh` fail. Those twelve plus three are untouched here. `tools/fmt/run.sh` fails at the
base for the same file (`lib/std/io.fi is not formatted`, out of shape: 1,
refused: 8) and `tools/strlib/comfort/run.sh` and `tools/core/run.sh` with
the same lines. They are not mine to fix without knowing which of the three
merges was meant to win.

## What is new

`f32` is a language type in BOTH compilers: IEEE-754 binary32, four bytes,
alignment four. `float` is its second spelling — round 70 handed out `int`,
`long`, `byte` and `double` and held `float` back on purpose, so that it
would not first mean `f64` and then something else.

**Literals are typeless now.** Where the context says `f32`, the literal is
an `f32`; where nothing says anything, `f64` holds. The suffix `1.5f` exists
for the place where there is no context at all (`let z = 2.5f`); `2f`,
`1e3f` and `1_000.5f` work too. SPEC §14.1.f64 said "floating point literals
are NOT typeless" — that was true exactly as long as there was one type.

**Exactly one implicit conversion exists in the language:** `f32` → `f64`.
It loses nothing. The other direction needs `as f32`. It sits in ONE place
per compiler — `sema::expr` marks it, `lower::lower_expr` carries it out —
so that no context can lose it, and `expr_types` keeps the OWN type of the
expression, because reading an `f32` variable as if it were eight bytes wide
would reach beyond its storage.

## The ABI: a debt from round 11, paid

Up to round 70 an `f64` travelled as a bit pattern in an INTEGER register.
Within Firn that was consistent and correct, and SPEC §14.1.f64 named it as
deviation **F2** together with the reason it was still standing. With `f32`
the debt came due: whoever reads a WAV or a glTF talks to code that somebody
else translated, and that code follows System V AMD64.

`abi.rs` (and `types.fi::word_class` next to it) now classifies per
EIGHTBYTE, as the ABI document prescribes:

| type | eightbytes | registers |
|---|---|---|
| `f32`, `f64` | SSE | `xmm0`–`xmm7` |
| `{ f32, f32 }` | SSE | `xmm0` |
| `{ f64, f64 }` | SSE, SSE | `xmm0`, `xmm1` |
| `{ i64, f64 }` | INTEGER, SSE | `rdi`, `xmm0` |
| `{ f64, i64 }` | SSE, INTEGER | `xmm0`, `rdi` |
| `{ i32, f32 }` | INTEGER | `rdi` (mixed eightbyte = INTEGER) |

Anything for which no register of its class is left travels on the stack.
The code generator reads the class off the FIR type of the argument and
nothing else — which is why the lowering loads an SSE eightbyte as an `f64`
value: `f64` is not the truth about the content, it is the truth about the
register.

**Measured, not claimed.** `tools/abi/run.sh` links Firn objects into a C
program translated by GCC and calls in BOTH directions: GCC calls Firn, and
Firn calls GCC (the Firn stubs are weakened with `objcopy --weaken-symbol`,
so the strong definitions of the C side win at link time). 64 checks, 0
differing, with and without the optimizer.

What is LEFT of the deviation has nothing to do with floating point:
aggregate arguments over 16 bytes travel as a hidden pointer to a copy owned
by the caller, and aggregate returns over 8 bytes always through the hidden
pointer in `rdi`. Both are older than this round and both are written down
in the head of `compiler/src/abi.rs`.

## The error this round found

The interesting part of the round is not the type. It is this:

> Reading a decimal as a correctly rounded binary64 and narrowing it
> afterwards is **not** correctly rounded.

At the exact middle between two binary32 values the first rounding lands ON
the middle, and the tie-to-even of the second then decides without knowing
which side the real value lay on. Measured against glibc `strtof`:
**63568 of 239064** such cases came out one ulp wrong.

The code carried a comment claiming the opposite, with a citation
(Figueroa 1995, "2p+2 bits are enough"). The theorem is real; it holds for
the results of ARITHMETIC on p-bit operands, not for an arbitrary decimal.
The comment was written before the measurement, and that is exactly the
order in which one gets these things wrong.

It came out because the measurement was built before the claim was believed:
the first run of the extended `tools/lexnum/run.sh` reported ONE deviation,
at `3.4028235677973366e+38`, and pulling on that thread unravelled the rest.

**The fix is round-to-odd in between:** cut off, and set the last bit when
anything was left over. A value whose last bit is set is never exactly a
binary32 middle, so the second rounding always has a direction and it is the
right one (Boldo/Melquiond; 53 ≥ 24 + 2). `float_exact` in
`lib/firnc1/lexer.fi` and `strtod` in `lib/num/strtod.fi` have that mode now;
`firnc0` reads the text straight into an `f32`. The float token carries BOTH
bit patterns since this round, because the narrowing has to happen on the
TEXT and the text only exists in the lexer.

**A second finding, smaller:** `numpy.float32("…")` is not an independent
reference. It parses into a double and casts — the very way that is wrong.
The Python column of `tools/lexnum/check.py` computes the exact rational
with `Decimal`/`Fraction` and rounds once, half to even, without a floating
point operation appearing anywhere.

## Text out again

The shortest text for an `f32` is not the shortest text of the double next
to it: `0.1f` widened is 0.100000001490116119384765625, and `dtoa` rightly
makes seventeen digits out of that — but "0.1" reads back as the same four
octets. `lib/num/f32_text.fi` therefore shortens the digits of the widened
double and CHECKS every candidate: read back with `strtod_odd`, narrowed,
compared octet for octet. Among texts of equal length the nearer one wins,
so `3.4028235e+38` comes out and not `3.4028234e+38`.

Held against `numpy` on eleven values, among them both edges of the range,
the smallest subnormal and `1/3`: identical.

## Real data

A test in which a program writes a number and reads it back proves nothing
about a file format — it proves that the program agrees with itself.
`tools/f32data/run.sh` therefore reads octets that come from outside:

* `testdata/f32/tone.wav` — a WAV with 32-bit float PCM (format tag 3),
  64 samples including 0, −0, ±1, the largest value, the smallest subnormal
  and 0.1;
* `testdata/f32/tri.glb` — a binary glTF, 8 vertices as float32 VEC3 in the
  BIN chunk, with the JSON chunk checked as text (`"componentType":5126`,
  `"type":"VEC3"`, `"POSITION"`).

Both files are PRODUCED by `tools/f32data/gen.py`, not checked in: binary
rubbish in a repository is something nobody can read, and a generator can be
looked at. Both compilers read them; all 88 values match what Python reads
out of the same octets.

## What the library learned

* `num.write_f32` / `write_f32_bits` — the shortest text with a round-trip
  guarantee of its own.
* `num.read_f32` / `read_f32_bits` / `text_to_f32` / `text_to_f32_bits`, and
  the `_at` forms in `std.core` for the kernel profile.
* `num.f32_bits` / `bits_f32` / `f32_is_nan` / `f32_is_infinite` /
  `f32_is_null` / `f32_sign`, plus the bit questions on a raw `u32`.
* `math.sqrt32`, `fabs32`, `fmin32`, `fmax32`, `fclamp32`, `trunc32`,
  `floor32`, `ceil32`, `round32`, `math_bits32`, `math_f32`, `INF32`,
  `NAN32`, `EPSILON32`, `is_nan32`, `is_infinite32`, `is_finite32`. All of
  them EXACT: a binary32 computation carried out in binary64 and rounded
  back once is correctly rounded, because 53 ≥ 2·24+2 — and for
  `floor`/`ceil`/`round`/`trunc` there is nothing to round at all, their
  result is a whole number.
* `num.ParsedF32` / `parse_f32` / `f32_or` / `value_or`,
  `io.fmt_f32` (so that `f"{x}"` prints an `f32` as an `f32`),
  `io.read_f32` / `ask_f32`.

## Three things the acceptance found, and one it did not

1. `bin/layoutdump.fi` carried a **fourth** copy of the primitive name table
   and printed an empty name for `f32`. Whoever adds the next type should
   make one table out of the four.
2. The new tests used `size_of[T]()`; `bin/semadump.fi` cannot do that — it
   runs without the mono pass, and `size_of[i32]` fails there in the base
   too. The tests measure the width on the ADDRESSES now: two neighbouring
   array elements lie exactly one element apart, and that IS the layout.
3. `firnc1` wrote the WIDENED type into `etyp` where `firnc0` keeps the own
   one. The programs ran either way; only the type dumps differed — caught
   by `tools/sema_compare.sh`, which is what it is for.
4. **`firnfmt` tore `2.5f` into `2.5` and `f`** and wrote `2.5 f` back. A
   formatter that changes the token stream is worse than none. One line in
   `tools/fmt/fmt.fi`.

`tests/841_gcvec_incremental.fi` needed a bigger allowance and got the
measurement written into the file: the collector scans conservatively, one
word left lying in a frame or in `r12`–`r15` can keep a whole vector with
its 200 cells alive, and round 71 made `std.num`/`std.io` big enough that
the inliner decides differently. Measured: 0 surplus objects before, 131
after — and 0 again with the OLD library on the SAME compiler, which is what
says it is not a leak. A real leak would leave 12,800.

## What is still missing

* **No register allocation for floating point.** SPEC §14.1.f64 restriction
  F1 stands, and it now stands for both widths: every function with a
  floating point value in it goes through the baseline path. That is one
  round of its own — a second register class with intervals of its own.
* **No constant folding of floating point.** Unchanged: the value of an
  `Op::Const` is a bit pattern, and the folding computes with integers.
  Correctly rounded folding belongs to `comptime`.
* **No `f16`/`bf16`.** They would be the next widths and they need the same
  three pieces: a class in the ABI, a rounding in the lexer, a shortest text
  in the library.
* **`%` and bit operations on `f32`** stay refused, the same rule as for
  `f64`.
* The module boundary is still all-or-nothing: whoever imports `std.num`
  gets the `f32` half too, whether he uses it or not. That is what made
  `tests/841` wobble, and it is a linker question (`--gc-sections`), not a
  language question.

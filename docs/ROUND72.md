# Round 72 — checked integer arithmetic, in both compilers

**Branch** `r72-arith` · **Date** 2026-08-22 · AMD EPYC 7571, 64 cores,
12 GiB, Linux x86_64, Debian 12.12, `rustc` 1.99.0-nightly release build of
`firnc0`.

Everything below **was run**. Where a number stands, it was measured on this
machine on this day; where something does not work, it says so.

This document covers the round in **two passes**: the one that built checked
arithmetic in `firnc0` (commit `d589d26e`), and the one that made it hold in
`firnc1` — without which the round could not be merged at all, because the
self-hosting fixpoint was broken.

---

## 1. What the round was for

`SPEC.md` §13 (`L9`) had promised, since the day it was written:

> `+ - *`, `/ %` and a narrowing `as` **check** in `dev`/`dev-fast`/
> `release-safe` and **wrap** in `release-fast` (defined, **not** undefined —
> deliberately different from C).

None of that was true. `opt::Level::ReleaseSafe` existed, ran every
optimisation pass, and the arithmetic underneath it was exactly as unchecked
as `release-fast`. Nothing anywhere ever **asked** the build level whether
arithmetic should be checked, because `lower.rs` — the one place where
`a + b` becomes FIR — never saw the level at all; only `opt.rs`'s FIR-to-FIR
passes did.

The name `release-safe` was making a promise the compiler did not keep.

---

## 2. What was built (first pass, `firnc0`)

| | Lines | What |
|---|---|---|
| `compiler/src/checkmode.rs` | 55 | The missing question, asked once per compilation: does THIS build level check? A `thread_local` set at the top of `main.rs::run`, read by `lower.rs` for every arithmetic expression. |
| `compiler/src/panic_rt.rs` | 614 | The panic path. One shared out-of-line trampoline per object file, a `.rodata` table of prepared messages, and the three shared emitters both backends call (`emit_checked_bin`, `emit_checked_div`, `emit_checked_cast`). |
| `compiler/src/fir.rs` | +91 | `Op::CheckedBin`, `Op::CheckedDiv`, `Op::CheckedCast`, `Op::BinWrapSat`. A checked operation is **never pure** — it can abort the program, so dead code elimination must not remove it even when its result is unused. |
| `compiler/src/lower.rs` | +134 | The decision, and the four message texts. The position is baked into the message as plain text at lowering time, because FIR carries no source positions at all. |
| `compiler/src/lexer.rs`, `parser.rs`, `ast.rs` | +86 | `+% -% *%` (wrapping) and `+| -| *|` (saturating): read greedily, same precedence as the operators they are spelled after, **never** checked in any level. |
| `compiler/src/codegen_x86.rs`, `regalloc.rs` | +371 | Both backends; the saturating clamp is written twice because the two load their operands through different APIs. |
| `demos/kernel/start.s` | +65 | A minimal `osum_panic`: writes the message to COM1 and halts. |

### The shape of a checked site

`rax` and `rcx` hold the two operands, already extended to a full 64 bits.
The arithmetic then runs **at the type's own width**, so the processor's
flag means what the type means:

```
    push rax                     ; the two ORIGINAL values, for the message
    push rcx
    add al, cl                   ; u8: eight bits, not sixty-four
    jc  .Lchksite<fn>_0          ; unsigned -> CF; signed -> OF
    add rsp, 16
    jmp .Lchkok<fn>_0
.Lchksite<fn>_0:
    pop rcx
    pop rdx
    lea rdi, [rip + .Lpanicmsg0]
    mov esi, 57
    mov r8, 1                    ; PANIC_ADD
    mov r9, 1                    ; read the two numbers as UNSIGNED
    jmp .Lpanic_arith
.Lchkok<fn>_0:
```

`add`/`sub` set `OF` for the **signed** reading only; an unsigned type that
leaves its width sets `CF` instead and can leave `OF` clear — `200u8 + 100u8`
is the example, because as `i8` that sum still fits. That is why the jump is
`jc` for unsigned and `jo` for signed, for **every** one of `+ - *`, not
only for the multiplication.

The two originals go on the **stack** and not into a spare register:
`regalloc.rs`'s `TEMP_REGS` includes `r10`/`r11` as ordinary value
registers, so a live value can be sitting in them across a checked site.
Only `rax`/`rcx`/`rdx` are pure scratch there — the same three the unchecked
path already computes in.

### `profile kernel`

There is no runtime to fall back on: no `write`, no `exit_group`. The
trampoline hands off to an **external** symbol `osum_panic(msg, len, a, b,
code)` that the kernel author defines. Leaving it undefined is a **link
error**, not a quiet no-op — which is the honest outcome for a kernel that
never defines it. `demos/kernel/start.s` shows a minimal one.

---

## 3. The two bugs the first pass found

**The CLI default was `release-fast`.** `DESIGN_GOALS.md` and `--help` both
said `dev-fast`; `OptConfig::default()` said `release-fast`. So
`firnc -o x file.fi` — the way every example in the documentation invokes
the compiler — never checked anything, even after the checks existed.

**`test.sh`'s `mode=opt` meant "no flag".** It had been a correct name only
as long as the default happened to be `release-fast`. The moment that
default was fixed, `mode=opt` silently stopped exercising `release-fast` at
all, leaving the suite's own header comment ("`--opt-level=release-fast`,
`--no-opt` and `--opt-level=dev-fast`") untrue and the wrapping promise
completely untested. It now passes `--opt-level=release-fast` explicitly
instead of relying on whatever the default happens to be today.

---

## 4. Why the round was not merged: the fixpoint was broken

`firnc1` — the compiler written in Firn, `lib/firnc1/*.fi` — implemented
**none** of this and wrapped unconditionally in every build level. Measured
on the branch as it stood at `d589d26e`:

| | Measured |
|---|---|
| `tools/self_compare.sh` (section 16) | SAME 242 · **DIFFERING 7** · **FAULTY 63** · first deviation `tests/028_cast_narrow.fi` (`firnc0`: 101, `firnc1`: 57) |
| `tools/fixpoint.sh` (section 17) | **STAGE 2 FAILED (rc=1)** — `firnc1` could not even read `lib/rt/intern.fi` any more, because round 72 had written `h *% 1099511628211` into it and the self-hosted lexer did not know the token |
| `./test.sh` section 3 | **16 failures** — eight programs whose deliberate wraparound the new checks (correctly) turned into a panic |

The fixpoint comes first. Everything in section 5 exists to make those three
lines read differently.

---

## 5. The second pass: `firnc1`

### 5.1 Three bugs in the backend, found by testing it

These are in `firnc0` and were shipped by the first pass. All three are
fixed in commit `3cede7f1`.

**`imul al, cl` is not an instruction.** The two-operand `imul` exists for
16, 32 and 64 bits only. A checked — or saturating — **signed 8-bit**
multiply emitted it anyway and `as` rejected the whole file with "operand
size mismatch". Not one of the 292 programs in `tests/` happens to multiply
two `i8` under a checked level, so it shipped. The one-operand form is the
8-bit answer: `imul cl` computes `ax = al * cl` and sets `OF` exactly when
the product does not fit back into `al` — which is the question being asked.

**The saturating clamp labels collided across functions.** They were built
from the three FIR **value numbers** (`.Lsatclamp0_1_6`). Value numbers
restart at 0 in every function, so five one-line `+|` functions in one file
produced the same label five times and `as` refused it — the exact collision
`SiteCounter` had been introduced for on the checked side, and forgotten
here. Both backends had it.

**Every operand was printed as a signed decimal.** `u64::MAX + 1` reported
`(a=-1 b=1)` — a number that appears nowhere in the program the reader is
looking at. The trampoline now takes `r9 = 1` for an unsigned type and skips
the sign test; the same overflow now reads
`(a=18446744073709551615 b=1)`.

**The optimiser had stopped seeing the arithmetic.** `release-safe` is the
level that checks **and** runs every pass. It ran the passes and folded
nothing at all, because `fold_constants` matches `Op::Bin` and every
`+ - * / %` had become `Op::CheckedBin` one round earlier. Measured on
`tests/opt/fold_arith.fi`: folded at `release-fast`, **not** folded at
`release-safe`. `fold_constants` now folds all four new instructions — a
checked one **only when the result really fits**, because folding an
out-of-range one away would delete a panic the program promised.

**And `test_opt.sh` fell into the trap the round had just found next door.**
Every one of its 46 checks is about a pass that runs at the FULL level only;
"no flag" meant `release-fast` exactly as long as the default happened to be
`release-fast`. Ten of them failed after the default was corrected — none of
them about arithmetic. The level it proves is written down now. Measured:
`FAIL 10/46` before, **`PASS 45/45`** after. (The total is not the same
number in both runs; a few of these checks only run at all when the
assembly they inspect contains what they are looking for.)

### 5.2 What `firnc1` did not have at all: source positions

The syntax tree in `lib/firnc1/ast.fi` carried **no position of any kind** —
not a line, not a file. It had never needed one: `firnc1` reports no
diagnostics of its own, and every position a reader had ever seen came out
of the token stream, which is gone by the time lowering runs.

A checked panic is the first thing in this language that has to name a place
in the source at **run** time. And it has to name the same one `firnc0`
names, down to the column, because `tools/fir_compare.sh` compares the two
intermediate representations as **text** and the message is part of it.

The rule is `parser.rs::join` read off its two branches: a compound
expression takes its **left** operand's position, and `( e )` is
transparent, because the parenthesised form returns the inner node itself.
So `lib/std/math.fi`'s

```firn
    return (b as u64) -% (a as u64)
```

reports column **13** — where `b` stands — and not column 12, where the
parenthesis does. In the tree that falls out of one rule: a leaf takes the
token it is read from, a compound copies from its left child, and the
setter only writes into a node that has none yet.

`ast.e_pos` packs file, line and column into one `u64` (20 + 22 + 22 bits);
the file is the **intern number of its name**, not a per-lexer file number,
because the tree is shared by all modules of a translation and a per-lexer
number would mean two different things in two halves of it.

### 5.3 The rest of the port

| File | What |
|---|---|
| `lib/firnc1/lexer.fi` | The six tokens, read greedily before the one-character forms. `bin/lexdump.fi`'s name table grows with them — the two compilers agree on the **name** of a token kind, never on its number. |
| `lib/firnc1/parser.fi` | `B_ADDWRAP`..`B_MULSAT` at 18..23, the same order as `ast::BinOp`; the position machinery of 5.2. |
| `lib/firnc1/sema.fi` | `if op >= 10` became `if op >= 10 && op <= 17` — without the upper bound every `h *% k` probed as `bool`. Constant folding: wrapping is plain arithmetic, saturating is refused (the limit `firnc0` states in words). |
| `lib/firnc1/fir.fi` | Four new instructions with the text forms of `fir.rs::fmt_inst`. The message travels as an interned string. |
| `lib/firnc1/lower.fi` | The check-mode flag and the four messages, built octet for octet like `lower.rs`. |
| `lib/firnc1/codegen.fi` | +638 lines: the trampoline, the `.rodata` table, and the four emitters. |
| `lib/firnc1/mono.fi` | An instantiated template keeps the **template's** position, as `firnc0`'s clone of the tree does. |
| `bin/firnc1.fi` | `--opt-level=` and `--no-opt`, defaulting to `dev-fast`. `firnc1` has no optimiser, so the level says nothing else there — but it has to say **this**, or the two compilers would disagree about every overflowing program. |
| `tools/fmt/fmt.fi` | `firnfmt` scans the six operators. Without that it split `*%` into `*` and `%` and rewrote the source into `h * % 1099511628211`, which no longer scans. |

### 5.4 The generated file the round forgot

`lib/firnc1/gctext.fi` carries the collector runtime **as data**: `firnc0`
embeds the same text with `include_str!` and therefore picked up round 72's
`*%` in `lib/gc/gcmap.fi` at `cargo build` time, while `firnc1` kept the old
octets and had to be regenerated by hand (`tools/gen_gctext.sh`).

It was not regenerated. **Twenty-eight** test programs disagreed between the
two compilers for that reason alone, none of them about arithmetic. Since
this is exactly the kind of thing that costs an afternoon to find,
`tools/fixpoint.sh` now refuses to start when `gctext.fi` is older than
`lib/gc/*.fi`.

The same trap in the other direction: `lib/std/core.fi` and
`tests/306_dtoa_roundtrip_small.fi` are assembled by
`tools/strlib/expand.py`, so the `*%` had to go into the sources under
`tools/strlib/src/` and `lib/math/`, not into the generated copies.

### 5.5 The wraparounds that had to be written down

Round 72's own rule is that a wraparound one **wants** gets written down.
Eight sites did not have it and became panics; each one is a place where the
wraparound really is the point:

| Where | What |
|---|---|
| `lib/gc/gcmap.fi`, `lib/rt/map.fi` (round 72) | the two MurmurHash3 mixing multiplications |
| `lib/rt/intern.fi`, `lib/std/str.fi`, `lib/str/atom.fi` (round 72) | the FNV-1a multiplication |
| `lib/math/core_math.fi` → `lib/std/math.fi`, `lib/std/core.fi` | `abs_diff`: `(a as u64) -% (b as u64)` **is** the method — it gives the right magnitude for `i64::MIN` precisely by never leaving `u64` |
| `lib/math/core_math.fi` (`pow_u`) | the **last** squaring is dead work: once the highest set bit of the exponent is consumed, `base` is squared once more and thrown away. Wrapping it is right, nothing reads it. The multiplication into `r` stays **checked** — an `r` that does not fit is a wrong answer, not dead work. |
| `tests/1230_bitnot.fi` | `0 -% 255` on `u32`, spelling `0xFFFFFF01` |
| `tests/306_dtoa_roundtrip_small.fi` | the xorshift\* mixing multiplication |
| `tests/690_lowering_core.fi` | `u64::MAX +% 1 == 0`, the line that exists to show it |
| `tests/800_std_str_core.fi` | `0 -% 1`, the `npos` sentinel |

Three more programs exist **specifically** to prove that `release-fast`
truncates — `tests/028_cast_narrow.fi`, `tests/030_wrap_u8.fi`,
`tests/054_i16_ops.fi`. They carry `// only_mode: opt` now, the mechanism
round 72 introduced for `tests/1334b_type_truncation.fi`: run at the one
level where the assertion is true by definition, instead of hiding the
truncation they exist to demonstrate behind some other operator.

---

## 6. The measurements

### 6.1 The fixpoint (`tools/fixpoint.sh`, section 17)

|  | Before (`d589d26e`) | After |
|---|---|---|
| Stage 2 | **FAILED (rc=1)** | 4,580 ms, 4,084,048 octets |
| Stage 3 | not reached | 13,688 ms, 4,084,048 octets |
| Stage 2 vs. Stage 3 | — | **character-identical, 689,612 lines of assembly** |
| `.firnc2` over the corpus | — | SAME 312 · DIFFERING 0 · FAULTY 0 |

### 6.2 Behaviour (`tools/self_compare.sh`, section 16)

|  | Before | After |
|---|---|---|
| SAME BEHAVIOUR | 242 | **312** |
| DIFFERING | 7 | **0** |
| FAULTY | 63 | **0** |
| SKIPPED (`firnc0` cannot compile the file alone) | 19 | 19 |
| COMPTIME | 1 | 1 |
| first deviation | `tests/028_cast_narrow.fi` (101 vs 57) | — |

### 6.3 The intermediate representation (`tools/fir_compare.sh`, section 15)

The sharpest comparison in the series: the FIR text of both compilers,
octet for octet, **including** the panic message with its file, line and
column.

| | Measured |
|---|---|
| SAME | **174** |
| DIFFERENT | 1 (`tests/590_f64.fi`, the round-20 floating point literal, known and named) |
| Instructions compared | 50,973 |

For `tools/checked/add_i32.fi` both compilers produce, character for
character:

```
  %9 = checked_add.i32 %7, %8 "panic: integer overflow in 'i32 + i32' at tools/checked/add_i32.fi:10:18"
```

### 6.4 The new section (`tools/checked/run.sh`, test.sh section 44)

**150 checks, 0 failures.** Twelve programs, six groups, both compilers:

* a program that goes out of range aborts with exit code **101** in `dev`,
  `dev-fast` and `release-safe`, and **wraps** (returns 7) in
  `release-fast` — `add_i32`, `sub_i8`, `mul_u8`, `mul_i8`, `cast_narrow`,
  `cast_sign`;
* the message is compared **between the two compilers**, not just for its
  shape:
  `panic: integer overflow in 'i32 + i32' at tools/checked/add_i32.fi:10:18 (a=2147483647 b=1)`
  out of both;
* division by zero and `MIN / -1` (`div_zero`, `div_min`, `rem_zero`) — the
  three checked levels only: `release-fast` has nothing to wrap there, the
  processor raises `SIGFPE`, and that is not this round's promise;
* `+% -% *%` and `+| -| *|` return 42 in **all four** levels out of both
  compilers (`wrapsat`, `two_sites` — the latter being the regression test
  for the label collision of 5.1);
* counter-checks: `widths.fi` stays in range at every integer width and
  behaves exactly as it always did; and a program built `release-fast`
  contains **zero** mentions of the trampoline while the same program at
  `dev-fast` contains several;
* the optimiser really sees the checked arithmetic (`fold.fi`): **seven**
  checked/wrapping/saturating operations before the optimiser at
  `release-safe`, **zero** after — and the out-of-range addition of
  `add_i32.fi` is still there, because folding that one away would delete
  a promised panic.

### 6.5 What it costs

`firnc1` compiling **itself** is the clean measurement: it has no optimiser,
so the only difference between the two levels is the checks.

| | `dev-fast` (checked) | `release-fast` (unchecked) | Difference |
|---|---|---|---|
| Assembly | 689,612 lines | 654,701 lines | **+5.3 %** |
| Binary | 4,084,048 octets | 3,856,704 octets | **+5.9 %** |
| Compile time | 4,152 ms | 4,498 ms | — (noise; `firnc1` runs no passes either way) |

Run time, `bench/firn/`, `firnc0`, best of three, `release-safe` (checked,
all optimisation passes) against `release-fast` (unchecked, the same
passes):

| Program | `release-safe` | `release-fast` | Factor |
|---|---|---|---|
| `fib` | 56 ms | 43 ms | 1.30× |
| `sieve` | 128 ms | 105 ms | 1.22× |
| `statemachine` | 202 ms | 176 ms | 1.15× |
| `bytecount` | 506 ms | 321 ms | 1.58× |
| `bubblesort` | 207 ms | 76 ms | 2.72× |
| `matmul` | 407 ms | 61 ms | **6.67×** |

`matmul` is the honest worst case and it should be read as one: its inner
loop is three array indices and one multiply-accumulate, so nearly every
instruction in it becomes a checked one, and the `push`/`push`/`add rsp, 16`
around each of them is not something the register allocator can hide. This
is the price of the promise — which is exactly why `release-fast` exists and
why `+% -% *%` exist for the places inside an otherwise checked program
where the wraparound is the point.

**No measurement was taken of a checked build being *faster* anywhere.**
Where a number is missing above, it is missing because it was not measured,
not because it was inconvenient.

### 6.6 The suite

`./test.sh` — 0 failures. `tools/english/check.sh` — `0 0 0 0 0`.
`firnfmt -c` over every `.fi` file that scans — clean.

---

## 7. What is NOT there

* **There is no unchecked spelling for a narrowing `as`.** `+% -% *%` exist
  for arithmetic; a deliberate truncation has to be written as
  `(x & 4294967295) as u32`. That came up for real in
  `lib/firnc1/fir.fi`, where the low half of a packed word is read out on
  purpose. Masking first says the same thing and the checked `as` agrees
  with it — but it says it in arithmetic rather than in the operator, and
  an `as%` (or `truncate(x)`) would say it better. **Not built in this
  round.**
* **`+| -| *|` do not work in a constant expression.** Both compilers
  refuse: `firnc0` with a sentence, `firnc1` by marking the constant as not
  evaluable. Wrapping (`+% -% *%`) does fold. Saturating at compile time is
  not hard, it is simply not written.
* **The panic prints the two operand values, never the result.** For a cast
  there is only one value and `rcx` carries the same one twice, so the
  message reads `(a=313 b=313)`. `panic_rt.rs` says so in its own header;
  there was no natural second number to put there instead.
* **`osum_panic` gets the message, not the numbers.** The kernel branch of
  the trampoline hands over `rdi`/`esi`/`rdx`/`rcx`/`r8`/`r9` faithfully,
  but `demos/kernel/start.s`'s own definition prints only the text —
  duplicating the decimal formatter in hand-written assembly for a demo
  kernel was not worth doing twice.
* **The check is per operation, not per expression.** `a + b + c` tests
  twice, and `(a + b) - b` is not recognised as an identity. No range
  analysis of any kind runs before lowering, so an addition the optimiser
  could prove in range still pays. That is the single biggest lever left on
  the 6.67× above.
* **Only constant folding was taught the new instructions, not CSE and not
  LICM.** Two identical `a * b` in one function stay two checked
  multiplications at `release-safe` and become one at `release-fast`
  (measured). For CSE that is simply missing work. For LICM it is more than
  that: hoisting a checked operation out of a loop would make a loop that
  never runs panic, so it needs an argument, not just a match arm.
* **`firnc1`'s assembly is not `firnc0`'s assembly**, and it never was —
  `firnc1` has no register allocation, every value lives in the frame. What
  is compared is the FIR text (6.3) and the behaviour (6.2), which is
  documented at the top of both comparison tools.

---

## 8. Reproducing it

```sh
bash test.sh                       # everything; section 44 is this round
bash tools/checked/run.sh          # 139 checks, both compilers
bash tools/fixpoint.sh             # stage 2 == stage 3
bash tools/self_compare.sh         # 312 same, 0 differing, 0 faulty
bash tools/fir_compare.sh          # the FIR text, octet for octet

# by hand:
compiler/target/release/firnc tools/checked/add_i32.fi -o /tmp/a && /tmp/a
# panic: integer overflow in 'i32 + i32' at tools/checked/add_i32.fi:10:18 (a=2147483647 b=1)
compiler/target/release/firnc --opt-level=release-fast tools/checked/add_i32.fi -o /tmp/a && /tmp/a
echo $?     # 7 -- it wrapped

./.firnc1 tools/checked/add_i32.fi -o /tmp/b && /tmp/b     # the same sentence
```

`.firnc1` is built by `tools/self_compare.sh` and by
`tools/checked/run.sh`; neither reuses an older one than its sources.

---

# Round 83 — the same round, eleven rounds later

**Branch** `r83-arith` · **Date** 2026-08-23 · same machine.

Round 72 was finished and never merged. In the meantime `main` grew round
78 (the operating system is called **Osum** now, and the layout acceptance
runs against a frozen reference instead of Chromium), round 79 (escape
analysis), round 80 (the aarch64 backend), round 81 (the standard library:
deflate, JSON, crypto, hash) and round 82 (v128, AES-NI/SHA-NI, the
peephole pass). A `git merge r72-arith` produced 13 conflicts in 8 files.
This round is the merge, done properly.

## 9. Merge, not rebase

Six commits that touch the same lines over and over, against five rounds of
change: a rebase means resolving the same thirteen collisions up to six
times, once against every intermediate state — and the intermediate states
of a round are not states anybody wants to bisect through. One merge commit
resolves each collision ONCE, against the state that gets tested.

### 9.1 The collision that git did not report

`lib/firnc1/ast.fi`. Round 72 and round 79 BOTH gave the tree source
positions, independently, and neither knew about the other. git merged the
two additions on top of each other without a word:

| | Round 72 | Round 79 |
|---|---|---|
| Field | `e_pos: Vec[u64]` | `e_pos: Vec[u64]`, `s_pos: Vec[u64]` |
| Packing | file 20 bits, line 22, column 22 | file 16 bits, line 24, column 24 |
| File number | the INTERN number of the file NAME | the number in the TREE's file table (`ast.file_add`) |
| Accessors | `e_pos`/`e_pos_set` | `expr_pos`/`expr_pos_set` |
| Written by | `pos_now`/`pos_default`/`pos_from` | `at_pos` in `un_expr` |
| Read by | `lower.fi` (the panic message) | `escape.fi` (the refusal message) |

The result compiled as text and was nonsense as a program: two `e_pos`
vectors in one struct, two `pos_pack`, the vector freed twice in
`tree_free` and pushed twice per node in `expr_new`. **Round 79's is the
one that stayed** — `firnc1.fi` and `escape.fi` already feed it — and round
72's readers were moved onto it. `lower.fi::msg_at` therefore prints the
file name out of `ast.file_ptr` instead of out of the interner; it is the
same text, because `firnc1.fi` registers every file under the name
`firnc0` prints.

That last sentence had a hole in it, and `tools/atomic/run.sh` found it
within the hour: `bin/firdump.fi` and the other small drivers never call
`par_file_set`, because nothing they did needed a file name before. Their
tree had an EMPTY file table, so the panic message read `at :13:13` where
`firnc0` wrote the path. Round 72 could not run into this — it carried the
name's intern number in the position itself and needed no table. The fix is
in `par_new`, for the tree a parser owns, where it cannot be forgotten
again.

### 9.2 The rest of the thirteen

| File | Settled |
|---|---|
| `compiler/src/codegen_x86.rs` | Round 82's xmm cache (`xclear`/`xretire`) and round 72's `SiteCounter` parameter are both in `emit_block`. Neither replaces the other. |
| `lib/firnc1/codegen.fi` | Round 75's `tr`/`ext_out` and round 72's `pmsg`/`site` are both fields of `Cg`. |
| `lib/firnc1/mono.fi` | Round 79's `copy_expr_inner`, which already does what round 72's `copy_expr_at` did. |
| `lib/firnc1/parser.fi` | `fileid` gone, `pos_now` is `at_pos`. |
| `tools/layout/chrome.py` | Round 78's frozen reference wins; round 72's viewport probe stays, because `--refresh-reference` needs it. |
| `tools/layout/FirnMetric.ttf` | Regenerated with `make_font.py`. Byte-identical to the one on `main` — the script pins `head.created`/`modified`. |
| `lib/firnc1/gctext.fi` | Regenerated with `tools/gen_gctext.sh` (GCTEXT_ALL 150,167 → 150,285). |
| `test.sh` | Both sections; the number is now 44 (see 9.4). |

### 9.3 `karst_panic` → `osum_panic`

Round 78 renamed the operating system and cleared the old name out of the
repository; round 72 was written before that and carried it back in.
`karst_panic`, `KARST_PANIC`, `.Lkarst_loop`, `.Lkarst_nl`, `.Lkarst_wait`,
`.Lkarst_halt` — in `panic_rt.rs`, `lib/firnc1/codegen.fi`,
`demos/kernel/isr.s`, `demos/kernel/start.s`, the three `run.sh` that check
for undefined symbols, `SPEC.md`, `RUN.md` and this document.
`grep -ric karst` over the tree (without `.git` and `target`) is **0**.
`lib/firnc1/codegen.fi` also needed its literal length adjusted, 148 → 146
octets: the name is two characters shorter, twice.

### 9.4 Section 40 → section 44

Round 72 took section 40 while it stood on an older `main`. 40 went to the
escape analysis (round 79), 41 to the library (round 81), 42 to the speed
(round 82) and 43 to the second machine (round 80) in the meantime.
Checked arithmetic is **section 44**. Head comment and section both; the
numbers 1 to 44 each occur exactly once.

## 10. What `main` looked like underneath

`main` **did not build**. The merges of round 80 and round 82 crossed:
`codegen_a64.rs` builds an `Emitter` without round 82's `xmm` field and its
`match` has no arm for `Op::Simd`. `cargo build --release` fails with two
errors, `./test.sh` therefore dies in its first section, and that is the
`RC=101` the last acceptance run left behind. Repaired here, because
nothing else can be measured otherwise: the aarch64 backend REFUSES
`Op::Simd` by name (NEON is a different instruction set, not a different
spelling of SSE), and two more `Emitter` literals in the module tests of
`codegen_a64.rs` needed the same field.

## 11. The second machine (`compiler/src/panic_rt_a64.rs`)

`docs/ROUND80.md` §7 described what checked arithmetic on aarch64 would
take and left it undone. This round built it — see that section for the
detail. It was not a free choice: the command line default is `dev-fast`,
`dev-fast` checks, and `tools/aarch64/run.sh` compiles without a flag, so a
backend that refused the four new operations would have refused nearly
every program in `tests/`.

## 12. The wraparounds rounds 76 and 81 wrote

§5.5 above is the list round 72 made for the code that existed THEN. Five
test programs and three library files that arrived AFTER it aborted under a
checking build level, every one of them for a good reason:

| Where | What, and why it is right that it struck |
|---|---|
| `lib/std/hash.fi` (round 81) | FNV-1a, xxHash64 and splitmix64 are arithmetic modulo 2^64 BY DEFINITION — nineteen operations, now `*%` / `+%` / `-%`. |
| `lib/std/bytes.fi` (round 76) | The wire writes the BIT PATTERN of a signed number; `v as u32` is a checked narrowing cast (same width, other sign) and `-1` is not a `u32`. Eight functions `bits_u8`..`bits_i64` spell the reinterpretation once, in arithmetic that never leaves its range, and are EXPORTED: every caller building a packet by hand needs the same thing, and each one writing its own mask is how a check gets worked around instead of served. |
| `tests/1600`, `tests/1601`, `tests/1611`, `tools/bench82/cross.fi` | The MMIX/PCG generator is modulo 2^64; `(x >> 33) as u8` means the low octet, and the mask says so. |
| `tests/1602_nbt_roundtrip.fi` | `i8::MIN` … `i64::MIN` written as the numbers they are, instead of as a cast that does not fit. |
| `tools/stdlib81/hashprobe.fi` | A counting sequence that runs past 255 and is MEANT to start over. |
| `demos/kernel/kmain.fi` | The deliberate `#DE` was `42 / zero`, with the divisor read out of the data area so no pass could fold it. The checked division catches it BEFORE the processor does, hands it to `osum_panic`, and the kernel never returns — QEMU ran into its time limit. What that test wants is the CPU's own exception, so the division moved into `asm(...)`, past the language. The round working, not the round breaking the kernel. |

Every `tests/*.fi`, `tests/opt/*.fi` and `examples/*.fi` compiled at
`dev-fast` and run: **0 programs abort** (they were 5). Every program under
`tools/`: 0 (they were 2).

## 13. What this round did NOT do

* **`as%` still does not exist.** §7 above wanted it in round 72 and wanted
  it again here: three files now spell a reinterpretation as
  `((v as i64) & 4294967295) as u32`, which says the same thing in
  arithmetic and says it worse. It is the one language gap this merge made
  visible twice.
* **`profile kernel` on aarch64** is still refused (round 80 §2), so
  `osum_panic` has no A64 form.
* **The optimiser still does not hoist or CSE a checked operation** (§7).

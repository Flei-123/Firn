# Round SPEED — Firn is to become faster than Rust

Branch `speed`. Goal named by Justin on 27.08.2026 and written into
`orientos/ROADMAP.md` as item 4.9: not level with `rustc -O`, **in front of
it**.

Everything below was **run**. Every number has the command next to it, and
the rounds that failed are in here too — those are the ones worth reading.

**Machine:** AMD EPYC 7571, 12 vCPU, Debian 12, Linux 7.0.14-5-pve x86_64.
**Toolchain:** `rustc` 1.99.0-nightly (c98d0cb27 2026-08-12), GNU `as`/`ld`,
`valgrind` 3.19 (callgrind).
**Base:** `main` at `c9f2d7fe`.

**How it is measured.** Two tools, and they answer two different questions.

* `python3 tools/bench90/bench.py` — the wall clock, median of 9 runs, four
  binaries per program (`release-fast`, `release-safe`, `rustc -O`,
  `rustc -O -C overflow-checks=yes`). Both sides print their result and the
  outputs must match, so nothing can be optimised away on either side.
* `python3 .speed/ab.py --old A --new B` — new in this round. The machine is
  shared; measuring all of A and then all of B puts a load spike on one of
  them. This one **interleaves**: one run of A, one run of B, nine times
  each, then the medians are compared. A layout change does not move the
  instruction count at all, so `icount.py` is blind to it and the clock is
  the only witness there is — it had better be a clock that is not lying.

---

## Round 0 — the starting point, measured again from scratch

    BENCH90_RUNS=9 python3 tools/bench90/bench.py

| benchmark | firn `release-fast` | `rustc -O` | factor | firn `release-safe` | `rustc -O +checks` | factor |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.049 s | 0.032 s | 1.54x | 0.049 s | 0.031 s | 1.56x |
| sieve | 0.043 s | 0.031 s | 1.38x | 0.050 s | 0.027 s | 1.83x |
| matmul | 0.055 s | 0.025 s | 2.25x | 0.202 s | 0.078 s | 2.59x |
| bytecount | 0.385 s | 0.176 s | 2.19x | 0.374 s | 0.120 s | 3.12x |
| bubblesort | 0.078 s | 0.039 s | 2.02x | 0.113 s | 0.067 s | 1.70x |
| statemachine | 0.211 s | 0.082 s | 2.58x | 0.193 s | 0.073 s | 2.64x |
| bitmap | 0.065 s | 0.034 s | 1.93x | 0.077 s | 0.032 s | 2.43x |
| xxhash | 0.284 s | 0.191 s | 1.49x | 0.270 s | 0.188 s | 1.44x |
| jsonscan | 0.326 s | 0.072 s | 4.53x | 0.342 s | 0.078 s | 4.39x |
| memstride | 0.215 s | 0.204 s | 1.06x | 0.240 s | 0.204 s | 1.18x |
| branchy | 0.555 s | 0.464 s | 1.19x | 0.520 s | 0.478 s | 1.09x |

**median `release-fast` vs `rustc -O`: 1.93x · median `release-safe` vs
`rustc -O +checks`: 1.83x.**

---

## Round 1 — the number in the roadmap was measuring the wrong build level

**Hypothesis.** `sieve` is 4.16x behind `rustc -O` because the code
generator is bad at it.

**Setup.** Before optimising anything, look at what the 4.16x actually
compares. `bench/bench.py` builds the Firn side with

    firnc -o bin bench/firn/sieve.fi

— no build level at all. Since round 72 the default level is `dev-fast`, and
`dev-fast` (a) **checks** every integer operation and (b) does not run the
one pass that is not debug-preserving, **`inline`**. `bench/firn/sieve.fi`
does every single byte access through `ld8()`/`st8()`, so at `dev-fast` the
hot loops look like this:

    .Lmain__bb5:
        mov rdi, r15
        mov rsi, r12
        mov rdx, 1
        call _F0.st8          <- a function call per byte written
        add r12, 1
        jc  .Lchksitemain_3
        jmp .Lmain__bb4

and `st8` itself pushes a frame, adds, stores, pops and returns. `rustc -O`
inlines it and checks nothing.

**Measurement** (`python3 .speed/bench2.py --runs 9`, median of 9):

| level | sieve | vs `rustc -O` |
|---|---:|---:|
| `dev-fast` (what the table measured) | 0.1141 s | 3.89x |
| `release-safe` | 0.0487 s | 1.66x |
| `release-fast` | 0.0424 s | 1.44x |

**Conclusion. The hypothesis is wrong and the 4.16x is not a code generator
result.** It is a checked, uninlined everyday build held against a fully
optimised unchecked one. `docs/ROUND90.md` §2.1 had already found and
written this down; `bench/RESULTS.md` and roadmap item 4.9 were never
brought into line with it and still quote the round-86 table. Both are
corrected at the end of this round.

Two honest questions remain, and they are measured separately from here on:
`release-fast` against `rustc -O` (both unchecked — how good is the code
generator) and `release-safe` against `rustc -O -C overflow-checks=yes`
(both checked — what do the checks cost).

**Not thrown away:** `sieve` at 4.16x was still the reason to look at the
generated code, and looking at it produced round 2.

---

## Round 2 — every loop pays two taken jumps per iteration

**Hypothesis.** The block layout places the loop **exit** in the fallthrough
and the loop **body** somewhere else, so each iteration executes two taken
branches instead of one.

**Setup.** `regalloc.rs::emit_order` builds traces and, at a `brcond`,
prefers `else_bb` — unconditionally, since round 51. A loop head is
`brcond i < n, body, exit`: the `else` side is the exit, the one edge of the
whole loop that is taken exactly once. `matmul`, innermost loop,
`--opt-level=release-fast --emit=asm`:

    .Lmain__bb16:
        cmp r12, 240
        jb  .Lmain__bb17      <- TAKEN, every iteration
    .Lmain__bb18:             <- the exit, fallthrough, reached once
        ...
    .Lmain__bb17:
        ... body ...
        jmp .Lmain__bb16      <- TAKEN, every iteration

**The change.** `emit_order` breaks the tie at a `brcond` by **loop depth**:
the successor that sits deeper inside loops wins, and only on a tie does the
old preference for `else` stand. `loop_depth` already existed in the same
file (the allocator asks the same question). `emit_block` inverts the
condition itself when `then` turns out to be the next block, so no case is
lost. Ten lines, layout only — liveness, intervals and register choice all
keep working on the FIR order, and the instruction count does not change by
one.

    .Lmain__bb16:
        cmp r12, 240
        jae .Lmain__bb18      <- NOT taken, every iteration
    .Lmain__bb17:
        ... body ...
        jmp .Lmain__bb16      <- taken

**Measurement.** `icount` is blind here by construction — the same
instructions in a different order. Interleaved wall clock, 9 runs each,
`release-fast`:

    python3 .speed/ab.py --old .speed/firnc-base --new .speed/firnc-r2 --runs 9

| benchmark | before | after | change |
|---|---:|---:|---:|
| fib | 0.0465 s | 0.0465 s | -0.10 % |
| **sieve** | 0.0416 s | **0.0284 s** | **-31.78 %** |
| matmul | 0.0471 s | 0.0460 s | -2.35 % |
| **bytecount** | 0.3693 s | **0.3250 s** | **-11.99 %** |
| **bubblesort** | 0.0767 s | **0.0579 s** | **-24.55 %** |
| statemachine | 0.2062 s | 0.1975 s | -4.22 % |
| bitmap | 0.0616 s | 0.0624 s | +1.30 % |
| xxhash | 0.2624 s | 0.2662 s | +1.44 % |
| jsonscan | 0.3030 s | 0.2980 s | -1.66 % |
| memstride | 0.2141 s | 0.2094 s | -2.21 % |
| branchy | 0.5268 s | 0.5332 s | +1.20 % |

**Conclusion. The hypothesis holds, and it is worth the most exactly where
the loop body is short**: `sieve`'s three loops are four to six
instructions, so one taken branch out of two is a third of the loop.
`bitmap`, `xxhash` and `branchy` move by about a percent the wrong way —
that is inside the noise of this machine and no loop of theirs changed
shape; `branchy` is dominated by mispredictions that no layout touches.

`sieve` at `release-fast` is now **0.0284 s against `rustc -O`'s 0.0294 s** —
the first benchmark of the set in front of Rust.

---

## Round 3 — the loop depth that was an approximation

**Hypothesis.** Round 2 helped `sieve` by a third and `bytecount` by only a
twelfth. If the rule is right, the difference is not in the rule but in
whether it fires.

**Setup.** `regalloc.rs::loop_depth` — the function round 2 borrowed for the
tie-break — answers with an approximation: a back edge `u -> v` with `v <= u`
counts the block NUMBERS `v..=u` one deeper. That is right whenever a loop
body is numbered contiguously. `bench/firn/bytecount.fi` is not:

```text
bb7:  brcond k < n, bb15, bb8      <- the loop head
bb8:  ...                          <- the EXIT (inside the outer loop)
bb14: br bb7                       <- the latch
bb15: ...                          <- the BODY, numbered past the latch
```

The back edge is `bb14 -> bb7`, so the approximation counts `bb7..=bb14` and
gives the body `bb15` depth 0 while the exit `bb8` gets 2 — it lies inside
the outer pass loop as well. Round 2's tie-break then picked the EXIT, on the
hottest loop of that benchmark.

**The change.** `layout_depth` in the same file, over the **natural loops**:
a back edge is an edge whose target dominates its source, and the loop is the
head plus everything that reaches the source without passing the head — the
definition `licm.rs` already uses. Used by the block layout alone. The old
`loop_depth` stays exactly as it was: the register allocator reads it, and
changing what the allocator sees is a different round.

**Measurement** (`.speed/ab.py`, 9 runs each, interleaved, `release-fast`):

| benchmark | round 2 | round 3 | change |
|---|---:|---:|---:|
| **bytecount** | 0.3584 s | **0.2499 s** | **-30.3 %** |
| **bitmap** | 0.0648 s | **0.0462 s** | **-28.7 %** |
| statemachine | 0.2323 s | 0.2147 s | -7.6 % |
| branchy | 0.6346 s | 0.5848 s | -7.9 % |
| matmul | 0.0472 s | 0.0459 s | -2.8 % |
| **sieve** | 0.0301 s | **0.0356 s** | **+18.4 %** |
| **bubblesort** | 0.0571 s | 0.0690 s | **+20.9 %** |

**Conclusion. The hypothesis holds where it was meant to and produced two
regressions that had nothing to do with the rule.** For `sieve` the emitted
instructions are **character-identical** between the two builds — only the
order of the blocks in the file differs, and its count loop went from two
taken jumps to one. It cannot execute more work and it did. That is a
placement effect, and it is what rounds 4 and 5 are about.

---

## Round 4 — aligning loop heads: a coin toss, and it stays out

**Hypothesis.** `sieve` got slower without executing one more instruction, so
the cause is the ADDRESS: a loop whose body straddles a fetch window costs an
extra window per iteration. `rustc` writes `.p2align 4` in front of every hot
loop. Do the same.

**Setup.** `.p2align 4, 0x90` in front of every block that is the target of a
backward edge. First try with the GCC-style cap `,,10` (skip the alignment if
it would cost more than ten octets) — measured, and it almost never fired:
the loop heads of `sieve` sit 15 octets before the boundary. Without the cap
it fires.

**Measurement** (interleaved, 15 runs, `release-fast`, against round 3):

| benchmark | round 3 | round 3 + alignment | change |
|---|---:|---:|---:|
| **sieve** | 0.0364 s | **0.0285 s** | **-21.7 %** |
| **bytecount** | 0.2409 s | **0.2881 s** | **+19.6 %** |
| bitmap | 0.0421 s | 0.0436 s | +3.5 % |
| matmul | 0.0448 s | 0.0463 s | +3.4 % |
| bubblesort | 0.0591 s | 0.0584 s | -1.0 % |

**Conclusion. The hypothesis is not disproved and the change is worthless.**
Alignment moves both benchmarks by twenty percent, in opposite directions,
and the median over the set is **+3.4 %** — worse. Sixteen octet alignment
does not put a loop into one window, it moves everything behind it as well,
and the next loop pays what this one won.

It stays in the file behind `FIRN_ALIGN_LOOPS=1` with this measurement in the
comment, because the next person will have the same idea. Doing it properly
needs the SIZE of the loop, which means emitting twice and aligning only the
loops that then really fit into one window. That is its own round.

---

## Round 5 — a loop is laid out in one piece

**Hypothesis.** `bubblesort` did not get slower from alignment but from round
3, and not by chance. Look at what moved.

**Setup.** The trace builder carried on at the LOWEST unplaced block whenever
a trace broke off. That interleaves regions that have nothing to do with each
other. `bubblesort`, innermost loop, round 3:

```text
400412: jg 4004d5      <- the `if x > y` arm, 0xa1 octets away, jg rel32 (6 octets)
```

against round 2:

```text
400483: jg 4004a4      <- 0x1c away, jg rel8 (2 octets)
```

The whole final summing loop had been placed between the comparison and the
swap it jumps to — and in a bubble sort the swap is taken on a good half of
the comparisons.

**The change.** When the trace breaks, carry on with the **deepest** unplaced
block instead of the lowest-numbered one; ties keep the old order. The rest
of a loop then stays next to the loop.

**Measurement** (15 runs, interleaved, `release-fast`, all against round 2):

| benchmark | round 3 | round 5 |
|---|---:|---:|
| bubblesort | +2.4 % | **+1.1 %** |
| statemachine | -6.0 % | **-12.5 %** |
| matmul | -0.1 % | -0.4 % |
| bytecount | -26.9 % | -11.4 % |
| bitmap | -30.0 % | -19.1 % |
| sieve | +11.4 % | +7.9 % |

**Conclusion. It fixes what it was built for and it is not free.** The short
jump is back, `bubblesort` is level and `statemachine` doubles its gain;
`bytecount` and `bitmap` give back about half of round 3 — placement again,
and this time on the other side. It stays, because "the blocks of one loop
belong together" is a rule that can be defended from the disassembly, and
"this address happened to be luckier" is not.

**What this round really taught, and it is worth more than the change:** on
this machine, wall-clock differences below about ten percent on a thirty
millisecond program are NOT attributable. Three other test suites were
running on it for most of this session (load average between 6 and 19), and
the same binary moved by fifteen percent between two sessions. Everything
from here on is decided on `callgrind` instruction counts, which are exact,
and the clock is used for the size of an effect, not for its existence.

---

## Round 6 — dividing by a constant without `div`

**Hypothesis.** `bytecount` fills 16 MiB with `i % 251` — 16.7 million
divisions in one loop. `div r64` costs some 14 to 47 cycles on this
processor. `rustc` emits none.

**Setup.** The disassembly of the fill loop, before:

```text
mov rax, r10 / mov rcx, r8 / xor edx, edx / div rcx
```

and `rustc -O` for the same line:

```text
movabs rdi, 367465021388636487 / mov rax, rcx / mul rdi / ...
```

**The change.** Hacker's Delight figure 10-3 (unsigned) and 10-1 (signed),
generalised to 32 and 64 bits, in `regalloc.rs::emit_div_const`. Checked
against Python's integer division for every divisor from 2 to 300 plus the
edges, at both widths, before a line of Rust was written; for `d = 251` at 64
bits it yields **367465021388636487** — the same constant `rustc` puts in.

It also applies to `Op::CheckedDiv`: a constant divisor that is neither `0`
nor `-1` makes both of that instruction's checks dead by construction.

**And the reason it did not fire on the first try**, which is the part worth
writing down: `immediate_consts` struck the DIVISOR out of the immediate set,
for both operands of a `Div`/`Rem`, with the comment "operand `a` goes
through `load_ext` — no immediate". So `emit_div_const` never saw a constant
divisor: `bytecount` put its 251 in `r8` and divided by the register. Nothing
needed it struck — the only place that reads the divisor is `load_ext`, which
turns an immediate into `mov rcx, 251` exactly as it turns a register into
`mov rcx, r8`.

**Measurement** (15 runs, interleaved, `release-fast`):

| benchmark | before | after | change |
|---|---:|---:|---:|
| **bytecount** | 0.3117 s | **0.2142 s** | **-31.3 %** |
| **statemachine** | 0.2002 s | **0.1541 s** | **-23.0 %** |
| **matmul** | 0.0434 s | **0.0392 s** | **-9.7 %** |
| sieve | 0.0292 s | 0.0317 s | +8.5 % |
| bitmap | 0.0522 s | 0.0549 s | +5.1 % |
| bubblesort | 0.0562 s | 0.0581 s | +3.5 % |

**Conclusion. It holds where there is a constant division and it moves the
placement everywhere else.** `sieve` has no division in any of its three hot
loops — its number is the address lottery of round 4 again, this time caused
by `print_u64` getting shorter.

In the same round the conversion learned to write into its target register
instead of going through `rax`: `Op::Load` has taken that route since round
51, the `Op::Cast` right next to it did not, and `bytecount`'s fill loop had
`mov r10, rax / mov rax, r10 / mov r11, rax` in it. `sieve` at `release-fast`
went from 241,974,999 executed instructions to 231,971,880 (**-4.1 %**,
callgrind, exact).

---

## Round 7 — the range analysis: checks that can never fire

**Hypothesis.** `docs/ROUND90.md` §4 named it: "LLVM proves most of its
checks away and Firn proves none of them away." Prove them away.

**Setup.** Executed instructions before the round (`tools/bench90/icount.py`,
exact):

| | `release-fast` | `release-safe` | the checks cost |
|---|---:|---:|---:|
| matmul | 501,828,373 | 1,917,415,771 | **3.82x** |
| bubblesort | 306,632,250 | 821,296,445 | **2.68x** |

**The change.** `compiler/src/rangecheck.rs` — an interval per value, in the
mathematical value (`i128`), from three sources: the structure (`x % k` lies
in `[0, k-1]`, a checked index in `[0, len-1]`, sums and products of bounded
values, but only where the result cannot leave the type, because `Op::Bin`
wraps), the branches on the way in, and the type. A checked operation whose
operands' intervals cannot leave the type becomes the plain one. Runs in the
`bce` slot, which is debug preserving — removing a panic that cannot happen
changes no value and no call stack — so it applies at `dev-fast`, the default
level, as well.

**The part that had to be got right, and it took a second attempt:** the
facts. `opt.rs::known_facts` walks up the chain of single predecessors and
stops at the first join — and a loop HEAD is a join, so everything a loop
guard says is lost one loop deeper. `matmul`:

```text
bb1: %27 = cmp.lt.u64 %148, %1 ; brcond %27, bb2, bb3
bb4: %32 = cmp.lt.u64 %149, %1 ; brcond %32, bb5, bb6   <- two predecessors
bb5: %36 = checked_mul.u64 %148, %1                     <- needs %27
```

With the chain walk, `i < 240` was invisible exactly where `i * 240` stands
(122 check sites in `matmul`, 96 after). The rule that works is the dominance
one: a branch proves its condition in every block its successor DOMINATES,
provided that successor has no other predecessor. **54 check sites.**

**Measurement.** Executed instructions, `release-safe`, exact:

| benchmark | before | after | change |
|---|---:|---:|---:|
| **matmul** | 1,917,012,576 | **1,083,303,381** | **-43.5 %** |
| sieve | 421,552,109 | 391,548,898 | -7.1 % |
| bubblesort | 821,260,528 | 767,210,046 | -6.6 % |
| fib | 303,114,138 | 289,015,750 | -4.6 % |

Wall clock, 9 runs, interleaved, `release-safe`:

| benchmark | before | after | change |
|---|---:|---:|---:|
| **matmul** | 0.2113 s | **0.1004 s** | **-52.5 %** |
| **bitmap** | 0.0756 s | **0.0617 s** | **-18.4 %** |
| **bytecount** | 0.2772 s | **0.2333 s** | **-15.8 %** |
| statemachine | 0.1451 s | 0.1380 s | -4.9 % |
| sieve | 0.0417 s | 0.0400 s | -4.1 % |
| bubblesort | 0.1069 s | 0.1032 s | -3.4 % |

`release-fast` is unchanged to the tenth of a percent, as it has to be —
there are no checks there to remove.

**Counter-check, and it is the one that matters:** `bash tools/checked/run.sh`
— **150 checks passed, 0 failed**. A program that really does go out of range
still aborts with exit 101 and the same message, in `dev`, `dev-fast` and
`release-safe`, out of both compilers. `bash tools/optlevels/run.sh`: ok.

**Conclusion. The hypothesis holds, and the price of safety in Firn is now
0.98x** — over the eleven benchmarks the checked build is, in the median, as
fast as the unchecked one. It was 1.05x going into this round and 1.97x
before round 90.

---

## Round 8 — `lea` for scaling

**Hypothesis.** Scaling an index is the most frequent multiplication there
is, and `mov` + `shl` is one instruction too many for it.

**Setup.** `matmul` at `release-safe`, innermost loop: the checked pointer
addition blocks the address folding, so the `* 4` of every index really is
emitted — twice per iteration — as `mov rdx, rsi` + `shl rdx, 2`.

**The change.** `lea` with a scale for the constants 2, 3, 4, 5, 8 and 9
(`[x+x]`, `[x+x*2]`, `[x*4]`, `[x+x*4]`, `[x*8]`, `[x+x*8]`), 64-bit
operands only — a 32-bit `lea` would compute with a 32-bit address size,
which is a different question from a 32-bit result.

**Measurement** (11 runs, interleaved):

| benchmark | `release-fast` | `release-safe` |
|---|---:|---:|
| matmul | **-3.0 %** | **-1.8 %** |
| xxhash | **-7.4 %** | -0.5 % |
| bitmap | -2.4 % | +0.4 % |
| bubblesort | +0.5 % | -0.5 % |
| jsonscan | +0.3 % | +1.0 % |

**Conclusion. Small and real.** Two instructions out of twenty in `matmul`'s
checked inner loop, and nothing regresses beyond the noise of this machine.

---

## Round 9 — `&&` and `||` had been living in memory since round 92

**Hypothesis.** `jsonscan` is 4.00x behind `rustc -O`, the worst of the whole
set. Before optimising anything, find out whether it is the code or the
amount of work.

**Setup.** Callgrind, exact, both sides:

| | executed instructions | per octet of input |
|---|---:|---:|
| Firn `release-fast` | 1,731,000,586 | 64 |
| `rustc -O` | 335,294,975 | 12.4 |

**5.16x the instructions.** So it is not the code generator. The disassembly
of `scan`, one comparison out of a `||` chain:

```text
cmp r10b, 123
sete al
movzx eax, al
mov qword ptr [rbp-224], rax      <- the value, spilled
mov al, byte ptr [rbp-224]        <- read back
mov byte ptr [rbp-3001], al       <- into the bool cell
cmp byte ptr [rbp-224], 0
jnz .Lscan__bb7
```

Eight instructions and three memory accesses for `c == 123`. All six `||`
cells of `scan` were still `alloca`s in the FIR.

**The cause, and it is a hole between two passes.** `&&` and `||` merge their
result through a bool cell. Two passes can get rid of it:

* `threading.rs::thread_bool_cells` threads the jumps past the cell —
  `cmp` + `jcc`, two instructions.
* `mem2reg::promote_allocas` turns the cell into a phi.

Round 92 gave the second one phis, and added a guard to the first one:

```rust
// ROUND 92: this pass REDIRECTS edges ... So a function
// that already carries phis is left alone; by then `mem2reg.rs` has
// taken over the cells anyway (see `fork_cells`).
if f.has_phi() { return 0; }
```

It does not take them over. `promote_allocas` skips exactly these cells
(`fork_cells`) so as not to delete round 51's optimisation. So in any
function that has **both a loop and a `&&`/`||`** — the loop counter makes a
phi, the phi stops `thread-bool`, and `mem2reg` waits for `thread-bool` — the
cell was dropped by both and stayed in memory. That is the tokenizer, the
JavaScript engine, the CSS parser and `scan`.

**The change.** The condition that matters is not "does this function have a
phi" but "does the block I am about to jump into have one". `fork_at()` is
now the one place that decides what a fork is, it asks that question per
fork, and **both** passes ask it — so `mem2reg` leaves alone exactly the
cells `thread-bool` really takes, and no cell falls between the two.

**Measurement.** Memory accesses in `scan` fell from 95 to 33.

| | executed instructions | wall clock (9 runs, interleaved) |
|---|---:|---:|
| before | 1,731,000,586 | 0.3021 s |
| after | **1,178,500,537** (-31.9 %) | **0.1677 s** (**-44.5 %**) |

**Conclusion. The hypothesis was right that it was not the code generator,
and the cause was a regression, not a missing optimisation.** `jsonscan` goes
from 4.00x to about 2.2x. Every other benchmark moves inside the noise
(statemachine +1.0 %, xxhash +1.7 %, branchy +2.9 %, bitmap -1.7 %) — they
have no `&&` or `||` in a loop.

---

## Round 10 — the same threading, one step later: through a phi

**Hypothesis.** Round 9 handed the bool cells of `&&` / `||` to `mem2reg`
wherever `thread-bool` could not take them. A phi is far better than a
memory cell and it is still not what the machine wants:

```text
cmp r15b, 32 / sete al / movzx r14d, al / test r14b, r14b / jnz ...
```

five instructions where `cmp` + `jcc` is two. If the join block does nothing
but branch on a **bool phi**, every predecessor already knows the answer on
its own edge and can jump straight past the join. The claim is that this is
the same optimisation as round 51's, one SSA step later, and that it is worth
another slice of `jsonscan`.

**Setup.** `thread_bool_phis` in `compiler/src/threading.rs`, run in the same
pass slot as `thread_bool_cells`. A join is a block of EXACTLY ONE
instruction — a bool phi — whose terminator is `brcond` on that phi, and
whose two arms carry no phi of their own (an edge arriving there would need
an entry, and no pass may invent the value that travels along an edge that
did not exist — round 92). The three rewrite rules are round 51's, with the
value read out of the phi entry instead of out of the last `store`:

* predecessor ends `br J`            -> `brcond v, T, E`
* predecessor ends `brcond v, J, X`  -> `brcond v, T, X`
* predecessor ends `brcond v, X, J`  -> `brcond v, X, E`

and only where the phi's entry for that edge IS that same `v`.

**What it cost to get right, and this is the part worth keeping.** The first
two builds both passed `jsonscan` and both broke the test suite, in two
different ways, and both had the same root: *what is left standing where the
join used to be.*

1. **`block bb7 in 'thread_channel_send' has no terminator`** — 20 failures,
   every one of them a GC or thread test. A join whose LAST predecessor is
   rewritten is dead, and what stays behind is `%x = phi.bool []`, a phi with
   no entries at all. `dce` will not take a block apart while it still sees
   that block's values read — and two dead joins referring to each other are
   exactly that case — so the corpse travelled on into `regalloc`, which
   found a block it could not lay out. Measured, not guessed: `--emit=fir-opt`
   printed `bb1: %68 = phi.bool []` and five blocks reading `<unset>`, and
   `--no-pass=merge-blocks` printed none of them, which named the shape the
   corpse had to have.

2. So the corpse was made the one `merge_blocks` leaves — no instructions,
   `Term::Unset`. **That was worse, and it was silent:**
   `condition %68 in '__thread_work' is u64, expected bool`. Clearing the
   instructions deletes the DEFINITION of `%x` while a (likewise dead)
   `brcond %x` still names it. Inside the function nobody minds. But
   `inline.rs` copies a callee value by value, and a value with no defining
   instruction gets no entry in the remap table — so the old id travels into
   the CALLER unchanged and lands on whatever value happens to carry that
   number there. Bisecting the pass pipeline with a per-pass type check on
   every `brcond` put the damage at `nach-inline`, three passes before the
   one that looked guilty.

   **The rule this leaves behind: a pass may delete a block's control flow,
   but not a definition another instruction still names — not even in dead
   code, because `inline` reads dead code too.**

3. What works: the definition stays, only the phi goes. An unreachable block
   may compute anything at all, so `%x = phi.bool []` becomes `%x = const 0`,
   and `dce` removes it in the ordinary way.

**Measurement**, `firnc` before this round against `firnc` after it, both at
`release-fast`, interleaved, median of 9 runs each:

| benchmark | before | after | |
|---|---:|---:|---:|
| **jsonscan** | 0.1670 s | **0.1476 s** | **-11.6 %** |
| matmul | 0.0443 s | 0.0438 s | -1.1 % |
| memstride | 0.2346 s | 0.2322 s | -1.0 % |
| xxhash | 0.2875 s | 0.2848 s | -0.9 % |
| branchy | 0.5344 s | 0.5307 s | -0.7 % |
| bitmap | 0.0602 s | 0.0598 s | -0.6 % |
| sieve | 0.0290 s | 0.0289 s | -0.2 % |
| bubblesort | 0.0752 s | 0.0754 s | +0.2 % |
| fib | 0.0445 s | 0.0447 s | +0.5 % |
| bytecount | 0.2304 s | 0.2323 s | +0.8 % |
| statemachine | 0.1384 s | 0.1422 s | +2.7 % |

Median of the change: **-0.6 %**. Emitted assembler for `jsonscan`: 938 lines
-> 875, and not one line moved in `statemachine`, `branchy`, `bytecount` or
`sieve` — they have no `&&` or `||` whose cell survived into a phi.

**Conclusion. The hypothesis holds, narrowly and exactly where it said it
would.** Everything outside `jsonscan` is this machine's noise; the +2.7 % on
`statemachine` is a benchmark whose assembler is byte for byte identical
before and after, which is a useful reminder of what a wall clock on a shared
machine can and cannot resolve. Gate after the fix: 329 programs x 4 build
levels = 1316 runs, 0 failures; `cargo test`: 262 passed.

---

## Round 11 — a corpse `dce` would not bury

**Hypothesis.** Round 10 passed a gate of 1316 runs and still broke the full
suite: `internal error: block bb36 in 'domjs__attr_fn' has no terminator`,
in `lib/browser/b4_main.fi`, at every build level. The claim is that this is
not a third bug in the new pass but the SAME shape as round 10's first
failure, one layer further out — and that the fix therefore does not belong
in `threading.rs` at all.

**Setup.** `--emit=fir-opt` on the failing file: exactly one block reads
`<unset>`, no phi is empty any more, and `--no-pass=merge-blocks` prints
none. So `merge_blocks` is the one that leaves it, which it always did:
its corpses are an EMPTY block with `Term::Unset`, and it counts on
`remove_unreachable_blocks` to remove them.

**The cause.** That pass has a safety net from an earlier round: if a value
defined in an unreachable block is still read from anywhere, it removes
**nothing at all** — better dead code than a dangling `Val`. Round 10 made
that combination reachable for the first time. It turns joins unreachable,
and a value of a now dead block can still be named by another dead block —
so the net trips, `merge_blocks`'s corpses stay, and `regalloc` finds a
block with no terminator and gives up on a block that is never entered.

**The change** (`opt.rs`, in the net's own branch): an unreachable block
that is EMPTY and has no terminator is closed with a jump to itself. It is
unreachable, so the jump is never taken; it names no value, so it changes no
live range; and it is a block every back end can emit. Blocks that still
carry instructions are left alone — deleting a definition another
instruction names is what round 10 already paid for once.

**Measurement.**

| | before | after |
|---|---|---|
| `lib/browser/b4_main.fi`, four build levels | fails at all four | **builds at all four** |
| `tools/liveb4/run.sh` | 3 failures | **1** (a count under load, see below) |
| gate, 329 programs x 4 levels | — | **1315 / 1316** |
| `cargo test` | — | **262 passed** |
| `test_opt.sh` | — | **45 / 45** |

**The one gate failure, and it is not this round's.**
`tests/860_thread_basic.fi [safe]` returned 14 — the test's own
counter-check, which demands that an UNGUARDED counter lose a race against
four threads. Run 60 times back to back on the same machine:

| compiler | runs where the race did not happen |
|---|---:|
| `firnc` before this round (HEAD) | **56 / 60** |
| `firnc` after it | **31 / 60** |

The test is probabilistic and it is flaky on this machine either way; the
change makes it flake *less*. It is named here so the next round does not
spend an hour on it.

**Conclusion. The hypothesis holds: the bug was not in the new pass.** Round
10 was the first pass to combine "turn a join unreachable" with "a dead
block still names a dead value", and it found a hole that had been sitting
between `merge_blocks` and `dce` since long before this round. Anything that
makes blocks unreachable would have found it eventually.

---

## Where the round stands

**Measured** (`bench/bench.py`, 9 runs, two passes, `bench/RESULTS.md`):

| benchmark | at the start (as quoted in ROADMAP 4.9) | now, `release-fast` |
|---|---:|---:|
| fib | 1.52x | 1.64x |
| bytecount | 1.43x | 1.30x |
| statemachine | 1.99x | 1.71x |
| bubblesort | 2.17x | 1.84x |
| matmul | 2.90x | 2.01x |
| **sieve** | **4.16x** | **1.01x** |
| **median** | **2.08x** | **1.67x** |

`release-safe` — the same passes with EVERY integer operation checked, so
Firn doing strictly more work than Rust — is at a median of **1.52x**.

**The target was median under 1.5x and `sieve` under 2.5x.** `sieve` is met
with room to spare. The median is not: 1.67x, and `matmul` at 2.01x is what
carries it now.

**What carried and what did not.**

* **Carried:** the build level in the measurement (round 1 — worth the most
  of anything in this round, and it was a measurement error, not code);
  block layout and loops in one piece (rounds 2, 5); the exact loop depth
  (round 3); division by a constant (round 6); the range analysis (round 7,
  which brought the price of safety to 0.98x); `lea` for scaling (round 8);
  the bool cells of `&&` / `||` (round 9, `jsonscan` -44.5 %); threading
  through a bool phi (round 10, `jsonscan` -11.6 %).
* **Did not carry:** aligning loop heads (round 4) — a coin toss on this
  machine, and it stayed out.
* **Named and not done:** no vector instructions are emitted, and the
  register allocation is linear rather than colouring. Those two are what
  `matmul` is waiting for, and they are the next round.

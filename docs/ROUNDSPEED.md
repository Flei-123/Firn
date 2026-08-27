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

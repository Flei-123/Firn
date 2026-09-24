# TEMPO 14 -- short loops unrolled completely

State before: TEMPO 13 (`xmm-ra`) merged onto `main`. MP3 decoder, 8 s of
sound: **124.0 million instructions** (callgrind), `minimp3` in C with
`gcc -O2`: 74.8 million. The largest single gap was `synth`: 35.0 million
against 22.7 million in C.

## Why

The hot loop of `synth` (`lib/ton/mp3.fi`, branch `ton`) is

```firn
var k: i32 = 0
while k < 8 {
    ...
    if k == 0 { ... } else { if (k & 1) == 1 { ... } else { ... } }
    k = k + 1
}
```

Eight passes, and every pass pays for the loop test, for `k == 0` and for
`k & 1` -- compares and branches whose outcome the compiler could know,
because the trip count and every value of `k` are fixed at compile time.
`gcc -O2` peels such a loop completely and the branches fold away.

## What is built

`compiler/src/unroll.rs`, pass `unroll` (slot 13, runs after `memset`,
before `bce`; debug preserving -- every copied instruction keeps its source
position, so it runs in `dev-fast` as well).

1. **Recognition.** A natural loop with one back edge, one predecessor
   outside, a header whose `brcond` is the ONLY way out, and no inner loop.
   The condition is a compare of a header phi with a constant; the phi
   starts at a constant and is stepped by `+`/`-` a constant (plain,
   wrapping, or checked).
2. **Trip count by simulation**, in the counter's own type and signedness --
   so `<`, `<=`, `!=`, counting down, odd steps and a `u8` that wraps are all
   the same question. A checked step is accepted only if the simulation
   shows it never overflows on a pass the loop really makes.
3. **The copy.** The loop blocks are copied once per pass. In copy `j` a
   header phi is not an instruction but a name for an existing value (the
   preheader's for `j = 0`, the back edge value of copy `j - 1` otherwise).
   The header's `brcond` becomes `br`, the back edge of copy `j` enters the
   header of copy `j + 1`; one more header copy evaluates the header once
   more (its values may be read behind the loop) and jumps to the exit.
   Nothing is folded in the pass itself -- `fold`, `simplify-term`,
   `merge-blocks` and `dce` do that in the same fixpoint loop.
4. **Size limit.** At most 16 passes and at most **640 FIR instructions**
   after copying. `FIRN_UNROLL_BUDGET=<n>` overrides the limit,
   `FIRN_NO_UNROLL=1` switches the pass off, `FIRN_UNROLL_TRACE=1` prints
   every candidate with its size and the decision.
5. **Left alone:** loops with a `break` or `return` in the body, inline
   assembler, `clone`, `secret` values, `#[constant_time]`.

## A bug the merge brought to light

Before this round, TEMPO 1-13 (`xmm-ra`) were merged onto `main`. The fUi
check (`tools/wasm/webdemo.sh` -> `tools/fui/x11live.py`, step L8: x11demo at
`Xft.dpi: 192`) crashed in `release-fast`: `canvas_fill_rect` wrote through a
wild pointer. Cause, in `regalloc::foldable_addresses`: after phi
elimination and coalescing, the `q` of

```firn
var q: u64 = px + ((y * w + ix0) * 4) as u64
while k < ix1 { ...; q = q + 4; k = k + 1 }
```

is ONE value with TWO writers. The scaled sum of the first writer (TEMPO 3,
`lea q, [px + idx*4]`) was keyed by the value and emitted for the second
writer as well: `q = q + 4` became `lea r9, [r8+r9*4]`. Every address fold
now requires a value with exactly one writer (and so does the offset it
removes). `tests/1705_fold_one_writer.fi` is the loop cut down; it crashed
before the fix and passes with it (and on the old `main`, which has no
scaled fold). MP3: 124,033,353 -> 124,033,313 instructions, i.e. the fold
never hit a two-writer value there.

## Choosing the limit (measured, MP3 8 s, instructions)

| budget | total | `synth` | `dct_ii_4` | `.text` MP3 | `.text` firnc1 (release-fast) |
|---|---|---|---|---|---|
| off (merge state) | 124.03 M | 34.96 M | 11.28 M | 210,653 | 1,191,580 |
| 256 | 116.35 M | 34.96 M (8 x 37 = 296, too big) | 11.28 M | -- | 1,193,816 |
| 320 | 109.06 M | 27.64 M | 11.28 M | 209,808 | 1,193,816 |
| 512 | 107.66 M | 27.64 M | 10.34 M | 213,250 | -- |
| **640** | **106.81 M** | **27.64 M** | **9.49 M** | 214,587 | 1,198,184 |
| 1000 | 106.54 M | 27.64 M | 9.49 M | 220,436 | -- |

Past 640 only `l3_change_sign` (16 x 107) joins: 0.27 M instructions for
another 2.7 % of code. 640 is where the curve flattens. The self-hosting
compiler grows by 0.6 % (`release-fast`) and 0.6 % (`dev-fast`); its
compile time did not move (3.9 s dev-fast, 4.6 s release-fast, both with
and without the pass).

**What it costs in code.** The self-hosting compiler +0.6 %, the MP3
decoder +1.9 %, fUi's x11demo +2.5 % (`release-fast`) / +2.8 %
(`release-safe`), the fUi gallery as WebAssembly 482,521 -> 507,167 octets
(+5.1 %). Two ways to cut that were measured and did not pay: at most 9
passes instead of 16 (503,730 octets, the growth sits in short loops), and a
separate small budget for loops that save nothing but their own test
(0 to 128 instructions: 503,714 octets, but MP3 108.1 M instead of 106.8 M)
-- the growth comes from exactly the loops that win. At most 4 passes would
keep the gallery at +1.1 %, and loses `synth`.

**Result: 124.0 -> 106.8 million instructions (-13.9 %), bit identical**
(8 s and 60 s against `ref60.pcm`). Per function:

| function | before | after | C (`gcc -O2`) |
|---|---|---|---|
| `synth` | 34.96 M | 27.64 M | 22.7 M |
| `l3_imdct36` | 16.91 M | 11.70 M | 12.9 M |
| `dct_ii_4` + `dct_ii` | 14.92 M | 12.69 M | 11.1 M |
| `l3_huffman` | 16.04 M | 15.56 M | 12.1 M |
| `l3_antialias` | 2.89 M | 1.79 M | -- |

`l3_imdct36` is now below C.

Benchmark bank (`bench/firn`, instructions, output identical everywhere):
`gc_barrier` -16.3 %, everything else unchanged (none of the other programs
has a short counted loop in its hot path).

## Tried and dropped

**Folding the pointer chains.** After unrolling, `synth` walks three
pointers as chains (`p1 = p0 - 256`, `p2 = p1 - 256`, ...). Rewriting
`(x + c1) + c2` to `x + (c1 + c2)` was built and measured: 106.81 ->
106.95 M, i.e. worse. The code generator folds `base + constant` into an
address only for a plain `load`/`store` directly behind it and only for
offsets >= 0 -- not for `simd.Load`, and not for the negative offsets of
`pz`. So the constants became values of their own and cost registers. It
pays only together with address folding for the vector loads (see below).

## Tests

`tests/1704_unroll.fi` (eleven shapes: branches on the counter, count down,
`<=`, `!=`, wrapping `u8`, counter read after the loop, zero and seventeen
passes, nested, array by index, f32 order, `break`) in every build level;
`tests/opt/unroll_counted.fi` + a line in `test_opt.sh`: after the optimizer
no `cmp.lt.i32` is left of the counted loop. `FIRN_VERIFY_PHI=2` reports no
phi breakage by `unroll` (the two transient ones after `simplify-term` in
`rt__read_fd` / `io__read_line` are there without the pass too).

## What is worth doing next

1. **`synth` without the detour through memory.** `synth` + `scale_pcm4` +
   `synth_pair` are 37.1 M against 22.7 M for C's `synth` (which has the
   conversion inline). Inside `synth` after this round (callgrind per
   instruction): 6.6 M `mov` -- mostly addresses of `ai`/`bi` and hoisted
   constants reloaded from an 11 KB frame --, 4.2 M `lea` (the pointer
   chains above), 2.6 M `movaps` (two operand SSE; `--cpu=avx` removes
   them). Converting to `i16` in registers instead of four arrays and a
   call per outer pass (`scale_pcm4`: 7.2 M) plus address folding for
   `simd.Load` with negative offsets (then the chain rewrite pays) is worth
   an estimated **8-10 M** of the 14 M gap.
2. **Strength reduction of induction variables + partial unrolling** for
   loops whose trip count is only known at run time. `matmul` is the
   largest factor of the bank (2.66x instructions against `rustc -O`): the
   inner loop costs Firn 11 instructions per pass (among them `imul k*n`
   every pass), Rust 3.75 (unrolled four times, the pointer stepped by
   `4n`, `imul` with a memory operand).

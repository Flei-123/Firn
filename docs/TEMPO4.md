# Round TEMPO 4 — SIMD becomes usable: `v128` in the path with register allocation

State 21.09.2026, branch `xmm-ra`. Everything measured: instructions with
`valgrind --tool=callgrind` (8 s of sound), times as the smallest of nine runs with
output to `/dev/null`.

## The finding that triggered this round

After TEMPO 3 the MP3 decoder stood at 304 M instructions, `minimp3` in C at
75 M. A third of the difference has a single cause: **`gcc -O2`
pours the loops itself into `packed` instructions** — four `f32` per instruction.
Without vectorisation the same C needs 0.10 s instead of 0.07 s (measured with
`-fno-tree-vectorize`).

Firn already had `v128` as a language feature (round 82: 42 intrinsics for
AES, SHA, CRC, byte shifting) — **but not a single floating-point instruction**, and
above all: *every* function with a `v128` fell back to the basic path of the
generator, so without register allocation. Whoever used vectors
lost the allocation for the whole function — address calculation included.
Thereby SIMD in Firn was practically unusable as soon as anything else was
calculated around it.

## What was built

### 1. Three packed floating-point instructions (both machines)

`__v128_addf32`, `__v128_subf32`, `__v128_mulf32` — x86: `addps`, `subps`,
`mulps`; aarch64: `fadd`, `fsub`, `fmul` with `4s`. They calculate **per lane
exactly what the single instruction calculates**; an unrolled loop that is
switched to four lanes therefore stays bit-identical. That is checked
in `tests/1615_simd_f32.fi` lane by lane against the single calculation, in
all four build stages.

### 2. `v128` in the path with register allocation

`unsupported_basic` now lets a **narrow selection** in: `__v128_load`,
`__v128_store`, `__v128_zero`, the three calculations and `__v128_shuffle32`.
In addition `Op::Load`/`Op::Store`/`Op::Copy` with `v128`. Everything else
(crypto, byte shifting, reading and writing individual lanes) still goes
through the basic path with the cache from `simd.rs` — it is rightly
placed there, because this path can do it.

Necessary for that:

* **Slots for `v128`**: sixteen octets, sixteen-fold aligned
  (`layout`). `rbp` stands after the prologue on a multiple of
  sixteen, so a distance that is one suffices. With that `movaps` may go
  between register and slot; only the accesses through a pointer from
  the program stay `movdqu`.
* **Registers**: `v128` belongs in the same class as `f32`/`f64` (`xmm`), and
  the second pass of the linear scan from round XMM 3 distributes them along. The
  condition from there still applies: whoever survives a call keeps its
  slot (all sixteen `xmm` are caller-saved).
* **`mem2reg` promotes vector variables.** Excluded until now, because the
  copy that `phi.rs` makes from a `phi` was emitted on the basic path as an
  INTEGER copy — eight of sixteen octets. Both paths
  can do it now (`simd::emit_copy_v128`).
* **`--cpu=avx`** knows the new instructions: `vaddps`/`vsubps`/`vmulps`
  three-operand, `vpshufd`, `vmovdqu`.

### 3. Drei Stellen im Dekoder auf vier Spuren umgestellt

| Function | why it fits |
|---|---|
| `synth` (the hottest function of the decoder) | the four sums calculate the same with ADJACENT values and the same two window weights; `__v128_shuffle32` pulls each weight onto all four lanes |
| `l3_midside_stereo` | `a+b` and `a-b` over two rows, remainder of the length individually |
| `l3_antialias` | `u` ascending, `d` descending — both lie four values side by side each, `__v128_shuffle32(.., 0x1B)` reverses the order back and forth |

The window table `win` got **four values of padding** for this: the
weight pair is fetched as a 16-octet load (two values needed, four
read), and with that the last pair also stays inside the table.

## Die Zahlen

MP3-Dekoder, 60 s Ton, `release-fast`:

| | Instructions (8 s of sound) | Time (60 s of sound) |
|---|---|---|
| after TEMPO 3 | 304.4 M | 0.24 s |
| + `synth` on four lanes | 272.3 M | |
| + `l3_midside_stereo` | 267.3 M | |
| + `l3_antialias` | **258.6 M** | **0.21 s** |
| the same with `--cpu=avx` | **235.4 M** | **0.21 s** |
| `minimp3` in C, `gcc -O2` | 74.8 M | 0.07 s |
| the same C, `-O2 -fno-tree-vectorize` | — | 0.10 s |

**Distance to C: 3.0x** against `gcc -O2`, **2.1x** against the same C without
auto-vectorisation (before this round: 3.4x and 2.4x).

Correctness: after **each** of the three steps the PCM output is bit-identical
(`cmp` over 10.6 MB) and the self-test gives PASS 4/4 — in `baseline` as in
`avx`. That is no coincidence, but the property on which the whole round
stands: `addps` is four times `addss`, lane by lane, with the same rounding.

## What would be possible next

Measured, in the order of weight in the decoder:

1. **`dct_ii` (45 M) and `l3_imdct36` (44 M)** — the same conversion as
   `synth`, but more work: the loops are unrolled by hand and
   first have to be sorted back into lanes.
2. **`scale_pcm` (28 M)** — needs three more instructions
   (`cvttps2dq`, `cvtdq2ps`, `packssdw`) and an exact emulation of the
   edge cases (`floor(x+0.5)` and the clamping at ±32767), otherwise the
   output is no longer bit-identical.
3. **The integer side.** In `synth` there now stand 204 `mov`, 31 `add`,
   27 `lea` against 26 `movaps` and the packed calculations — the
   address calculation is half the work. That is the next big
   chunk and has nothing to do with SIMD (advance pointers instead of recalculating
   addresses, cut lifetimes at calls).

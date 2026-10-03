# Round TEMPO 1 — floating point finished, and a bug from XMM 3

State 20.09.2026, branch `xmm-ra`. Everything here has **been run and measured**;
the commands are given with it.

## Ausgangslage

Round XMM 3 taught the register allocator floating point. The MP3 decoder
then ran 60 s of sound in **0.91 s**, the same template in C (`minimp3`,
`gcc -O2 -DMINIMP3_NO_SIMD`) in **0.11 s** — a factor of 8.3. A measurement of
the instructions really executed (`valgrind --tool=callgrind`) said what
the cause was: **644 million** against **75 million** instructions. So it was not the
order and not the cache, but the sheer amount of
work generated.

`perf` does not work in this container (`perf_event_paranoid` is fixed),
callgrind does. The mapping address -> function comes from `nm`.

## What was found, in order of effect

### 1. A `-x` sent half the decoders to the basic path (translator)

`FIRN_RA_STATS=1` says which function gets no register allocation.
Answer: **the seven hottest** — `l3_imdct36`, `l3_huffman`,
`synth_pair`, `scale_pcm` and three more, all with the same reason
*"one-operand calculation with a floating-point number"*. XMM 3 had excluded `Op::Un`,
so **every** function in which somewhere a `-x` stands on a floating-point number.

The sign reversal is a bit: `xorps` against a mask that comes via `rax`
into `xmm1` (SSE has no form with a constant). With that the
exclusion drops out; `!` on a floating-point number stays outside, because it
has no meaning at all.

**644 Mio -> 467 Mio Befehle, 0,91 s -> 0,64 s.**

### 2. The address calculation of the decoder had a jump (library)

`lib/ton/mp3.fi` calculates with signed indices, because the
synthesis filter bank really needs negative ones (`zlin[4*(i-16)+2]`). That stood
as `if i < 0 { p - (-i)*4 } else { p + i*4 }` — before **every single**
field access a comparison, a jump and a phi.

The branch is not necessary: addresses calculate modulo 2^64, and `i as u64` is,
at equal width, a reinterpretation of the bit pattern (checked in `dev`, `release-safe`
and `release-fast`). `p +% ((i as u64) *% 4)` is for negative `i`
the same — without a jump. Only with that can the translator fold the
access into the address part of the instruction at all.

**467 Mio -> 370 Mio, 0,64 s -> 0,37 s.**

### 3. `+%` was invisible to every optimisation (translator)

And here the measurement showed the actual joke: `Op::BinWrapSat { kind:
Wrap }` and `Op::Bin` mean **the same** in FIR — both keep the
lower bits, neither checks anything (checked arithmetic is called
`Op::CheckedBin`). The difference was purely syntactic. Only **every**
optimisation asks for `Op::Bin`: common subexpressions, loop invariants,
the algebraic simplifications and above all folding the address into the
instruction (`regalloc::foldable_addresses`). A `+%` bypassed all of them.

`canon_wrap` rewrites `Wrap` once before all passes into `Op::Bin`.
`Sat` stays untouched — clamping really is something different.

**370 Mio -> 338 Mio, 0,37 s -> 0,34 s.**

### 4. The sign changes no bit (translator)

`copy_propagate` made a conversion disappear only if both sides were
of equal width **and equally signed**. Thereby a real `mov` stayed behind from
`(i as u64)` — in the decoder before every access. At
equal width the pattern is identical; whether it is read as negative
is decided solely by the instruction that uses it (each carries its own
type). Bool and floating-point numbers stay separate, the **checked** conversion
is a different instruction.

### 5. Drei Kleinigkeiten im Fliesskommaweg (Uebersetzer)

* **Swap instead of copy:** if for `a + b` / `a * b` the second
  operand already lies in the destination register, it is swapped — that saves the rescue to
  `xmm1` and the copy of the first.
* **Floating-point parameters** may stay in their register (the prologue
  has long been able to do that; they were excluded from the time when this
  path emitted no floating point).
* **`-1.5f` is folded:** the sign of a constant is a bit, not a
  calculation step. Before, in `scale_pcm` — the innermost loop —
  there stood load constant, load mask, `xorps`.

**338 Mio -> 324 Mio.**

## And a wrongly generated program from round XMM 3

`tests/1452_f32_abi.fi` gave **6 instead of 0**, in `release-fast` as in
`release-safe`. The bug sat in XMM 3 itself (reproduced with the state of
`14f9ee3c`), not in this round — it was found only by the
full test run here.

Cause: `fp_taugt` decided "may this value go into an xmm?" by the **kind**
of the instruction (`Bin`, `Cmp`, `Cast`, `Copy`, `Store`: always fine). That is too
generous. The calling convention copies an aggregate **eight bytes at a time**,
and in doing so there stands a `store.u64` with a value whose type is `f64` — a
`struct { f32, f32 }` travels as one eight-byte in `xmm0`. This instruction goes
through the integer path, fetches its operand with `mov`, and if the value
in the meantime lived in an `xmm`, it wrote nonsense into the frame. In the
generated code there stood literally `mov qword ptr [rbp-360], rbp`.

Now what counts is not the kind of instruction, but whether it **really** stands in the
floating-point path — that is, exactly the condition under which the output takes its
floating-point branch (`inst.ty.is_float()`, for the comparison the type of
the comparison, for the conversion one of the two sides).

## Die Messung

MP3 decoder, 60 s of sound (1.4 MB, 192 kbit/s, stereo), `release-fast`, smallest
of nine runs, same machine:

| | Instructions (callgrind, 8 s of sound) | Time (60 s of sound) |
|---|---|---|
| XMM 3 (starting point) | 644 M | 0.91 s |
| after this round | **324 M** | **0.31 s** |
| `minimp3` in C, `gcc -O2` | 75 M | 0.11 s |

**Distance to C: from 8.3x to 2.8x.**

Correctness: the PCM output is **bit-identical** to before (`cmp` on the
whole file) and bit-identical to C (`mp3_pruef_main` -> PASS 4/4 over four
streams: MPEG-1 stereo, MPEG-2 mono, MPEG-2.5 8 kHz, short blocks).

Integer programs do **not** change: `bench/instr.sh` counts for
`bubblesort` and `statemachine` the same instruction (1 of 1.47 billion
difference, that is the start-up setup). This round acts on
floating point, on `+%` and on address calculation with signed
indices — not on every loop.

Test series: `bash test.sh` without a new error; `1452_f32_abi` went from FAIL to
PASS. Three points remain open that have nothing to do with the generator
(the English name probe fires on `lib/fui/*`, the
self-run fixed point wanted `tools/gen_gctext.sh` — caught up —, and
`testdata/test262/subset.sha256` is missing in this working tree).

## What would measurably bring something next

1. **Floating-point cells in `xmm`.** A loop variable
   (`Op::Alloca`, promoted as a cell) today gets only an
   integer register and travels at every access via `movd` back and forth — or
   stays entirely in its slot. In `synth`, the hottest function of the
   decoder (112 M of 324 M instructions), these are eight sums: per
   iteration sixteen trips into the frame and back.
2. **Cut lifetimes at calls.** Whoever survives a call
   today gets no register at all (all sixteen `xmm` are
   caller-saved). In `synth` three base pointers therefore stand in their
   slots and are fetched anew before every access.
3. **Three operands (AVX).** `vmulss d, a, b` saves the copy that `mulss`
   forces — in `synth` there stand 50 such `movaps`. Needs a switch
   for the target CPU; the baseline stays SSE2.

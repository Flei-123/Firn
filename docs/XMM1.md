# Round XMM 1 -- floating point without the detour via `rax`

State 18.09.2026, branch `xmm`. Background: `docs/TON2.md` (in branch `ton`)
measured that Firn is about ten times slower than C at floating point,
but only just three times at integers. The reason lay in the translator, not
in the language -- and this round clears away the first, cheapest part of it.

## What was wrong

`codegen_x86.rs` ferried every floating-point value via `rax`:

```asm
mov  eax, dword ptr [rbp-40]     ; Slot -> Ganzzahlregister
movd xmm0, eax                   ; Ganzzahlregister -> SSE-Register
```

And on the way back the same in the opposite direction. That is TWO
instructions per operand and two per result -- for a multiplication
that is six instead of three. The reason was historical (round 71 introduced floating point
in the first place and took the shortest safe way), not
technical: `movss`/`movsd` read and write memory directly.

## What stands now

```asm
movss xmm0, dword ptr [rbp-40]   ; one instruction
...
movss dword ptr [rbp-48], xmm0
```

Exactly two functions are changed (`load_xmm`, `store_xmm`). Everything
else -- calculation, order, rounding -- stays untouched.

One point had to be decided: `movss` writes only four octets
into an eight-octet-wide slot, the upper four keep their old
content. That is permitted because an `f32` slot is read exclusively as `dword`
(`load_xmm` with `single`, `cvtss2sd`, the argument passing);
whoever copies eight octets copies the upper ones along, without ever interpreting them.

## Messung

All on the same machine, `--opt-level=release-fast`:

| Measurement case | before | after | C for comparison |
|---|---|---|---|
| `f32` kernel (2 M iterations) | 0.50 s | **0.35 s** (-30 %) | 0.03 s |
| MP3 decoder, 60 s of audio | 2.07 s | **1.93 s** (-7 %) | 0.34 s |

The decoder gains less, because it does not only calculate, but also reads
Huffman bits and addresses tables -- that is integer work and was
never affected.

Correctness: `tools/ton_bauen.sh` -> PASS 4/4, the output of the decoder is
still **bit-identical** to the C template. The test series of the repo (`test.sh`)
runs through unchanged.

## What is still outstanding (round XMM 2)

The big remainder still lies where `docs/TON2.md` named it:
**floating-point values do not stay in the register between two instructions.**
Every calculation loads anew from the frame slot and writes the result
back. Two ways lead out of that:

1. **The `xmm` value cache of the basic path also for scalars.**
   `simd.rs` has it already completely -- with a retraction plan, flushing
   at block boundaries and displacement -- but hard-wired to `v128` (16 octets,
   `movdqa`). It needs a width per entry (4/8/16) and the
   matching move instruction. A small intervention, a large part of the gain,
   because most intermediate values are read in the same block.
2. **A second register class in the linear scan** (`regalloc.rs`).
   The clean way, but the bigger one: the allocation has to keep two pools,
   and the output of the RA path so far knows not a single
   floating-point instruction (functions with `f32`/`f64` never
   arrived there). In addition, on System V ALL `xmm` registers are
   caller-saved: a value whose lifetime crosses a call
   needs saving or stays in memory.

Order: first 1, then measure, then 2.

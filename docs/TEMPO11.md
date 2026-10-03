# TEMPO 11 -- `lea` in 32 bits, constant to the right, loop rotation

Measurement basis as always: MP3 decoder, 8 s of sound, instructions by callgrind,
`release-fast`. PCM bit-identical to the gold output after every step.

| Step | Instructions |
|---|---|
| after TEMPO 10 | 135.4 M |
| `lea` with a 32-bit destination + constant to the right | 132.2 M |
| loop rotation (only the comparison in the head) | 129.5 M |
| loop rotation (up to three calculations before the comparison) | **128.5 M** |
| the same with `--cpu=avx` | **120.2 M** |
| C, `gcc -O2` | 74.8 M |

## 1. `lea` also with a 32-bit result (`regalloc.rs`, `lea_possible`)

`i + 1` on an `i32` was `mov rdx,r10` + `add edx,1`. `lea r32,[..]`
forms the address in 64 bits and stores the lower 32; the lower 32 bits
of a sum depend only on the lower 32 bits of the summands. So it
is bit-identical. The displacement in the `lea` is signed, a
32-bit immediate is therefore reinterpreted modulo 2^32 (`0xFFFFFFFF` -> `-1`).
Test: `tests/1701_lea_32bit.fi` (overflow, underflow, large `u32`
constant, `u8`/`i16`).

## 2. Constant to the right (`peephole.rs`)

`4 * i` became `mov $4,r10; imul r8d,r10d`, because the generator asks only
for `imm(b)`. For commutative integer operations (`+ * & | ^`) the
constant now stands on the right.

## 3. Schleifenrotation (`regalloc.rs`, `emit_block`, `Term::Br`)

The measurement (instructions per address) showed 4.46 million unconditional back jumps,
almost all `jmp head`, where the head is only `cmp` + `jcc`. Now at the
end of a block that jumps to such a head stands the head itself:

```
before                         now
  body: ...                      body: ...
        jmp  head                      cmp  r8,r10
  head: cmp  r8,r10                    jl   body
        jge  exit
```

Two instead of three instructions per iteration. The head may have up to three plain
calculations before the comparison (`Bin`, `Un`, `Cast`, `Copy`, `Const`,
`PtrAdd`, `Load`), e.g. `lea 8(r11),rdx; cmp r10,rdx` in `rt.mem_copy`.

Why this cannot break anything: the block jumps with `jmp` -- there is only
this one edge, and all phi copies already stand before it. The
machine state at the end of the block IS the one at the entry of the head, and the
output of an instruction depends only on the fixed slots of its values. Not
admitted is what creates jump labels of its own (checked calculations) or
writes memory/calls. The handover registers (`fp_handover`) apply only
within one block and are copied along.

Can be switched off with `FIRN_NO_ROTATE=1`. Test: `tests/1702_loop_rotation.fi`
(zero iterations, `continue` as a second way back, head with calculation,
nested, floating point, unsigned).

## What still stands there (same count)

- Fetching from the frame: 9.8 M -- above all `l3_huffman` (63 live values).
- `movaps xmm,xmm` of the two-operand form: 8.7 M -- gone with `--cpu=avx`.
- Leaf functions reserve a frame even when they do not need one
  (`sub $0xa0,rsp` in a function without a single frame access).

## Geprueft

Quick test in `release-fast` (320), `release-safe` (316) and `dev-fast`
(316): all good. Fixed point: stage 2 and 3 character-identical (793,453 lines),
`self_compare` 341 of 341 same behaviour, 0 deviating.

Wall clock (60 s of sound, best of nine, machine not quiet): Firn 0.14 s,
with `--cpu=avx` 0.13 s, C `gcc -O2` 0.07 s, C without auto-vectorisation 0.10 s.

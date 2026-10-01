# Round TEMPO 6 — two instructions that C does not even write

State 21.09.2026, branch `xmm-ra`. This round answers a question with
measurement instead of opinion: **why does Firn need about three times
as many instructions as C for the same work?**

## The measurement first

The same sound decoder, the same 8 s of sound, function by function
(`callgrind`, Firn before this round against `minimp3` with `gcc -O2`):

| Funktion | Firn | C | Faktor |
|---|---|---|---|
| `synth` | 59,1 Mio | 22,7 Mio | 2,6x |
| `l3_huffman` | 30,1 Mio | 12,1 Mio | 2,5x |
| `l3_imdct36` | 27,1 Mio | 12,9 Mio | 2,1x |
| `dct_ii` | 18,7 Mio | 11,1 Mio | 1,7x |
| `l3_dct3_9` | 12,4 Mio | 8,0 Mio | 1,6x |

So there is **no single big cause** — it is the same factor everywhere. That is why
the look at the smallest, clearest function is worthwhile:
`l3_dct3_9`, straight calculation without a loop and without address games.
Firn 155 instructions, C 109. The difference stands literally in the generated code:

**Firn (before)**
```text
    mov   r8, rdi
    mov   rcx, r8              <- copy of the address
    movss xmm15, [rcx]
    lea   r9, [r8+8]
    mov   rcx, r9              <- another one
    movss xmm14, [rcx]
    ...
    mov   eax, 0x3f000000      <- build the constant 0.5 at run time
    movd  xmm10, eax           <- and use up a register for it
    mulss xmm9, xmm10
```

**C**
```text
    movss xmm2, [rdi+0x18]           <- offset directly in the instruction
    mulss xmm8, [rip + 0x4ecf]       <- constant from .rodata as operand
```

Two things that C does not even write. Exactly those are gone now.

## 1. The address already stands in a register

`addr_mem` -- the path through which every floating-point and vector access gets its
memory operand -- has **always** copied the address to `rcx`
first:

```rust
self.load_full(e, "rcx", v);
"[rcx]".to_string()
```

If it already lies in a register, it is the operand. Six lines.

**Gemessen: 209,6 -> 198,4 Mio Befehle (-5,4 %).**

## 2. Gleitzahl-Konstanten gehoeren in `.rodata` (`fpool.rs`)

SSE has no form with an immediate constant. Until now Firn built every
floating-point constant at run time from two instructions — and then held it
in an `xmm` register for as long as it was needed. In
`l3_dct3_9` these are **six constants, so six of the twelve registers**
that the allocator has to hand out at all.

Now there is a pool per translation unit: one entry per bit pattern
and width, addressed `rip`-relative, and the constant is the
MEMORY OPERAND of the calculation (`mulss xmm8, dword ptr [rip + .Lfconst3]`).
One instruction, no register.

`l3_dct3_9` thereby falls from 155 to 131 instructions.

### The bug that the probe found

After the resolution of the `phi` nodes FIR is **no longer SSA**: a value
may be written several times. A loop variable that starts at `1.0`
has the `const` in the pre-header and a copy on the
back edge — both to the same value. Whoever puts such a value into the pool
reads the `1.0` forever in the loop.

Measured: `math.powi(2.0, 10)` gave **1.0 instead of 1024.0**, and `math.exp(0.0)`
gave 0. The rule already stands once in the same module (`immediate_consts`,
round 92) and applies here just the same: only what has **exactly one
write site** goes into the pool. Second find of the same probe: the copy of a
floating-point value asked for `place()` instead of `fpo()` — but a constant from
the pool has no slot at all.

## Die Zahlen

| | Instructions (8 s of sound) |
|---|---|
| after TEMPO 5 | 209.6 M |
| + address directly as operand | 198.4 M |
| + constants in `.rodata` | **197.3 M** |
| the same with `--cpu=avx` | **180.2 M** |
| `minimp3` in C, `gcc -O2` | 74.8 M |

PCM bitgleich, Selbsttest PASS 4/4, in beiden CPU-Stufen.

In the decoder the pool brings little (there the constants have long stood in
registers and the pressure was not the bottleneck) — in calculation-heavy code like
`std.math` it is the difference between "six registers gone" and "no
register gone".

## And what remains of the 2.6x?

For `synth`, the hottest function, the count of the generated
instructions says where the work lies: **204 `mov`, 31 `add`, 27 `lea`** against
26 `movaps` and the packed calculations. So address calculation, not calculating.
C gets by with less there, because `gcc` does two things that Firn does not
do yet:

1. **Zeiger weiterschalten statt Adressen neu rechnen** (Induktionsvariablen
   mit Staerkereduktion auf der ADRESSE, nicht nur auf der Multiplikation —
   die allein hat Runde TEMPO 3 gemessen und wieder verworfen).
2. **Lebensdauern an Aufrufen zerschneiden**, damit ein Zeiger, der einen
   Aufruf ueberlebt, nicht bei jedem Zugriff neu aus dem Rahmen geholt wird.

Beides ist Arbeit am Zuteiler, nicht an der Rechnung — und der naechste
grosse Brocken.

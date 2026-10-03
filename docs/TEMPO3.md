# Round TEMPO 3 — one folding, two discarded ideas

State 21.09.2026, branch `xmm-ra`. This round is short, and it consists two thirds of
**discarded** attempts. Both stand here, because the
measurement result is the actual content.

## Was geblieben ist: die Skalierung wandert in die Adressrechnung

A base pointer that SEVERAL accesses use cannot move into the operand of
a single instruction — that is what `foldable_addresses` is for, and
it demands that the address is read exactly once. The SCALING of the
index can nevertheless come along:

```text
vorher:   lea rdx, [0 + rdi*4]        nachher:   lea rdi, [rax + rdi*4]
          lea rdi, [rax + rdx]
```

Conditions: the sum calculates with full width, the scaled part stands
IMMEDIATELY before it and is read only there (then it may be omitted entirely,
without anything changing in the lifetimes), base value and index lie
in registers. Factor 2, 4 or 8 — what the addressing itself can do.

**Gemessen** (MP3-Dekoder, 8 s Ton, `release-fast`, callgrind):
**308,2 Mio -> 304,4 Mio Befehle**, −1,3 %. Ausgabe bitgleich.

## Discarded 1: the multiplication with the loop variable (IVRED)

The classic pass: `i * m` in the loop body becomes a second
loop variable that advances by `m * step` per iteration — the
multiplication disappears. Built, tested (correct in all four
build stages, `imul` really gone), **measured**:

| Program | with IVRED | without | |
|---|---|---|---|
| MP3 decoder | 313.3 M | 304.4 M | **+3 %** |
| `bench/firn/matmul` | 543.6 M | 501.8 M | **+8 %** |
| `bench/firn/bubblesort` | 261.8 M | 234.7 M | **+12 %** |
| `bench/firn/bytecount` | 2031.1 M | 2031.1 M | 0 % |

**Worse, everywhere.** The reason is the machine: x86 has `imul r, r,
imm` as ONE instruction, and what hangs on the multiplication (sign extension,
`lea`) stays anyway. In return the new loop variable costs a register
over the whole loop and an increment per iteration — and registers
are exactly what is missing in these loops (measured: pressure 16 to 20 at
14 available).

The pass has been removed again. The idea would pay off only if the
WHOLE address chain became a pointer that is advanced (that is what C does at
this place) — not the one `imul`.

## Verworfen 2: `synth_pair` einbetten

`synth` calls `synth_pair` twelve times; every call destroys all
caller-saved registers, which is why three base pointers there stay in the
frame. The obvious thing: raise the upper limit of inlining (`MAX_CALLEE_INSTS = 40`)
so far that the function (around 120 instructions) comes along.

Measured: **308.2 -> 307.3 M instructions (−0.3 %)** with a growing program.
The pressure in `synth` stays the same, after all — the pointers still get no
register. The limit stays where it was.

## Addendum to round TEMPO 2: the one AVX failure was none

The first test run with `FIRN_CPU=avx` reported in `tools/self_compare.sh`
`FAULTY: 1` (first deviation `tests/1002_js_interp.fi`). Reproduced: at
that time TWO full test series were running at the same time on the same
machine, and the tool works with `timeout 20` per program. The run
alone, on a quiet machine:

```text
SAME BEHAVIOUR:     338      DIFFERING: 0      FAULTY: 0
```

— number for number the same as in the baseline stage. The cause was the
machine, not the generator.

## State against C

Unchanged compared with TEMPO 2, the 1.3 % of this round lie within the
measurement accuracy of the clock: MP3 decoder 0.25 s (`baseline`) or 0.23 s
(`--cpu=avx`) against 0.07 s for `minimp3` with `gcc -O2` — **3.3x**, and
**2.3x** against the same C without auto-vectorisation.

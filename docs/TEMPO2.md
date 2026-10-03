# Round TEMPO 2 — the handover register, cells in `xmm`, and `--cpu=avx`

State 21.09.2026, branch `xmm-ra`. Everything measured, not estimated. Times:
smallest of nine runs, output to `/dev/null` (writing 10.6 MB of PCM
otherwise covers exactly the difference we are after), quiet
machine, AMD EPYC 7571. Instruction counts: `valgrind --tool=callgrind` on 8 s
of sound, mapping via `nm`.

## Wo Runde TEMPO 1 aufgehoert hat

The MP3 decoder stood at 324 M instructions. The measurement showed two things
clearly:

* `synth` alone was **112 M** of them, and in the generated code there stood per
  calculation step a pair like this:

      movss  [rbp-3912], xmm0     <- write it out
      movss  xmm0, [rbp-3912]     <- and fetch it right back

* 51 `movaps` in the same function: SSE has only the two-operand form,
  `mulss d, s` therefore always calculates INTO the destination, and if the first operand
  lies elsewhere, it has to be copied beforehand.

## 1. The handover register (works, −4.7 %)

Twelve `xmm` are not enough in a densely calculated loop; whoever gets none
lies in the frame. But: if such a value has **exactly one** reader, and
it stands shortly after in the same block, then it does not need the frame at all
— it can stay in the register.

`fp_handover` (in `regalloc.rs`) collects these values per basic block and
distributes them greedily onto the registers that the allocator never hands out: `xmm2`,
`xmm3` and everything from the pool `xmm4`–`xmm15` that the function does not
need at all. Conditions, all necessary:

* the value is a floating-point number, got **no** slot in a register,
  is no cell, no alias, no immediate constant, not `secret`,
* it is read **exactly once**,
* producer AND reader stand in the floating-point path of the output (the same
  condition that fixed the bug from round XMM 3),
* the reader stands in the same block, at most sixteen instructions further on,
  and in between lies **no call** — every `xmm` is caller-saved.

Overlaps do really exist (`t1` is produced, then `t2`, only
afterwards are both read); that is why a small
interval distribution stands behind this and not a single register.

**324 Mio -> 308 Mio Befehle, 0,29 s -> 0,25 s.**

## 2. Floating-point cells in `xmm` (honestly: no effect here)

A cell is an `alloca` that holds a variable. Until now it could
only get an INTEGER register — an `f32` sum then travelled at every
access via `movd` between `r13` and the arithmetic unit. Now
a cell that holds a floating-point number and is touched only in the floating-point path
may get an `xmm`; loading and writing are then a copy or nothing
at all.

**Measured in the decoder: zero effect** — and the reason is to be stated honestly:
`mem2reg` has long since promoted these variables to ordinary values with
`phi`, there is in `lib/ton/mp3.fi` at `release-fast` **not a single**
floating-point cell (`FIRN_FPCELL_DEBUG=1` says so per function). What remains is
the effect where `mem2reg` does not run — `--opt-level=dev` — and in
functions whose `alloca` stays for other reasons. The code
stays in, because it costs nothing and nobody wants the class confusion that
existed before (`movd` through an integer register) anyway.

## 3. `--cpu=avx`: die Dreioperandenform (−11 % Befehle, −8 % Zeit)

`vmulss d, a, b` names its destination itself. With that the copy that SSE
forces is gone. New is the switch `--cpu=<baseline|avx>` (default
`baseline`, so SSE2 as before; `FIRN_CPU=avx` acts the same, so that the
full test series can run in both stages).

Zwei Teile:

1. **The rewrite.** `addss d, s` and `vaddss d, d, s` mean the same;
   `vexify` in `codegen_x86.rs` rewrites every SSE instruction of the path into its
   VEX form. That brings **no** speed by itself — it prevents both forms from standing
   mixed in one function. On Intel every
   switch between old SSE and VEX costs two-digit cycle counts and would eat up the
   gain.
2. **The real three-operand form** in the calculation itself (`emit_bin`,
   sign reversal): first source a register, second may be memory,
   destination free — the copy disappears.

Only the path with register allocation writes VEX. The basic path stays with SSE,
because there `simd.rs` holds `v128` values in the registers and a rewrite from
`movss` to `vmovaps` would wipe out their upper half.

One subtlety is documented: `movss xmm1, xmm2` leaves the upper 96 bits
standing, `vmovaps xmm1, xmm2` does not. On this path that is irrelevant, because
it holds only scalars.

## Die Zahlen

MP3-Dekoder, 60 s Ton (192 kbit/s, Stereo), `release-fast`:

| | Befehle (8 s Ton) | Zeit (60 s Ton) |
|---|---|---|
| Runde XMM 3 (Stand 20.09., mittags) | 644 Mio | 0,88 s |
| Runde TEMPO 1 | 324 Mio | 0,29 s |
| **TEMPO 2, `--cpu=baseline`** | **308 Mio** | **0,25 s** |
| **TEMPO 2, `--cpu=avx`** | **275 Mio** | **0,23 s** |
| `minimp3` in C, `gcc -O2` | 75 Mio | 0,07 s |
| dasselbe C, `-O2 -fno-tree-vectorize` | — | 0,10 s |
| dasselbe C, `-O0` | — | 0,39 s |

**Distance to C:** 3.3x against `gcc -O2` (which vectorises its loops itself),
**2.3x** against the same C without vectorisation. Against `-O0` Firn is
faster.

A pure `f32` calculation kernel (four sums, eight window pairs, `bench/inner_loop.c`
against the same loop in Firn):

| | Zeit |
|---|---|
| Firn `--cpu=baseline` | 0,09 s |
| Firn `--cpu=avx` | 0,08 s |
| C `gcc -O2` | 0,03 s |
| C `gcc -O2 -mavx2` | 0,02 s |

Correctness: the PCM output is bit-identical in **all** stages
(`cmp` over 10.6 MB, in `baseline` as in `avx`), the self-test of the
decoder gives PASS 4/4. `bash test.sh` runs without a new error, in
`baseline` and with `FIRN_CPU=avx`.

## What still stands between Firn and C

Measured, not guessed:

1. **Vectorisation.** 30 % of the lead of `gcc -O2` comes from it
   pouring the loops itself into `packed` instructions (four `f32` per
   instruction). Firn has `v128` as a language feature, but no pass that uses it
   by itself.
2. **Cutting lifetimes at calls.** Whoever survives a call
   gets no register today. In `synth` three
   base pointers therefore stand in the frame and are fetched anew before every access.
3. **Ordering of the instructions.** Firn emits the instructions in FIR order.
   Pulling a load close to its consumer lowers the pressure on
   the registers and thereby the number of values that have to go into the frame at all.

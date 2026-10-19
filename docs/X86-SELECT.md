# x86-64 instruction selection: Firn vs gcc -O2 (round x86sel)

Source of the numbers: `bench/firn_vs_c` (identical Firn/C pairs). Time = user+sys CPU seconds,
median of 7 runs on a loaded host (load 15-25), so +-3 % is noise. Instructions = callgrind `Ir`
of the whole program (deterministic). `release-fast` for Firn, `gcc -O2` for C.

## Causes found (hot loop read next to `gcc -O2 -S -masm=intel`)

| Benchmark | Cause in the Firn loop | Fix |
|---|---|---|
| sha256 | No `ror`: every rotate was `mov; shr; mov; shl; mov; or` (6 instructions, 6 rotates per round). | `rotate.rs::run_u32` + `BinOp::Rotr32` on x86 (`mov r,x; ror r,n`) |
| spectral_norm | `movsd x14,[mem]; mulsd x13,x14` (load + op), `mov rax,rdx; cvtsi2sd x,rax`, signed `/2` copied the dividend to `rcx` and did a useless `sar 63` before `shr 63`. | float load-op folding, direct `cvtsi2sd` from the register, cheaper signed power-of-two division |
| nbody | Same load/op pairs; `sqrtsd` always went through `xmm0` (3 instructions). | load-op folding, `sqrtsd w,w` in the target register |

Not done (measured or judged smaller than the effort): rematerialising `globaladdr` instead of
spilling it (nbody keeps 6 array bases in stack slots; same instruction count, only the
store disappears), `rorx` (needs BMI2, baseline is x86-64), copy coalescing for the
`mov r8,r10; xor r8d,r11d` shape (allocator hints, larger change), `x/2` of a provably
non-negative value (needs a range fact in the emitter; gcc gets it from the signed
overflow rule).

## What was built

1. **32-bit rotate** (`compiler/src/rotate.rs`). `or.u32(shr.u32 x,n ; shl.u32 x,32-n)` with
   constant `1 <= n <= 31` and the SAME source becomes `Rotr32` (type `u32`). The pass now also
   runs for x86-64 (it only ran for aarch64) and then also handles the `u64`-carrying-32-bit form.
   Emitted as `mov dst, src; ror dst32, n` (`regalloc.rs`, base path `codegen_x86.rs`: `ror eax, cl`).
   `ror r32` zero-extends, exactly the FIR semantics of `Rotr32`.
2. **Float load-op folding** (`regalloc.rs::fold_float_load`). A `load.f32/f64` whose only reader
   is the add/sub/mul/div directly behind it becomes the memory operand
   (`mulsd xmm13, qword ptr [r10+rdx*8]`). Adjacent only: nothing can store to the memory or reuse
   an address register in between. `+` and `*` may have the load on either side. `FIRN_NO_LOADOP=1`
   switches it off.
3. Small selection fixes: `cvtsi2sd/ss xmm, r64` straight from the register for `i64`;
   `x / 2^L` (signed) reads the dividend from its own register and drops the `sar` for `L == 1`;
   a register -> frame slot copy stores the register itself (no `mov rax,r; mov [slot],rax`);
   `sqrt` computed in the target register.

## Tests

* `tests/4450_x86_rotr_u32.fi`: nine counts in both operand orders, sigma mix, rotate of a rotate, 64-bit
  widening, look-alikes that must NOT become a rotate (other source, counts that do not add up,
  `u16`), edge values and 4000 random values against n single-bit rotations.
* `tests/4451_x86_float_load_op.fi`: `f64`/`f32` sum of products, `a-s`, `s-a`, `a/s`, `s/a`
  (load as first or second operand), a load read twice, a store between load and use.
* `tests/4437_rotr32_pattern.fi` (r837) now also exercises the x86 rotate (`u64` form).
* unit tests in `rotate.rs` for the `u32` form (counts 1/7/16/31, different source, wrong sum).

## Results (release-fast)

Measured with the first build (rotate + load-op folding only; the small selection fixes came
afterwards and were NOT measured yet - gates had not run when this was written).

| Benchmark | Time base | Time new | gcc -O2 | Instructions base | Instructions new |
|---|---|---|---|---|---|
| sha256 | 0.230 s | 0.181 s (-21 %) | 0.143 s | 2.448 G | 1.844 G (-24.7 %) |
| spectral_norm | 0.228 s | 0.215 s (-5.5 %) | 0.167 s | 2.161 G | 1.981 G (-8.3 %) |
| nbody | 0.669 s | 0.631 s (-5.7 %) | 0.547 s | 4.256 G | 3.836 G (-9.9 %) |

Hot loop sha256 compression round: about 100 -> about 80 instructions (6 rotates x 4 -> 6 x 2).
Hot loop spectral_norm `mul_av`: 26 -> 20 instructions.

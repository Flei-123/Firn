# SIMD and inline assembly (r1564, r58/L16)

## Layers

| Layer | Where | Targets |
|---|---|---|
| `v128` value type + `__v128_*` intrinsics | `compiler/src/simd.rs` (x86-64), `simd_a64.rs` (NEON), `codegen_wasm.rs` (WASM SIMD) | x86-64 (SSE2 base, SSE4.1/SSSE3 for a few), aarch64, wasm32-browser, Windows (same x86 path) |
| `std.simd` | `lib/std/simd.fi` | named wrappers, `hsum_f32`, `dot4`, masks, packs |
| `std.cpu` | `lib/std/cpu.fi` | `has_sse41`, `has_avx2`, ... (aarch64: NEON bits, AVX2 always 0) |
| `std.lanes.*` | `lib/std/lanes/` | typed vectors, methods instead of intrinsic names |
| Auto-vectorizer | `vectorize_emit.rs`, `vec2reg.rs` | SSE2 `vecsum`, `vecforeign` |
| Inline assembly | `core.rs`, SPEC 14.1.asm | x86-64 and aarch64 |

## std.lanes

Import one module per type: `import std.lanes.vf4` and call `vf4.splat(1.0 as f32)`, `vf4.load(p)`, `a.add(b)`.

| Type | Module | Lanes | Notes |
|---|---|---|---|
| `f32x4` | `vf4` | 4 x f32 | add sub mul div min max sqrt abs neg clamp mul_add, lt le gt ge eq ne, shuffles, hsum hmin hmax dot |
| `i32x4` | `vi4` | 4 x i32 | add sub mul and or xor not neg abs min max, shifts by 1/8/16, comparisons, hsum hmin hmax |
| `u8x16` | `vu16` | 16 x u8 | add sub add_sat sub_sat min max abs_diff, bit ops, comparisons (`mask16`), select, interleave, hsum |
| `f32x8` | `vf8` | 8 x f32 | pair of 128-bit halves; same ops as `f32x4` |
| `i32x8` | `vi8` | 8 x i32 | pair of 128-bit halves |
| `mask4` / `mask8` | `vmask` | | `bits()` `any()` `all()`; `vf4.select(mask, a, b)` |
| conversions | `vconv` | | f32 <-> i32 (truncating), bit casts |

- No operator overloading in Firn: methods carry the operation (`a.add(b)`).
- Float ops are IEEE per lane. `min`/`max` on NaN lanes are unspecified (as in `std.simd`).
- `i32x4.mul` is SSE4.1 `pmulld` (NEON `mul`, WASM `i32x4.mul`); every x86-64 CPU since 2008 has it.
- `i32` shifts are logical, fixed amounts (the shift count is an instruction immediate).
- `f32x8` / `i32x8` are two `v128`; there is no 256-bit register path yet (AVX2 open, `std.cpu.has_avx2()`).
- Fallback: Windows uses the x86 path, WASM uses WASM SIMD (all kinds used by `std.lanes` translated, checked octet-equal against wat2wasm). There is no scalar-only target left.

## Tests

- `tests/6792_simd_lanes.fi`: xorshift random inputs against scalar reference, every method of all five types. Run on x86-64 (dev-fast, release-safe, release-fast, no-opt), aarch64 (qemu), Windows (wine), WASM (node).
- `tools/asm/run.sh`: inline asm operand tests on x86-64 and aarch64 (qemu), refusal of locked registers, `--unsafe=error` behaviour.
- `tools/safety/unsafe/run.sh`: `asm` needs `unsafe` (firnc0 and firnc1, same text).
- `bench/simd/run.sh`: callgrind instruction counts against C `-O2` (scalar and SSE2 intrinsics).

## Inline assembly

```firn
unsafe {
    sum = asm("add rax, rcx", out("rax"), in("rax") a, in("rcx") b)
    asm("mul rcx", out("rax") & lo, out("rdx") & hi, in("rax") x, in("rcx") y)
    asm("str x0, [x1]", in("x0") v, in("x1") p, clobber("memory"))
}
```

- Operands: `in("reg") expr`, `out("reg")` (value), `out("reg") p` (store into `*p`), `clobber("reg" | "memory")`.
- x86-64: rax rcx rdx rsi rdi r8..r11 (narrow names too). aarch64: x0..x17 / w0..w17.
- `asm` is an unsafe operation under `--unsafe=warn|error`.
- Tests: `tests/850_asm_basic.fi`, `1242_asm_multi_out.fi`, `6793_asm_operands_x86.fi`, `tests/asm_a64/`.

## Measurements (callgrind, release-fast, 16384 f32 x 400 repetitions)

| Program | Firn | C -O2 scalar | C -O2 SSE2 intrinsics |
|---|---|---|---|
| dot, raw `v128` | 9.06 M | 25.03 M | 8.65 M (1.05x) |
| dot, `std.lanes` f32x8 | 9.06 M | 25.03 M | 8.65 M (1.05x) |
| blur [1 2 1]/4, raw `v128` | 26.64 M | 18.52 M (gcc auto-vectorizes) | 18.52 M (1.44x) |
| blur, `std.lanes` f32x4 | 26.64 M | 18.52 M | 18.52 M (1.44x) |

Scalar replacement closes the typed-layer gap: `sroa.rs` splits struct copies between stack cells (32-byte `f32x8`/`i32x8` copies) into per-field loads and stores, and `vec2reg.rs` / `vec2reg_single.rs` turn a sixteen octet cell that is accessed as one `v128` and as two eight octet words (the ABI passes a 16-byte struct as two words) into SSA values. Before: dot 22.18 M (2.56x), blur 170.8 M (9.2x). Still open: 256-bit AVX2 path (r1604).

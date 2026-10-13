# GRAPHICS STACK, WAY E -- what Firn has, what is missing (13.10.2026)

OrientOS plans "way E": its own GL ES 3 / Vulkan subset as a **shared library written in Firn**, a **multi-core / SIMD
CPU rasterizer**, and a **shader core as SPIR-V interpreter / JIT**. This file is the inventory (evidence, not
promises) and the plan. Gate 134 = `tools/stack_e/run.sh`. Everything below was run on 13.10.2026 on x86-64 Linux
(busy host, load 16-24) unless it says otherwise.

## 1. What works today (measured)

| need | state | evidence |
|---|---|---|
| Firn code as a C-ABI `.so` | **works.** `firnc --pic -c x.fi` + `ld -shared -Bsymbolic --version-script` (no TEXTREL). Exports = `#[export_c]` (bare symbol name). | `tools/stack_e/run.sh`: `dlopen` from C (gcc) and `ctypes` from Python, 12 checks each, 3 opt levels (dev-fast, release-safe, release-fast), 0 failures. Older proof: `tools/modreload` (hot reload of a `.so`) |
| scalars int / f32 / f64 | works (System V register classes) | `se_add_i32`, `se_axpy_f32`, `se_len_f64` |
| struct **by pointer** (read + write) | works | `se_vtx_dot`, `se_vtx_scale` (struct of 4 f32) |
| small struct **by value** (<= 16 octets) | works (two f32 in xmm0) | `se_v2_add` |
| **callbacks** C -> Firn (exports) and Firn -> C (call a C function pointer) | works, with a trick: `(*h).f(i)` is rejected ("pointer calls are not supported in stage 0"); bind `let f: fn(i64) -> i64 = ...` and wrap the C address into a record (a Firn `fn` value is a pointer to a record whose first word is the code address) | `se_call_cb`: C function pointer and a Python `CFUNCTYPE` both called 10x, sum correct |
| module state in a global (`static mut`) | works (PIC, RIP-relative) | `se_count_inc` 1, 2 |
| v128 SIMD in a PIC object | works | `se_fma4_f32` vs C and numpy, bit-identical; `mc_blend.mix_span/blend_span` inside the `.so` vs C and numpy, every weight / alpha |
| runtime pieces without threads | `rt.heap_alloc` (mmap) and `jobs.parallel_for` with **0 workers** work in a `.so` | `lib_rt.fi`, gate 134 |
| fast float paths | release-fast is fast again (MC-FLOATS, RELEASE-FAST-REGRESSION): Raster bench 640x360 fast 21 ms / safe 26 / dev-fast 55 | KNOWLEDGE.md compiler section |
| SIMD (v128) | SSE2 / SSE4.1 / SSSE3 / AES / SHA via `__v128_*` intrinsics, NEON twin for aarch64, `__cpu_features()` | docs/ROUND82.md, MC-FLOATS.md section 2 |

## 2. What was added in this round (small compiler change)

Eight new `v128` binary intrinsics (compiler: `simd.rs`, `simd_a64.rs`, `regalloc.rs` fast path, `asm_x86.rs` for `--asm-intern`):

| intrinsic | x86-64 | aarch64 |
|---|---|---|
| `__v128_minf32` / `__v128_maxf32` | `minps` / `maxps` | `fmin` / `fmax .4s` |
| `__v128_divf32` | `divps` | `fdiv .4s` |
| `__v128_minu8` / `__v128_maxu8` | `pminub` / `pmaxub` | `umin` / `umax .16b` |
| `__v128_addsat_u8` / `__v128_subsat_u8` | `paddusb` / `psubusb` | `uqadd` / `uqsub .16b` |
| `__v128_mullo32` | `pmulld` (SSE4.1) | `mul .4s` |

* NaN lanes of `minf32` / `maxf32` are **unspecified** (x86 returns the second operand, NEON a NaN). Test data has no NaN.
* **Proof:** `tests/2860_simd_minmax.fi`, every lane against a scalar reference, 200 rounds of random data, dev-fast /
  release-safe / release-fast / `--asm-intern`, **and** `--target=aarch64-linux` under `qemu-aarch64` (NEON path), all exit 0.
  Encodings of the nine new mnemonics are in the `pixel_blend_operations_match_as` unit test (octets from `as`).
  Existing tests 1613-1617, 2753, 2756, 2763 unchanged (exit 0).
* wasm32 refuses them by name (`'main' uses the SIMD instruction 'MaxF32'`), like the 7 pixel intrinsics before.
* **Speed** (`tools/stack_e/simd_bench.fi`, 1 MiB per pass, release-fast, busy host, noisy; two runs each):
  clamp of 262144 f32 (`min(max(x, lo), hi)`): scalar 9.5-11.1 ms, v128 0.5-0.8 ms (**~12-20x**);
  saturating add of 1 MiB of bytes: scalar 12-13 ms, v128 0.4-0.6 ms (**~20-30x**). The scalar loop is branchy on random data and
  Firn has no scalar `minss`/`maxss` intrinsic, so the factor is a ceiling for such loops, not for every kernel.

## 3. What is missing (errors with evidence)

| id | problem | evidence | size |
|---|---|---|---|
| **G1** | **FIXED (13.10.2026, commit see log).** Threads/GC inside a `.so` used to corrupt the host: the runtime set `fs` (`arch_prctl(ARCH_SET_FS)`) and read `fs:0`, i.e. glibc's TCB. Now `--pic` x86-64 Linux keeps the thread block behind `gs` (`thread::use_gs`, `lower.rs::retarget_thread_base`); executables unchanged. Proof: `se_jobs_probe` with 3 workers == 0 workers from C and ctypes, `se_gc_probe` (GC chains on 4 threads + forced collections), C host with 4 pthreads running malloc/qsort/errno/TLS/stack protector in parallel, 3 opt levels. | `tools/stack_e/{run.sh,lib_rt.fi,host_rt.c}`, `docs/SHARED-OBJECTS.md` | done |
| G1b | Only the host thread that first used the runtime has a `gs` base; a second host thread calling into the library crashes (`gs = 0`). | python `threading` probe | small: `thread_attach()` export that registers a foreign thread (table entry + own stack bounds) |
| **G2** | **FIXED.** `firnc --shared [--header=f.h] -o lib.so x.fi`: one command, version script from `#[export_c]`, no `fn main` needed, header generated. | `run.sh` | done |
| **G3** | **FIXED (basic).** `std.cfn` (`rec`, `as_u0..u3`, `as_v1/v2`): C code address -> `fn` value for word-sized signatures. Float callbacks and struct-by-value callbacks are not covered. | `lib_e.fi se_call_cb` | done |
| G4 | `#[no_gc]` is transitive: an exported `#[no_gc]` function cannot call library functions that lack the attribute (`mc_blend` is not annotated); the workaround was to leave the export without `#[no_gc]`, which is only safe while nothing allocates | compile error "SPEC 3.5.4" | small: annotate leaf libraries |
| G5 | Windows (`.dll`) and aarch64 shared objects: `--pic` exists for Android aarch64/x86_64 (APK build); no PE DLL export table, no CI for a Linux aarch64 `.so` | docs/MOD-RELOAD.md limits | medium |
| G6 | SIMD still thin for a rasterizer: only 128 bit (no AVX2), no `sqrt`/`rcp`/`rsqrt` f32, no float shuffle (`shufps`), no compare-equal for 8/16 bit, no `movemask`, no `mulhi16`, no `avg`, no min/max for i16/i32, no gather, no `f32x4`/`u8x16` value types or `std.simd` wrapper (everything is `v128` + intrinsic names). wasm32 has none of the 15 pixel/min-max intrinsics | `compiler/src/simd.rs` table | medium, incremental |
| G7 | Shader core: `lib/gpu/spv` holds SPIR-V **binaries** (`tri.vert.spv`) and `vk.fi` consumes them; there is **no SPIR-V reader, interpreter or JIT** in Firn. A JIT needs executable memory (`mmap` PROT_EXEC) + an x86 emitter for v128 code from Firn code (the integrated assembler `asm_x86.rs` is Rust inside the compiler, not a library) | `ls lib/gpu/spv`, grep | large (own project) |
| G8 | Multi-core raster: `lib/jobs` is proven (6.6x at 8 threads) in executables, but not in a `.so` (G1) and not with the GL-ES-state objects of way E | KNOWLEDGE.md | depends on G1 |
| G9 | `release-fast` triangle loop is scalar `f64` per pixel; "SIMD edge functions, 4 pixels at once" is planned in MC-FLOATS section 3 but not started | docs/MC-FLOATS.md | medium |

## 4. Plan (order, each step has its own proof)

1. **G1 first** (blocks multi-core in a `.so`): shared mode TLS via `gs`; proof: `tools/stack_e/lib_rt.fi` `se_jobs_probe` with 3 workers equals the 0-worker result from C and ctypes; gate 134 prints "GAP CLOSED".
2. G2 + G3 (`firnc --shared`, header output, `std.cfn`): proof: the existing `lib_e.fi` builds without a manual `ld` line.
3. G6 in the order the rasterizer needs it: `sqrt/rcp/rsqrt f32`, `shufps`, `cmpeq 8/16`, `movemask`, `mulhi16`, min/max i16/i32; then a `std.simd` wrapper with `f32x4` / `i32x4` / `u8x16`. Every addition: scalar cross-check test like 2860 + NEON twin under qemu.
4. G9: SIMD edge functions in `mc_raster` (first real consumer; compare against current hash/PSNR of Raster/Sky).
5. G7 decision with OrientOS: **interpreter first** (portable, testable against the existing GLSL->SPIR-V shaders), JIT later.
6. G5 when OrientOS needs the library on Windows/aarch64 hosts.

## 5. Decisions for OrientOS (short)

* Way E is feasible for the **API surface** now (C ABI `.so`, SIMD kernels); it is **not** feasible for **threads or GC allocation inside the `.so`** until G1 is fixed. Until then: single-threaded `.so`, `#[no_gc]` exports, memory from the host or `rt.heap_alloc`.
* Hand buffers to the library as pointers + lengths; keep state in host-owned structs (as `tools/modreload`).
* Do not plan around AVX2 or wasm SIMD yet.

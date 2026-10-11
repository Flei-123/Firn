# MC-FLOATS -- float register allocation for the renderer loops (B3) and SIMD pixel blending (B19)

State: 11.10.2026. All numbers measured on this host (load 5-7 from other jobs while measuring, so wall times are
noisy; instruction counts come from callgrind and are exact). Tool: `tools/mc/b3_bench.sh`
(`FIRNC_FLAGS=--opt-level=release-fast bash tools/mc/b3_bench.sh label=<firnc>[:ENV=1] ...`).

## 1. B3: does `xmm-ra` (float register allocation, `regalloc.rs`, TEMPO 1-15) help the renderer?

`xmm-ra` is on main and active (`FIRN_NO_FP_RA=1` switches only it off, same binary). Workloads:
`tools/mc/raster/bench.fi` (640x360, 5x5 sections, per frame) and `tools/mc/sky/bench.fi` (sunset + noon + night).
Instructions per frame = (run with 6 frames - run with 2 frames) / 4, so set-up cancels.

| build | metric | no xmm-ra | xmm-ra | change |
|---|---|---|---|---|
| default (release-safe, what `tools/mc/*/run.sh` builds) | raster Ir / frame | 417.30 M | 415.96 M | -0.3 % |
| | sky Ir / iteration (3 scenes) | 82.71 M | 79.84 M | -3.5 % |
| | sky sunset, median wall | 10.57 ms | 9.61 ms | about -9 % (noise +-10 %) |
| release-fast | raster Ir / frame | 398.08 M | 396.66 M | -0.4 % |
| | sky Ir / iteration | 89.70 M | 89.67 M | 0 % |

The same numbers on the compiler with the Certus patch set merged (E3-E5, section 6 of CERTUS-COMPILER-MERGE.md)
and on plain main are equal to within 0.4 % (raster 415.96 M both, sky 79.84 M both; release-fast 396.66 M against 397.52 M).

**Verdict: B3 is on main and works, but it is worth only 0-4 % on the renderer loops.** Why, measured in the emitted
assembly of `draw_clipped_tri` (the pixel loop):

* default (release-safe): 19 `call mc_raster.ldd`, 4 `call math.floor`, 6 `call math.fabs`, `st32`, `ld32`, `stf` stay REAL
  calls. The helpers contain overflow-check sites (`jo .Lchksite`), which keeps the inliner away. Floats cannot live in `xmm`
  registers across a call (all `xmm` are caller-saved), so the allocator has nothing to keep: 196 -> 174 float memory
  operands only.
* release-fast: all those calls are inlined (0 calls left), the function grows 2272 -> 2799 lines, and the instruction count
  drops 5 % -- **but the wall time gets WORSE, by about 60 %** (interleaved runs on the same host, same compiler, six runs
  each: raster 60-68 ms release-safe against 98-109 ms release-fast; sky sunset 8.2-10.1 ms against 14.4-17.5 ms). Plain main
  shows the same (raster 95-99 ms release-fast). Fewer instructions, more time: this is a latency / dependency problem of
  the generated code, not a work problem. Not diagnosed (no `perf` in this VM). It is the biggest open item for the
  renderer: **the production build of the client should use release-safe until it is understood** (roadmap).

Practical consequence for the client: the speed is in the algorithm (per-pixel `f64` divisions: 9 `divsd` in the pixel
loop, perspective-correct `u`, `v`, `ao` each divided by `sum`), not in the register allocation. The next step is
reciprocal-multiply (one `divsd` for `1/sum`) and integer/fixed-point edge stepping, as `sky_draw` already does with its
column tables (`docs/mc/sky.md`).

## 2. B19: SIMD pixel blending

What existed: the `v128` value type with SSE2/SSE4/AES/SHA intrinsics for x86-64, NEON twins for aarch64
(`simd_a64.rs`), WASM translation (`codegen_wasm.rs`) -- but only 32/64-bit lanes and f32: no way to multiply bytes.

Added (compiler, round MC-B19), seven intrinsics, 16-bit lanes + byte interleave:

| intrinsic | x86-64 | aarch64 | immediate |
|---|---|---|---|
| `__v128_unpacklo8(a, b)` / `unpackhi8` | `punpcklbw` / `punpckhbw` | `zip1` / `zip2 .16b` | |
| `__v128_mullo16(a, b)` | `pmullw` | `mul .8h` | |
| `__v128_add16(a, b)` / `sub16` | `paddw` / `psubw` | `add` / `sub .8h` | |
| `__v128_srl16(a)` | `psrlw` | `ushr .8h` | 0..15 |
| `__v128_packus16(a, b)` | `packuswb` | `sqxtun` + `sqxtun2` | |

The binary ones also run in the xmm register allocator path (`regalloc.rs`, no slot round trip); the integrated assembler
(`--asm-intern`, `asm_x86.rs`) knows the three mnemonics it lacked (`pmullw`, `punpckhbw`, `packuswb`; octets checked
against `as`, test `pixel_blend_operations_match_as`). **WASM refuses the seven kinds with their name** (`wasm_simd_kind`):
needs `i8x16.shuffle` for the interleave, `i16x8.mul/add/sub/shr_u` and `i8x16.narrow_i16x8_u` in `wasm_enc.rs`.
Use `mix_span_scalar` / `blend_span_scalar` in a browser build until then.

Library `lib/mc/mc_blend.fi` (four RGBA pixels per register, 10-12 instructions per four pixels):

* `mix_span(dst, n, rgb, f)` -- `(c*(256-f) + k*f) >> 8`, the fog / shade formula of `mc_raster.fi` (fog = mix with the fog
  colour, shade = mix with black).
* `blend_span(dst, n, rgb, a)` -- `(k*a + c*(255-a)) / 255` with truncating division, the formula of `mc_hud.fi` (exact
  `/255` on u16 lanes as `(t + 1 + (t >> 8)) >> 8`, valid for `t <= 65025`).
* both with a scalar twin (`*_scalar`, `*_pixel`) that serves as reference and as tail loop. Alpha stays 255.

Proof: `tests/2756_mc_blend.fi` -- every weight 0..256 (step 3) / every alpha 0..255 x tail lengths, random pixels, SIMD
bit-identical to scalar; in 5 stages (`tools/mc/blend/run.sh`: release-fast, `--no-opt`, dev-fast, release-safe,
`--asm-intern`). The same test, cross-compiled `--target=aarch64-linux`, passes under `qemu-aarch64` (NEON path).

Speed, `tools/mc/blend/bench.fi`, one 640x360 frame (230 400 pixels), release-safe default build, busy host:

| operation | scalar | SIMD | factor |
|---|---|---|---|
| `mix_span` (fog / veil) | 3065-3307 us | 695-817 us | 4.0-4.4x |
| `blend_span` (HUD /255) | 3470 us | 705 us | 4.9x |

Where it pays in the client: a frame-wide veil / underwater tint / fog pass / damage flash / the HUD background rectangles
(`mc_hud.fi: hud_rect` can call `blend_span` per row). NOT integrated into the triangle loop: there the blend is
per pixel inside a scalar `f64` loop, and four neighbouring pixels do not share a fog weight. A SIMD triangle loop
(edge functions for 4 pixels at once, `__v128_*f32`) is the next stage and needs perspective-correct interpolation in f32;
not started.

## 3. Next

1. Find out why release-fast is 60 % slower than release-safe on these loops (instruction count is lower).
2. Reciprocal-multiply in the pixel loop; then SIMD edge functions.
3. WASM encodings for the seven kinds (browser client).
4. `hud_rect` -> `blend_span` (owner of `mc_hud.fi` is another worker; the formula is the same, test against the HUD
   cross-check `tools/mc/hud/run.sh`).

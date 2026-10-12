# RELEASE-FAST-REGRESSION -- why the renderer is 1.6-2.2x SLOWER at release-fast

State: 12.10.2026 (worker firn-gpu-d). Host: busy (load 10-23), AMD EPYC 7571 (Zen 1), LXC, no `perf`
(perf_event_paranoid=4). Numbers = min of interleaved runs; instruction counts from callgrind are exact.

## 1. Short answer

* **Not "fast vs safe". It is the INLINER.** `--opt-level=release-fast --no-pass=inline` runs the raster bench in
  **54 ms/frame**, plain `release-fast` in **121 ms/frame** (same binary of firnc, same source, bit-identical image).
* **Root cause (measured, not guessed):** `compiler/src/regalloc.rs::unsupported_basic` rejects EVERY function that
  contains one `Op::Un(Sqrt | Bits)` ("`__sqrt / __bits`") -- the whole function then goes to the **base path**
  (every FIR value in a frame slot of its own, no `rbx/r12-r15/r11`, a 12 KB frame). `math.floor`, `math.trunc`,
  `math.fabs` (all via `__bits` / `__f64_from_bits`) and `math.sqrt` (`__sqrt`) contain exactly that. As long as
  they stay CALLS (dev-fast, `--no-pass=inline`) only the tiny callee is on the base path. The moment the inliner
  (release-safe AND release-fast) pulls one into a hot function, the hot function loses its register allocation.
* Evidence: `FIRN_RA_WARN=1 firnc --opt-level=release-fast ...` prints
  `RA-BASE mc_raster__draw_clipped_tri reason=__sqrt / __bits insts=591` (release-safe: `insts=589`; dev-fast: no line).
  `draw_clipped_tri` frame/regs: release-fast 12976 B, 0 uses of rbx/r12-r15/r11, 2950 asm lines; `--no-pass=inline`
  544 B, all six registers used, 1443 lines; dev-fast 592 B.

## 2. Two mix-ups in the earlier analysis (docs/MC-FLOATS.md section 1)

* The CLI default is **dev-fast** (`opt.rs: OptConfig::default`, "DESIGN_GOALS.md line 554"), not release-safe. The
  table row "default (release-safe, what tools/mc/*/run.sh builds)" is dev-fast: no inliner at all. So "19 calls
  stay real calls because of `jo` check sites" was never the reason for the speed -- the real comparison was
  *no inliner* against *inliner*.
* "fewer instructions, more time" is the base path: callgrind counts 5 % fewer instructions (calls gone) but each
  one is a slot load/store; store->load forwarding chains of 4-6 cycles each sit on the critical path.

## 3. Measurements

Raster bench (`tools/mc/raster/bench.fi`, phase 1 only, 640x360, ms/frame, min of 5 interleaved runs, host load 11-15):

| build | ms/frame | note |
|---|---|---|
| release-safe | 128 | inliner on, hot function on base path |
| release-fast | 121 | inliner on, hot function on base path |
| release-fast `--no-pass=inline` | **54** | RA path |
| release-fast without one of: promote / licm / cse / bce / ivsr / unroll / strength / copyprop / sroa | 99-121 | no effect (all still inline) |
| dev-fast | 77 | no inliner (second run, load 12-15: dev-fast 77, release-safe 154, release-fast 136, no-inline 65) |

`--no-pass=mem2reg` is 396 ms (expected). Only the inliner matters.

Mini repro (`tools/perf/rf_repro.sh`, 80 lines of Firn, `shade()` = integer edge values + float divides + 2x floor + fabs):
frame of `shade`: dev-fast 208 B, release-safe 2416 B, release-fast 2416 B, release-fast `--no-pass=inline` 208 B;
min user time of 5 interleaved runs: dev-fast 1.46 s, release-safe 2.29 s, release-fast 1.87 s, no-inline 1.42 s.
Checksum identical in all four.

## 4. Fix (compiler, small): ROUND RFAST in `regalloc.rs`

Three hunks, ~50 lines:
1. `unsupported_basic`: drop the blanket `Un(Sqrt|Bits)` rejection and let `Sqrt` / `Bits` pass the "unary operation
   with float" check (only `Neg` passed before).
2. `fp_taugt`: `Un(Sqrt | Bits)` count as float-path definitions and uses (so the operands can live in `xmm`).
3. `emit_inst`: `Un(Sqrt)` = `fp_into xmm0; sqrtsd/sqrtss xmm0, xmm0; fp_out` (exactly the base path's sequence);
   `Un(Bits)` = float->int `fp_into xmm0; movq rax, xmm0` (f32: `movd eax, xmm0`, clears the upper half),
   int->float `load_full rax; movq xmm0, rax` (f32: `movd xmm0, eax`); no memory round trip.

STATUS: written on branch `staff/firn-mv4ux6ww` (commit "regalloc: sqrt/bits in the register path"), **NOT yet built or
tested** -- the heavy queue was 7 deep. Needed before merge: `cargo build --release` (heavy), the raster/sky benches
(bit-identical hashes: tests 2753, 2763, `tools/mc/raster/run.sh`, `tools/mc/sky/run.sh`), the float tests (`tests/` with
`f64`/`f32`/`__bits`/`__sqrt`), `tools/fir_compare.sh` and the parser/sema compare tools, then the full suite.
The peephole over the emitted text treats `movq`/`movd`/`sqrtsd` as unknown mnemonics (catch-all clears its state), so
it stays correct.

## 5. Workarounds until the fix is merged

* Build the client with `--opt-level=release-fast --no-pass=inline` (54 ms) or plain dev-fast (the default).
* Lib side (not done, not needed once RFAST lands): keep `floor`/`fabs` calls out of hot loops (hoist, or table).

## 6. Who is affected besides the renderer

Everything numeric built at release-safe / release-fast that calls `math.floor/trunc/fabs/sqrt/round/ceil` or
`__bits` (NaN-boxing in the JS engine, hashing of numbers, `frexp/ldexp`) from a function with real loops. Check any
function with `FIRN_RA_WARN=1 firnc ... 2>&1 | grep RA-BASE`.

## RESULT (2026-10-12, merged 28eb2bb3d)
Fix `compiler/src/regalloc.rs` (Sqrt/Bits stay in the register path) built and verified:
* `tools/perf/rf_repro.sh`: no RA-BASE line any more; frame `shade` 176 B (was 2416 B); min user time dev-fast 1.16 s, release-safe 0.51 s, release-fast 0.43 s (before: release-fast 1.87 s vs dev-fast 1.46 s); checksum identical in all 4 stages.
* `tools/mc/raster/run.sh` and `tools/mc/sky/run.sh`: all scenes bit-identical (hashes), counter-checks fire.
* 102 float-using tests (tests/*.fi, tests/opt) x release-fast / release-safe / dev-fast: 306 runs, 0 failures.
* NOT run: full `test.sh`, cargo test, fir/sema compare (front end untouched). Machine was busy (load 8-17); times are min of 5.

## RESULT 2 (2026-10-12, worker gpu-i)
* Rebuilt `firnc` from main (release): `tools/perf/rf_repro.sh` = no RA-BASE for `shade`, frame 176 B (dev-fast 208 B), min user
  time dev-fast 1.15 s, release-safe 0.47 s, release-fast 0.41 s, fast-noinline 1.07 s, checksum 602946897 in all 4 stages.
  `FIRN_RA_WARN=1` on the raster bench at release-fast: the only RA-BASE functions left are the `accel.*` AES/SHA-NI ones
  (vector instructions outside the narrow set), none in mc_*/math_*. Raster bench (640x360) release-fast: 21-25 ms/frame.
* Signed `/ 2^L` and `% 2^L` (L <= 30) in the register path: `emit_div_const` now emits shift + sign correction
  (`mov; sar; shr; add; sar`, remainder via `and -2^L`) instead of the magic-number `imul`. Result: correct (test 2798, identity
  q*k+r==x at four build levels), but NO measurable speed change on the raster bench (min of 7 interleaved: 21 ms old and new).
  The remaining `idiv` in the raster bench are divisions by VARIABLES (`atlas_texel` / texture size modulo), not by 256.
* The pixel loop's `divsd` (zinv = sum / area2f, u/v/ao = (...) / sum) stay: replacing three `/ sum` by one `1/sum` and multiplies changes the
  rounding (floor of u*tsz can flip), so the images would no longer match `ref.py` bit for bit. The compiler may not do it (IEEE); it is a
  renderer algorithm change that needs an explicit tolerance decision (see MC-FLOATS section 3).

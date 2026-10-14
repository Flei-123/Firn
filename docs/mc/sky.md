# mc_sky -- sky gradient, time of day, sun / moon, fog and light tint

`lib/mc/mc_sky.fi`, test `tests/2754_mc_sky.fi`, cross-check `bash tools/mc/sky/run.sh` (`RESULT mc-sky: ok`).
Uses `mc_raster` (colour buffer, camera basis, fog) only. No window, no libm: the result is a pure function of
`(ticks, rain)` + camera and therefore byte identical on every build stage and in `tools/mc/sky/ref.py`.

## Model (the contract the python reference repeats)
* Ticks wrap to 0..23999 (game convention: 0 sunrise, 6000 noon, 12000 sunset, 18000 midnight). Angle = `t*360/24000`.
* Sine = Bhaskara's rational approximation `4p/(40500-p)`, `p = a(180-a)` (max error 0.0016; exact at 0/90/180/270).
* Sun direction = `(cos, 0.9 sin, 0.43589 sin)` (rises east +x, noon high in the south +z); the moon is exactly opposite.
* `daylight` (0..256) = `floor(256 * clamp((sun_y + 0.1) / 0.3))`; `glow` (0..256) = sunset/sunrise strength
  `floor(256 a^2)`, `a = max(0, 1 - |sun_y| / 0.35)`, scaled by `(256 - rain) / 256`.
* Colours = integer lerps `(a*(256-t) + b*t) >> 8` between night (zenith 3,4,14 / horizon 10,12,30) and day
  (82,131,235 / 190,214,255); rain (0..256) pulls both towards a grey that follows the daylight. **Fog colour = horizon colour.**
* Light tint (multipliers 0..256 per channel): night 96,110,190 -> day 256,256,256, warm at sunset, slightly grey in rain.
* Rain >= 128 hides sun and moon.

## Background pass `sky_draw(sky, raster)`
Needs `raster_set_camera` of the frame. Per pixel: view ray (same pixel-centre / focal rule as the rasterizer), normalised;
elevation `g = clamp(1.6 * ray_y)`, `gg = 1 - (1-g)^2`, colour = lerp(horizon, zenith, floor(256 gg)); sunset glow
`floor(dot^3 * (1-gg) * glow)` towards 255,120,60 (only on the sun's side, `dot > 0`); sun disc `dot >= 0.9976`
(255,248,200), moon disc `-dot >= 0.9984` (226,228,240). Depth buffer untouched.

Order per frame: `raster_clear` -> `sky_draw` -> `sky_set_raster_fog(sky, r, start, end)` -> world (`wr_render`) ->
optional `sky_tint_frame` (multiplies every pixel that has a depth entry by the tint).

## API
| Function | Meaning |
|---|---|
| `sky_new()`, `sky_set_time(&sky, ticks, rain)` | state (`daylight`, `glow`, `sunx/y/z`, `zr..`, `hr..`, `fr..`, `tr..`, `discs`) |
| `sky_draw(&sky, &raster)` | background |
| `sky_set_raster_fog(&sky, &raster, start, end)` | fog colour hand-over |
| `sky_tint_frame(&sky, &raster)` | light tint of the world pixels |
| `sky_sin_deg / sky_cos_deg` | the libm-free sine (degrees, x in [0, 360)) |

## Checked (tools/mc/sky)
12 scenes (noon looking at the sun, sunrise east, sunset west, glow, midnight moon, light + heavy rain, looking down,
negative / large ticks, two tinted frames) rendered by the python reference per pixel with exact integers and the same
f64 operation order: state fields, f64 bits of the sun direction, framebuffer and SHA-256 equal; invariants (noon full day,
midnight dark, glow only at the horizon, tick wrap, sine error); 3 build stages byte-equal; counter-check (damaged sun
path) detected. Not independently checked: the look (colours are choices, not Mojang values).

## Fast pass (`sky_draw`) vs definition (`sky_draw_plain`)
`sky_draw_plain` is the per-pixel model above (one normalisation per pixel) and the definition of the image. `sky_draw` gives
exactly the same pixels, only evaluated lazily (tests/2761 compares both on 120 random scenes: any time, rain, yaw / pitch,
lens 20..170 degrees, odd sizes; a damaged threshold or box is detected):
* column part of the ray precomputed once per frame (same expressions), gradient colour = 257 entry table indexed by `f`;
* per row `f` is evaluated exactly only at a few columns and bisected: along a row `ny = (Py + ex ry) / |P + ex R|` has ONE
  extremum (closed form `ex* = (Py (P.R) - ry (P.P)) / ((P.R) ry - Py (R.R))`, the `ex^2` terms cancel), so left and right of
  it `f` is monotone and equal end values mean equal inside (5 columns around the extremum are exact; lenses wider than
  ~152 degrees evaluate every pixel);
* glow: floor(d3 (1-gg) glow) >= 1 needs `dsun^3 >= 1/(glow (1-gg))`; a conservative test on squares (no sqrt, per-`f` table,
  safety 0.9) lets only those pixels through; there `dsun`/`ny` come from one sqrt + one division and are re-done with the
  plain formulas when the value is within 1e-7 of an integer (rounding error ~1e-12);
* discs: plain-model pixels only inside a conservative screen box of each disc (interval arithmetic on the camera basis,
  margin 0.12 rad for the 0.07 rad disc; empty box when the disc is behind the camera).

## Measured (server, release, Load ~1-10)
`sky_draw` 640x360 (`tools/mc/sky/bench.fi`): noon / night ~2.4 ms/frame, sunset with glow ~8.6 ms (before r54: 4.5 / 13 ms;
the plain model costs ~28 ms). 1120 scenes (320x180, all yaw/pitch/tick combinations) are byte identical to `sky_draw_plain`.

r54 (target < 5 ms at sunset) was NOT reached; what was learned (AMD EPYC 7571, Firn stage 0 codegen):
* The glow region covers ~50 % of the frame at full glow (115k of 230k pixels at tick 11800): the value `floor(d3 (1-gg) glow)`
  changes about every pixel (0..190 over ~200 px), so no coarse evaluation / interpolation can be exact; a rigorous second
  derivative bound for bilinear interpolation gives an error > 1 level even for 2x2 cells, so the guard band would catch everything.
* Per glow pixel one sqrt + one division are needed (the sqrt via `math.sqrt` is a call: a call + sqrt + div costs ~14 ns vs 7 ns
  with the intrinsic in isolation; inside `sky_draw` replacing it by `__sqrt` was consistently 40 % SLOWER, left as is).
* The big win was code shape, not math: indexed helpers (`rt.ld32/st32`, `x * 8` with overflow checks, `tab[f]` bounds checks)
  were ~70 % of a glow pixel. Pointer-walking loops, shifts and one function per lit row (`sky_glow_row`) took the glow part
  from ~6.5 to ~2.9 ms and day/night frames (gradient search + fill) from 4.5 to 2.4 ms.
* Remaining at sunset (bench scene, rough split): gradient search 66k exact evaluations (sqrt + division each; the number of
  distinct f levels per row is inherent), per-pixel dot products for the glow test ~2 ms, glow pixels ~2.9 ms, discs and fill the rest. Below 5 ms would need
  SIMD or a faster sqrt/div path in the compiler (or giving up bit identity with the plain model), see roadmap r54.

## Approximate mode (`Sky.approx`, r54 / z2)
`sky.approx = true` (default false = bit exact as above; mcplay sets it unless `MCPLAY_SKY_EXACT=1`) swaps the two costly parts
of `sky_draw` for approximations that stay within **1 level per channel** of `sky_draw_plain` (tests/2767: 120 random scenes,
max diff 1, 0.25 % of the pixels differ, PSNR ~78 dB, gate >= 60 dB):
* gradient index (`sky_f_row_approx`): exact `floor(256 gg)` every 16 columns, linear in between where the second difference of the
  neighbouring samples is < 1.0, exact per column elsewhere (no row bisection, no monotonicity assumption),
* glow (`sky_glow_mid_fast`/`sky_glow_v`): ray `d(k) = d0 + k s` is linear in the column; `1/|d|` from an anchor (sqrt + division)
  and the series `(1-t)^(-1/2)`, t = 1 - |d|^2/|anchor|^2, anchor renewed when t^2 > 0.0025; weight exact every 8 columns, linear in
  between where the second difference is < 4 (error <= 0.5 level), per pixel near the sun; blend as 2 words (r/b in 16 bit lanes).
`tools/mc/sky/bench.fi` (640x360, pitch -10 sunset, min of 8 runs, load ~20): sunset exact 7.7-8.3 ms, **approx ~3.0 ms**
(gradient only without glow: exact 3.5 ms); noon / night exact 2.4 ms.

### Who uses which mode (r161)
* **mcplay:** approx by default (`MCPLAY_SKY_EXACT=1` = exact).
* **mcview:** exact by default, because it is the reference of the golden hashes / `tools/mc/view/ref_render.py` (a 1-level change would break them).
  `MCVIEW_SKY_APPROX=1` switches approx on (opt-in, for speed comparisons with mcplay; hashes then differ).
* **Perf series (`tools/mc/perf`, MC-PERF.md):** the world frame excludes the sky (begin + world + end), so approx does not change any series number;
  the sky is measured on its own with `tools/mc/sky/bench.fi` (exact and approx variants both there). No separate bench variant needed.

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

## Measured (server, release, Load ~10)
`sky_draw` 640x360: ~28 ms/frame (125 ns per pixel: normalise = sqrt + 3 divisions in f64). Too slow for 60 fps;
options: draw at half resolution, cache the gradient per (time, camera orientation), or per-row/column incremental rays.

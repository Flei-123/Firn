# Lighting / visual fidelity audit vs Vanilla 1.20.1 (26.10.2026)

Vanilla column = algorithm of ModelBlockRenderer / LightTexture / LevelRenderer as known from the 1.20.1 source (not run; numbers derived). Status: OK, FIXED (this audit), GAP (open, roadmap).

| # | Item | Vanilla | Ours before | Status now |
|---|---|---|---|---|
| 1a | Per-vertex light | avg of 4 cells (front, 2 sides, corner); packed light is NOT rounded (1/4 level steps); dark cells (0) replaced by front-cell light; corner replaced by side when both sides opaque | avg of non-opaque cells of the 2x2 patch, rounded half up to integer levels (4+4 bit in word1) | GAP (small): integer levels only; free word0 bits 29-31 could carry half levels |
| 1b | AO curve | shade 0.2 for solid, avg of 4 -> 1.0 / 0.8 / 0.6 / 0.4 | fq = 100 + 52 ao -> 1.0 / 0.80 / 0.59 / 0.39 | OK |
| 1c | Quad flip (anisotropy) | none in vanilla (known artefact) | diagonal chosen by brighter pair (max(sky, block) * (100 + 52 ao)) | OK (deliberately better than vanilla) |
| 2a | Light level -> brightness | b = f / (4 - 3f), f = level / 15 (level 8 = 0.22) | linear (level 8 = 0.53): caves far too bright | FIXED: `mc_lightmap.fi` + 5 fragment shaders + CPU raster (vanilla mode) |
| 2b | Lightmap colour | block light warm `(n, n*((0.6n+0.4)*0.6+0.4), n*(0.6n*n+0.4))`, n = 1.5 b(level); sky `lerp((d,d,1),(1,1,1),0.35)`; 2 x 4 % mix to 0.75 gray; gamma curve 1-(1-v)^4 | fixed tint 256/224/176, max() of sky and block | FIXED (same file) |
| 2c | Day/night sky light | `getSkyDarken`: timeOfDay curve, cos, rain/thunder -5/16, `*0.8+0.2` | linear in sun height (mc_sky.daylight) | FIXED: `mc_skylight.fi` (noon 256, sunset 180, midnight 51 of 256; rain 192) |
| 2d | Brightness option | Moody 0 / Default 0.5 / Bright 1 (gamma mix) | none | FIXED: Video Settings slider, `opt_gamma` |
| 2e | Block light flicker | k = 1.5 + random walk (torch flicker) | none | GAP (cosmetic) |
| 2f | Propagation (torch, lava, glowstone, opaque / translucent) | -1 per block, max(1, opacity), sky column | mc_light.fi fixpoint `L(c)=max(base, max_n L(n)-max(1,op))`, fuzz-tested vs full recompute (tests 2831, 2885, 2906, 2907) | OK |
| 3 | Face shade | up 1.0, down 0.5, N/S (z) 0.8, E/W (x) 0.6 | x faces 0.8, z faces 0.6 (swapped) | FIXED in vanilla mode (shader remap 204 <-> 153, raster `vanilla_face_k`); legacy mode unchanged for old goldens; entity shaders untouched (own diffuse in vanilla) |
| 4 | Smooth Lighting Off / Minimum / Maximum | enum OFF/MIN/MAX (lang keys exist in 1.20.1) | no option, always smooth + AO | FIXED: `F_FLAT_LIGHT` mesher flag (front-cell light). Off = flat + no AO, Minimum = flat + AO (our reading, MIN semantics not verified), Maximum = smooth + AO. Button in Video Settings, remesh on change |
| 5 | Biome tint grass / foliage / water | per-biome colormap, blend radius 5x5 default | mc_biome_tint / zoom / update, model tint | OK (r240 open: per-position tint of models) |
| 6 | Fog / underwater | per-biome fog, water fog colour blend 5 s, vision 0.25..1 | mc_biome_fog, mc_water_fog | OK |
| 7 | Mipmaps / anisotropy | mipmap level 4 default, aniso 1 | GL `glbe_set_filter` exists, NOT called by mcplay (nearest only) | GAP: needs option + laptop perf measurement (u_filter=1 uses textureGrad) |
| 8a | Sun / moon | textured quads, 8 moon phases | discs, no phases | GAP |
| 8b | Stars | ~1500 stars at night | none | GAP (sky is python-mirrored; needs GL + VK + CPU pass) |
| 8c | Clouds | 2 layers, 12 block cells, height 192 | none | GAP (biggest remaining visual item) |
| 9 | Light updates on place / break | server update_light + client prediction | `wl_edit` incremental (6 us surface edit), update_light 1.14-1.21 decoded, region follows player | OK |

## New files
`lib/mc/mc_lightmap.fi` (closed-form lightmap, packed light args), `lib/mc/mc_skylight.fi` (getSkyDarken), `F_FLAT_LIGHT` in mc_mesh, `mc_opts` fields `smooth_light`, `brightness`.

## Interface (no new vtable)
Vanilla mode = `set_light(day < 0, tint_r = gamma 0..256)`: `day = -1 - skyDarken256`. CPU raster, GL (terrain + models + web), Vulkan (rebuilt .spv) evaluate the same closed form per fragment on the interpolated levels. `day >= 0` keeps the legacy arithmetic (old goldens identical). mcplay: `MCPLAY_VLIGHT=0` returns to legacy.

## Tests
* tests/5710_mc_lightmap.fi: 12 hand-derived lightmap values (python float model), curve darker than linear at level 8, sky darken noon / sunset / midnight / rain, pack round trip.
* tests/5711_mc_flat_light.fi: flat light = front-cell light on all 4 corners, smooth differs.
* tools/gpu dump + vkmc dump: scenes `vlit_day`, `vlit_dusk`, `vlit_night` GL / VK vs CPU raster (PSNR 37.9 - 38.8 dB, threshold 37.5).
* Not done: laptop screenshots (cave / outdoors / underwater / night) - no time left in this run.

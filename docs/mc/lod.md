# mc_lod -- distant terrain as few quads (lib/mc/mc_lod.fi)

`lod_heights` (top visible block per x/z of a column) -> `lod_bin` (bin 2/4/8, max height + id of that block) -> `lod_mesh` (one mesh per
column: top quad per cell + skirt quads where a neighbour cell is lower; `nbr` = binned heights of the 4 neighbour columns closes the gaps
to them, 0 = no skirt). Same vertex layout as mc_mesh, draw at (cx*16, min_y + y0, cz*16) with the returned y0 (8 bit coordinates, clip at 255).
Test tests/2774_mc_lod.fi (layout, skirts, bin max, heights == terrain scan, image of LOD columns == image of full meshes on a flat world);
`bash tools/mc/lod/run.sh`, `bash tools/mc/lod/run.sh --bench`.

## Measurements (12.10., terrain world, 49 sampled columns x 8 sections, extrapolated; 9x9 columns stay full detail)
| view distance (columns) | full detail tris / bytes | LOD bin 4 | LOD bin 2 |
| --- | --- | --- | --- |
| 8  | 341,904 / 9.6 MB | 111,534 / 3.1 MB | 159,637 / 4.5 MB |
| 16 | 1,288,353 / 36.1 MB | 171,942 / 4.8 MB | 405,057 / 11.3 MB |
| 32 | 4,998,433 / 140.0 MB | 408,742 / 11.4 MB | 1,367,105 / 38.3 MB |
Per column: 1183 tris full, 75.5 (bin 4), 307 (bin 2); building the LOD column 0.2 ms vs 4.1 ms for the sections.

## Stream integration (mc_stream, 13.10., `s.lod_dist > 0`; set it BEFORE loading columns; default 0 = off)
- Per column mode (column table words C_*): 0 full detail, 1 LOD only (sections neither meshed nor drawn, their meshes freed), 2 drawn as LOD while the sections mesh again.
- Switch rules (no hole by construction): full -> LOD only when the LOD mesh is uploaded; LOD -> full only when every section of the column is meshed
  (`lod_col_ready`); the old representation stays on screen until then. Hysteresis `lod_hyst` (1): full -> LOD beyond `dist + hyst`, LOD -> full within `dist`.
  LOD meshes are built from distance `dist + 1` on (own budget `lod_budget_us`, nearest first, 64 per frame), bin 4 up to `lod_far_at` (24) columns, bin 8 beyond (hysteresis too).
- Skirts toward neighbours from their binned height maps (4 neighbours, same bin); height maps cached per column (1 KB), invalidated by stream_chunk_loaded / stream_mark_block / unload.
- The cull window is `dist + hyst`; `stream_drop_far` must be called with `extra >= lod_dist - dist` when LOD is on. LOD columns are frustum-tested per column (sections of its height range).
- Test tests/2861_mc_stream_lod.fi: modes, hysteresis (A, B), flight with 3 submits/frame (C), triangles (D), free (E).

## Flight holes (r101, 13.10., FIXED)
Cause (found by rendering single columns): LOD cells are MAXIMA of their bin. In the near ring (mode 2: LOD is drawn while the sections mesh) the bin-4 surface bulges
over the real terrain and the eye sits inside / below it, so the real ground in front of the camera stayed uncovered (351 px). Two fixes:
- `lod_bin_near` (default 1) for columns up to dist + hyst + 1 (+1 hysteresis): exact height field + skirts, ~2.5x the triangles of bin 4 for ~25 columns; the scratch of lod_build is sized for it.
- `lod_ahead` (default 2): LOD meshes are built that many columns beyond `lod_dist` (drawn only inside it), so the far edge does not wait for the build budget.
tests/2861 C (70 frames, 3 submits/frame, loaded server): worst 351 -> 17 px, total 3098 -> ~100 of 5184 x 70 (no-LOD control ~90000); asserted worst < 100, total < 600.
`stream_drop_far` keeps the whole LOD window (lod_dist + lod_ahead) by itself, whatever `extra` is passed.

## Open
- (blending: see Cross-fade) no LOD for water/trees, LOD builds run on the render thread (0.2 ms/column at bin 4, more at bin 1).
- remaining flight holes (<= 17 px) come from the build budget (wall clock) at ring switches.

## Mid ring (r78, 14.10.)
`lod_bin_mid` (default 2, 0 = off) for `lod_mid_w` (5) columns beyond the near ring: bin steps are 1 -> 2 -> 4 -> 8 instead of 1 -> 4, so each switch changes the surface less (cells are maxima of their bin).
Hysteresis +1 like the other rings. Test tests/2916_mc_lod_pop.fi: converged streams along a 40-frame flight (2 blocks/frame) against the full-detail image: differing pixels (sum over 40 frames of 5184 px)
11932 -> ~10200 (-14 %), frame-to-frame change of that error ("pop") 2104 -> ~1880 (-11 %) with the mid ring (stderr line `LODPOP`). Still no blending: pops remain, only smaller. `stream_lod_bin` = built cell size of a column.

## Seam cracks (u4, 14.10.)
Metric: pixels that are clear colour in the converged LOD stream but terrain in the FULL-DETAIL image (tests/2917, 40-frame flight, 5184 px each; the old "holes" of tests/2861 compare
against a converged LOD stream, so bulge differences counted as holes). Honest numbers before: 263 (mid ring) / 658 (no mid ring) holes over 40 frames.
- Cause 1 (the big one): `lod_mesh` used the lowest OWN cell as mesh origin y0 (unsigned coordinates). A skirt toward a neighbour that is lower than every own cell was clamped to
  height 0 and vanished -> a see-through slit along the column border (found by ray-marching the hole pixels back to world positions: all on x/z = 16k seams). y0 now also covers the
  edge cells of the neighbour height maps (tests/2774 case D).
- Cause 2: skirt heights toward a neighbour drawn at ANOTHER bin stopped at the neighbour's binned maximum -> crack between the coarse skirt bottom and the finer neighbour surface.
  `mc_lod.lod_bin_min`: the neighbour arrays hold the exact MINIMUM of each span (never above any representation of the neighbour), so no stale-neighbour-bin bookkeeping is needed.
- Result: tests/2917 holes 263 -> 80 (mid ring), 658 -> 64 (no mid ring), worst frame 8 px; tests/2861 flight worst 17 -> 3 px, total 79 -> 24 (of 5184 x 70).
- Remaining holes are 1-px grazing cases (ray exactly on an integer block edge between two quads, also inside full-detail columns): rasterizer edge rounding, not LOD logic.
- Blending in the sense of an alpha cross-fade needs a per-draw fade in the back end (CPU raster + GL); not built. The pop is reduced by the mid ring (tests/2916/2917: err 11570 -> 10052, pop 2083 -> 1852).

## Cross-fade (v2, 14.10.)
Screen-door (ordered dither) fade between two representations of a column, in the CPU raster AND the GL shader with the same Bayer 8x8 formula (`mc_raster.bayer8`, `mc_fs.glsl`).
- Backend call `set_fade(a, inv)` (mc_backend): following draws keep the pixels with `bayer8(x, y) < a` (inv = 0) or `>= a` (inv = 1); `a >= 64` = off (default). A fading-in and a fading-out mesh with the same `a` and opposite `inv` cover every pixel exactly once (no hole, no double cover). GL flushes the multidraw batch on a change. WebGL (glweb) has no fade (nop: both meshes drawn, depth decides).
- Stream: `s.lod_fade_frames` (default 10, 0 = hard switch as before) = length of the fade zone in frames; per column `C_FK` kind, `C_FT` progress 0..256 (`a = ft / 4`).
  - `FK_BIN`: LOD mesh rebuilt (bin change, neighbour change, FIRST mesh of a column shown as LOD): the old mesh (`C_OLH`, own y0/hi) fades out, the new one fades in. A rebuild during a running fade drops the old half-faded mesh.
  - `FK_TOLOD` (mode 0 -> 1): LOD fades in, the still meshed sections fade out (drawn from `lod_draw`, the column already left the cull window), freed when the fade ends.
  - `FK_TOFULL` (mode 2 -> 0): sections fade in over the LOD mesh, which fades out. Translucent (water/glass) meshes follow the same mask.
  - Coming back inside `dist` during FK_TOLOD cancels the fade (sections are still there).
- `stream_flush` waits for `st.lod_fading == 0` (converged = no fade running), `stream_lod_fade` returns the kind. Draws reset the fade to off before the entity pass.
- Measured (tests/2917 D, static camera, ring events dist 2->4, 4->2, far bin 4->8, 16 frames each, 96x54): worst frame-to-frame change 57 -> 20 px (-65 %), summed change equal (131 / 131), holes against the pre-event image 156 -> 124. Per-frame change is spread over the fade instead of one frame.
- GL vs CPU (tools/gpu dump.fi scenes `fade_a` / `fade_b`, a = 24): coverage mismatch 2-3 px of ~12k / ~20k, PSNR 47 / 45 dB (llvmpipe).
- Trade-off: error against the converged full-detail image is higher while fading (a fading column keeps its old look for `lod_fade_frames` frames).

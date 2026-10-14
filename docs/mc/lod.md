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
- no blending (pop at bin switches), no LOD for water/trees, LOD builds run on the render thread (0.2 ms/column at bin 4, more at bin 1).
- remaining flight holes (<= 17 px) come from the build budget (wall clock) at ring switches.

## Mid ring (r78, 14.10.)
`lod_bin_mid` (default 2, 0 = off) for `lod_mid_w` (5) columns beyond the near ring: bin steps are 1 -> 2 -> 4 -> 8 instead of 1 -> 4, so each switch changes the surface less (cells are maxima of their bin).
Hysteresis +1 like the other rings. Test tests/2916_mc_lod_pop.fi: converged streams along a 40-frame flight (2 blocks/frame) against the full-detail image: differing pixels (sum over 40 frames of 5184 px)
11932 -> ~10200 (-14 %), frame-to-frame change of that error ("pop") 2104 -> ~1880 (-11 %) with the mid ring (stderr line `LODPOP`). Still no blending: pops remain, only smaller. `stream_lod_bin` = built cell size of a column.

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

## Open
- no switching/blending between LOD and full detail (pop at the ring), no LOD for water/trees, no stream integration yet (stream draws sections only).

# mc_mesh -- chunk section -> vertex / index buffers on the CPU

`lib/mc/mc_mesh.fi` (first version written by sub-worker C, finished and cross-checked by the lead). No GPU, no window.

```
mesher_new / mesher_free
mesh_section(&m, &section, neighbours[6], MISSING_AIR | MISSING_UNKNOWN, &tables, flags)
mesh_padded(&m, padded18x18x18, &tables, flags)        the caller assembled a 3x3x3 neighbourhood
flags: F_GREEDY (merge equal faces into rectangles)  F_AO (per-vertex ambient occlusion)  F_CULL_SAME (glass next to glass)
MeshTables { opaque: u8[32768], tiles: u16[32768 * 6] }   per block-state id; tile 0 = no face; the registry feeds opaque
                                                          (mc_registry.reg_fill_opaque)
output: 8 octet vertices (x y z face ao | tile u v) + u32 indices (6 per quad); vtx_* accessors, mesh_used_bytes
```

* Faces toward an UNKNOWN neighbour (id 65535, not loaded) are not emitted and it does not occlude.
* Greedy merging only joins faces with equal tile AND equal AO at all four corners, AO constant along the merge direction, so the
  interpolated shading of a merged quad equals that of the single faces. The quad is split along the diagonal that joins the brighter
  pair of corners (no anisotropy artefacts); winding is counter-clockwise seen from outside.

Tests: `bash tools/mc/run.sh mesh` -- a brute force checker written independently in python (own visibility rule, own AO): 11 world
shapes x 8 flag combinations = 128 meshes; quads cover exactly the visible faces once, tile and corner AO of every covered cell equal the
quad's, naive = 1 quad per face, greedy <= naive, diagonal and winding of every triangle; counter-check with a damaged AO rule.
`tests/2730_mc_mesh_basic.fi`: real `mc_chunk` sections, neighbours, unknown neighbours.
Measured (release-fast, this server): terrain-like section 1340 sections/s (~800 quads, 43 KB), dense random section 310 sections/s
(7700 quads, 430 KB) -- single threaded; a 24-section column of terrain ~ 56 columns/s.

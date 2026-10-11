# mc_render_world -- world -> section meshes -> image

`lib/mc/mc_render_world.fi`, test `tests/2753_mc_render_world.fi`. Uses `mc_world`, `mc_chunk`, `mc_mesh` (`mesh_padded`, F_AO|F_GREEDY),
`mc_camera` (frustum), `mc_raster` (backend interface: `backend_mesh_upload/draw/free`).

* `wr_new(min_y, dist_chunks, &MeshTables)`, `wr_render(&wr, &world, &raster, &camera, max_mesh)` (call `backend_begin` first), `wr_free`, `wr_clear`.
* Per frame: columns within `dist` chunks (chebyshev) -> frustum test per 16^3 section -> mesh cache lookup (hash table, key chunk x / section / chunk z)
  -> dirty or new sections are meshed from the 3x3x3 neighbourhood (exact AO on borders; unloaded neighbours and space above / below the world =
  "unknown": no faces) -> visible list sorted front to back (squared centre distance, ties by slot) -> drawn. Air sections (single invisible block) cost nothing.
* `max_mesh` limits the sections meshed per frame (nearest first, `st.deferred` counts the rest; an old dirty mesh is still drawn).
* `wr_mark_block(x,y,z)` dirties the section + the neighbours whose shell contains the block; `wr_mark_chunk(cx,cz)` dirties the 3x3 columns (chunk load / unload).
* `wr_make_chunk(cx,cz,nsec,ids)` builds a Chunk from plain ids (tests, tools).
* `st` (RenderStats): chunks, far_chunks, sections, culled, meshed, deferred, drawn, tris.
Checked by the unit test only (cache hit = identical hash, remesh count after a block change, distance limit, budget converges to the unbudgeted image, culling).
No independent python renderer for whole worlds yet (that needs a second mesher; the mesher itself is cross-checked in `tools/mc/mesh`).

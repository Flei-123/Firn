# mc_stream -- threaded section meshing + chunk streaming (lib/mc/mc_stream.fi)

Design is in the head of the file. Render thread owns world + back end; per frame: cull (mc_cull) -> cancel stale queued jobs -> pick sections
(visible front to back, then nearest pending) -> snapshot 18^3 on the render thread -> `jobs.group_go` one record per job (`max_inflight`
cap = re-prioritisation, FIFO) -> workers mesh with a per-thread mesher slot (CAS claim) + compute the 15 bit connectivity -> render thread
uploads within `upload_budget_us` -> draw. Neighbour wait (`nbr_wait` frames), uniform air / enclosed solid sections need no job,
`stream_drop_far` unloads far columns, `stream_flush` = synchronous loading screen. Contract: `stream_chunk_loaded` after every world_put.

## Decisions
- `setback` is OFF by default: the set-back rule of the BFS is a heuristic; in tests/2773 one of 12 random poses lost a visible section with it
  (image differs), without it all images are byte identical to the unculled reference. `s.setback = true` re-enables it (a bit more culling).
- `stream_flush` stops when a frame starts and finishes nothing and nobody waits for a neighbour (pending sections outside the window stay pending).
- Window `dist` = radius in columns around the camera column; pending sections farther away are never meshed (drop them with stream_drop_far).

## Tests / tools
- tests/2773_mc_stream.fi: image == reference for inline, 4 workers, async with budget, neighbour wait + remesh, mark_block, cancel, drop_far.
- `bash tools/mc/stream/run.sh` (-> `RESULT mc-stream: ok`), `bash tools/mc/stream/run.sh --bench [radius] [nsec] [all|cull|load|lod]` prints `RESULT <key> <value> <unit>`.

## Measurements (12.10., terrain world, radius 3 = 49 columns x 8 sections, shared server load 12-23, CPU raster 160x90 -- NO GPU)
| what | value |
| --- | --- |
| sections/frame frustum -> + cave culling | 153.5 -> 148.3 (3.4 % fewer, mostly surface poses; search 72 us -> 187 us) |
| load, 238 mesh jobs, wall: inline / 1 / 4 / 8 workers | 263 / 188 / 117 / 123 ms (186 / 260 / 418 / 399 columns/s) |
| snapshot + submit on the render thread (all jobs) | 198 ms inline (incl. meshing), 41-54 ms with workers -> the snapshot is now the serial part |
| async load with budgets: worst frame / worst upload / worst snapshot+submit | 13.9 ms (incl. raster draw) / 1.1 ms / 1.6 ms |
Cave culling pays off only where caves/tunnels hide big volumes; on a hilly surface the frustum does most of the work.

## Translucent pass (r96, 13.10.)
`stream_set_translucent(s, true, T_WATER, T_ICE, T_GLASS)` before the first frame: the job meshes a second mesh (F_TRANS) per section (EW word 7, records R_T*),
the render thread uploads it with the opaque one, `stream_frame` draws all opaque sections + LOD, then `set_pass(PASS_TRANS)` and the translucent meshes BACK TO FRONT, then PASS_OPAQUE.
`tables.opaque` bit 0 = opaque (cull_conn / id_opaque look at bit 0 only). Columns shown as LOD have no water. Test tests/2886: image == wr_render (0 / 0 / 1 px), visible vs single pass.
GL path not rendered (CPU back end only); quad sorting inside a section not done.

## Parallel snapshot (r103/r77, 14.10.)
`try_submit` only takes the 27 section pointers (`snap_ptrs`, R_PTR) and decides air / enclosed solid (`snap_kind`) on the render thread. The 18^3 pad is filled by `snap_fill`
(row-wise `row_ids` instead of one `container_get` per cell) by whoever comes first: the worker at the start of `mesh_one` (`snap_ensure`, R_SNAP 1 -> 2 -> 0 by CAS) or the end-of-frame
barrier `snap_barrier` on the render thread (so the world may change again after `stream_frame`; `stats.snap_wait_us` = time spent there). A job cancelled while queued never fills (R_SNAP = 0).
Test tests/2915_mc_stream_budget.fi: async load from empty with 4 workers: >= 90 % of the frames keep the snapshot+submit phase inside `snap_budget_us` (+3 ms), no frame above 250 ms,
image == reference. Image identity of all other tests (2773/2861/2885/2886) unchanged.

## Open
- snapshot pointers + kind are the only serial part now (see "Parallel snapshot"); the pad fill itself still reads the live world, so the world must not change while a frame runs (true: stream_frame owns it).
- light: see docs/mc/light.md ("Light in Stream / Render"): `stream_set_light`, region volume + Chunk.light, `stream_light_sync` after edits. Moving region and update_light wiring open.
- GL path uses stream_frame through mc_backend (not measured on a real GPU).

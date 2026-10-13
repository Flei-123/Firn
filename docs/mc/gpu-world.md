# GPU world path (lib/gpu/gl_mc.fi) -- state 2026-10-12, worker mc-p1

Honest scope: ALL numbers come from Mesa llvmpipe (CPU rasterizer, GL 4.5 core), NOT a GPU. They show call counts and
correctness, not GPU speed. Nothing here is a claim about Vanilla.

## Done (tested = `tools/gpu/dump.fi`, gate 119, + `tests/2770_gpu_pool.fi`)
| # | Item | State |
|---|---|---|
| 1 | Pool allocator: sections are blocks in big VBO / IBO "pages" (default: first page 128K vertices = 1 MB VBO + 0.75 MB IBO, later pages x4 up to 1M = 8 MB + 6 MB); best-fit free list with coalescing (`lib/gpu/glpool.fi`, pure, unit test 2770); `glbe_defrag` rebuilds a page with glCopyBufferSubData; automatic defrag/new page when a block does not fit | tested (allocator + frames); defrag path compiled and called but NOT exercised (the test never fragments: 0 pages rebuilt) |
| 2 | Multi draw: `MODE_MULTI` = one glMultiDrawElementsBaseVertex per page; `MODE_INDIRECT` = glMultiDrawElementsIndirect (only GL >= 4.3, else MULTI); `MODE_POOL` = one glDrawElementsBaseVertex per section; `MODE_LEGACY` = old VAO per section. Section origin without gl_DrawID: `gl_VertexID >> 6` -> granule table (R32UI buffer texture per page) -> slot -> origin table (RGBA32F buffer texture, camera relative f32) | tested: all modes give the SAME frame (checksum) |
| 3 | Texture ARRAY (layer = tile-1, GL_REPEAT, no atlas bleeding) replaces the atlas; `glbe_set_filter(mip, aniso)` = mipmaps + anisotropy (u_filter=1, cut-out at alpha 0.5) | default exact path tested (PSNR table unchanged); mip/aniso path compiled only, NOT rendered/measured |
| 7 | Upload budget: `glbe_set_upload_budget(bytes)` stages mesh bytes on the CPU, `begin` uploads <= budget per frame (>= 1 section), staged sections are skipped by draw | tested (checksum equal after 11 frames at 32 KB) |

Constraint of MULTI: one slot has ONE origin per batch. Drawing the same handle twice in a frame (the dump test does)
splits the batch (extra call); a real world has one slot per section.

## Measured (llvmpipe, 640x360, 25 sections, 51050 tris/frame, median of 21 frames; busy server, +-15 % noise)
| mode | GL draw calls/frame | frame_us (with finish) | submit_us (CPU side) | upload 25 sections |
|---|---|---|---|---|
| legacy (VAO per section) | 25 | 12055 .. 13289 | 8160 .. 8944 | 481 us |
| pool_single (BaseVertex) | 25 | 12604 .. 15135 | 8560 .. 8738 | 2875 us |
| multi | 1 | 12565 .. 13343 | 8870 .. 9034 | 3066 .. 5719 us |
| indirect | 1 | 13062 .. 13366 | 8904 .. 9011 | 2859 .. 3326 us |
Result: draw calls 25 -> 1 per page, frame time on llvmpipe unchanged (rasterizer bound; draw-call overhead is not
visible on a software renderer). Pool upload is slower (6x) than one buffer per section on llvmpipe (extra bind + 2
glBufferSubData per section through dynlib). Whether fewer calls help on a real GPU/driver is UNMEASURED.

## Vertex format (step 4)
mc_mesh vertex = 8 bytes: word0 = x|y<<8|z<<16 (8 bit each, 0..16) | face<<24 (3 bit) | ao<<27 (2 bit) -> bits 29..31 free;
word1 = tile (16) | u<<16 | v<<24. Sky/block light (4+4 bit) would fit in word0 bits 29..31 only for 3 bits: NOT enough
for 8 bits; it needs a third word (12 B/vertex) or u/v narrowed (greedy quads reach 16 = 5 bit each -> 6 bits free in word1).
Section: 4084 vertices = 32672 B VBO + 24504 B IBO (test chunk). Not implemented, only analysed.

## Not done
- 5 transparency pass: built, see mc-r3 below (GL vs CPU image comparison still open).
- 6 instanced entities: scene-layer instancing existed (gl_scene); mc-r3 added the backend hook + box models (see below); draw-call measurement open.
- persistent mapped buffers (`gl_buffer_storage`, `gl_map_range`, fences are wrapped, unused/untested).
- real GPU, mip/aniso image test. The Windows twin under Wine (tools/gpu/run.sh) ran and gave the same scene statistics.

## Note from gpu-h (2026-10-12)
* Vertex format: unchanged (8 bytes, light 4+4 bit NOT added). Entities / HUD / sky use their own programs and do not touch it.
* "Pool upload 6x slower" (427 us legacy vs 2541 us pool for 25 sections, llvmpipe): cause NOT found this round. Per section the
  pool path does 3 `glBufferSubData` (granule table ~2 KB, VBO 32 KB, IBO 24 KB) + 3 binds into 8 MB / 6 MB buffers against 2
  `glBufferData` calls; first-touch page faults of the fresh big buffers are the first suspect (unmeasured: upload the same sections
  twice and compare). Not a real-GPU claim either way.

## Upload probe (gpu-l, 2026-10-12, `tools/gpu/mc/upload_probe.fi`, llvmpipe)
* Raw GL, no gl_mc: 8 MB `BufferData(NULL)` + 25 x `SubData(32 KB)`: first pass ~130 us, second/third ~30-35 us (first touch of the pages is ~4x). 25 x `BufferData(32 KB)` on 25 buffers (legacy): first ~650-770 us, then ~40-50 us. 75 x `SubData(2 KB)`: ~24 us.
* So raw SubData into a pool is NOT slower than legacy; the "pool 6x slower" (2541 vs 427 us) is not the upload itself. Remaining suspects: pool bookkeeping in gl_mc (granule table), the extra binds, or the measurement including first touch of the 8/6 MB buffers on a cold run. Next step: time the pool path inside gl_mc with a warm second pass.
* Not a real-GPU statement.

## mc-q1 (2026-10-12)
* Pool upload cause FOUND: the cold "25 sections" time was dominated by creating the first 8 MB + 6 MB page (BufferData zero-fill), NOT by SubData/binds. Warm (free all + upload again, new metric `upload_25_warm_us`): legacy 237-292 us, pool 145-161 us, multi 148-192 us, indirect 128-162 us -> warm pool is ~1.8x FASTER than VAO-per-section.
* Fix: default first page = 128K vertices (1 MB + 0.75 MB), further pages x4 up to 1M (`MAX_PAGE_VERTS`); explicit `glbe_set_page_verts` keeps fixed size. Cold 25 sections: pool 2458/2506 us -> 678/1041 us, multi 3762/2688 -> 1090/549, indirect 2869/2961 -> 1022/660 (legacy 417-487; llvmpipe, +-15 % noise).
* Vertex light DESIGN (not built; the mesher has no light input yet): word1 = tile 14 bit (<=16383) | u 5 | v 5 | sky 4 | block 4 = 32 bit; word0 unchanged (bits 29..31 stay free). u,v reach 16 in a greedy quad -> 5 bit enough. Steps when built: mc_mesh pack/vtx_* accessors, mc_raster unpack (2 places), mc_vs.glsl + glweb_vs.glsl shifts, light sampler in the mesher (section + 1 border), tests 'mesh' + image gate. Image equality holds as long as light = 15/0 default.

## mc-r3 (2026-10-13): transparency pass + entity boxes (llvmpipe/CPU only, NOT a GPU)
* Mesher split: `mc_mesh.F_TRANS`. MeshTables.opaque is now a bit field: bit 0 = opaque, bit 1 = translucent (plain 0/1 tables behave as before).
  Without F_TRANS translucent blocks are skipped; with F_TRANS only they are meshed (face hidden by an opaque neighbour or by a translucent
  neighbour with the same tile -> a water body keeps only its surface). Leaves stay cut-out (alpha 128).
* `mc_render_world`: `wr_set_translucent(wr, on, water, ice, glass)` builds a private table (tile ids -> bit 1) and meshes each section twice
  (second handle in cache word 4, own mesh object; GL: same pool, own slot). Off = old behaviour, identical frames (tests 2753/2763 green).
  Draw order: opaque front to back, then `b.set_pass(PASS_TRANS)`, translucent sections BACK TO FRONT (section centre distance, stable ties by slot),
  `set_pass(PASS_OPAQUE)`. Order INSIDE a section is the index order (no quad sort) - CPU and GL use the same order, so they still agree.
* CPU reference (`mc_raster`): `Raster.trans`: alpha >= 1 passes, blend `(src*a + dst*(255-a) + 127) / 255` per channel after shading + fog, depth test
  strict, NO depth write. python reference `tools/mc/raster/ref.py` extended: scenes `water_glass`, `water_glass_fog` byte-identical (RESULT mc-raster ok).
* GL (`gl_mc`): `be_set_pass` flushes the collected multi-draw batch, enables SRC_ALPHA/ONE_MINUS_SRC_ALPHA, depth mask off, colour alpha masked
  (frame stays opaque like the CPU), `u_trans` in mc_fs.glsl (discard alpha 0 only, output alpha). In MULTI/INDIRECT mode a page change ends the batch
  in the translucent pass (draw order across pages = submission order). Existing gate scenes unchanged (tools/gpu/run.sh ok). NOT yet rendered
  against the CPU reference (no GL translucent scene written) - the new shader path is compiled and the opaque path proven unchanged only.
* Entities: `Backend.draw_entities(ent_obj, records, n)` (record = ENT_WORDS f64: centre, size, yaw sin/cos, rgb, flags), CPU = `mc_raster.raster_draw_box`
  (flat colour, mc_mesh face winding/shading with ao 3, fog, depth; python reference scenes `boxes` byte-identical). GL = `gl_scene.scene_attach(sc, &b)`
  routes it through the existing instanced pass (ONE glDrawElementsInstanced, frustum + HiZ culling of scene_ent_flush) - compiled, NOT rendered/compared.
  `mc_entity_draw`: Store -> boxes (humanoid 6 boxes, object 1, LOD 1 box beyond far_lod, frustum culling per entity); test 2775. No skin tile (no textures).
* Not done: GL-vs-CPU PSNR scenes for water/glass/boxes, 1000-entity instanced-vs-individual draw call measurement, persistent mapped buffers / fences.

## mc-s3 (2026-10-13): GL-vs-CPU transparency/entities, entity benchmark, layer limit (llvmpipe only, NOT a GPU)
* r94 `tools/gpu/dump.fi` scenes `water_glass`, `water_glass_fog`, `boxes` (PASS_TRANS via `set_pass`, entity boxes via `gl_scene.scene_attach`, entity culling off):
  PSNR 42.3 / 43.4 / 42.4 dB, coverage mismatch 6-10 px of 57600, threshold 38 dB (same float-vs-1/16-pixel-snap + texture-sampling difference as the chunk scenes, 37-43 dB).
  Visual check: images equal by eye. Note: dump.fi still builds vertex word1 in the OLD layout (u<<16, v<<24), CPU and GL read it the same way, so it compares fine (open item).
* r95 `tools/gpu/mc/ent_bench.fi` (640x360, median of 21, busy server): 100 boxes instanced 4.0 ms / individual (100 calls) 9.4 ms / CPU raster 4.9 ms;
  1000 boxes instanced (1 glDrawElementsInstanced) 10.3 ms / individual (1000 calls) 54.2 ms / HiZ+frustum culling 8.2 ms (991 drawn) / CPU raster 20.5 ms.
  Persistent mapped buffers / fences NOT built: on llvmpipe the upload is not the bottleneck, nothing measurable to justify them.
* r99 `tools/gpu/mc/layers.fi`: llvmpipe GL_MAX_ARRAY_TEXTURE_LAYERS = 2048 (GL 3.3 only guarantees 256). 1120 layers (mc_texext 64+i) at tile 16 and 64 render the same
  colours as the CPU (upload 6 ms / 89-125 ms). `be_atlas` now reads the limit (`glbe_max_layers`) and uploads only what fits (`glbe_layers_lost`); tiles beyond read the
  last layer. A real fallback (2nd array / atlas pages) is open. Defrag with 24 unequal sections (1-7 x 4 quads, holes): frag 408 -> 0 permille, frame checksum equal to legacy.
  Mip/aniso: 1-px checker luma stddev nearest 49.5, mip 4.0, mip+aniso8 1.4 (smoother as expected; no CPU reference for mip).

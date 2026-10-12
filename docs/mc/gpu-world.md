# GPU world path (lib/gpu/gl_mc.fi) -- state 2026-10-12, worker mc-p1

Honest scope: ALL numbers come from Mesa llvmpipe (CPU rasterizer, GL 4.5 core), NOT a GPU. They show call counts and
correctness, not GPU speed. Nothing here is a claim about Vanilla.

## Done (tested = `tools/gpu/dump.fi`, gate 119, + `tests/2770_gpu_pool.fi`)
| # | Item | State |
|---|---|---|
| 1 | Pool allocator: sections are blocks in big VBO / IBO "pages" (default 1M vertices = 8 MB VBO + 6 MB IBO); best-fit free list with coalescing (`lib/gpu/glpool.fi`, pure, unit test 2770); `glbe_defrag` rebuilds a page with glCopyBufferSubData; automatic defrag/new page when a block does not fit | tested (allocator + frames); defrag path compiled and called but NOT exercised (the test never fragments: 0 pages rebuilt) |
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
- 5 transparency pass (water/glass sorted back to front, alpha blend): NOT started (mesher has no opaque/water split).
- 6 instanced entities: NOT started (wrappers `gl_draw_elements_instanced`, `gl_attrib_divisor` exist, untested).
- persistent mapped buffers (`gl_buffer_storage`, `gl_map_range`, fences are wrapped, unused/untested).
- real GPU, mip/aniso image test. The Windows twin under Wine (tools/gpu/run.sh) ran and gave the same scene statistics.

## Note from gpu-h (2026-10-12)
* Vertex format: unchanged (8 bytes, light 4+4 bit NOT added). Entities / HUD / sky use their own programs and do not touch it.
* "Pool upload 6x slower" (427 us legacy vs 2541 us pool for 25 sections, llvmpipe): cause NOT found this round. Per section the
  pool path does 3 `glBufferSubData` (granule table ~2 KB, VBO 32 KB, IBO 24 KB) + 3 binds into 8 MB / 6 MB buffers against 2
  `glBufferData` calls; first-touch page faults of the fresh big buffers are the first suspect (unmeasured: upload the same sections
  twice and compare). Not a real-GPU claim either way.

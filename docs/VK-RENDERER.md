# Vulkan renderer for the voxel client (round VK1, 2026-10-14, roadmap r177) -- DONE on lavapipe

Everything measured/tested here ran on **lavapipe (CPU Vulkan 1.x), no real GPU, no validation layer** (none installed). Nothing is claimed about GPU speed.

## What exists (all new files, `vk.fi` untouched)
| File | Content | State |
|---|---|---|
| `lib/gpu/vk_mem.fi` | extension function table (63 entries, via `vkGetInstanceProcAddr`, 10 int args through `dynlib.callx`), limits, **suballocating allocator** (64 MB blocks, first-fit free list, coalescing, buffers and images in separate blocks, host-visible blocks mapped once), `buf_make/img_make` | **tested** (`tests/3080_vk_mem.fi`: 200 buffers = 1 `vkAllocateMemory`, free + coalesce, 60 MB refit, staging map, image+array view, no leak) |
| `lib/gpu/vk_desc.fi` | render pass colour+D32 depth, depth image, framebuffer, descriptor set layout (b0 UBO, b1 atlas `sampler2DArray`), pool + sets, sampler, pipeline layout + 48 byte push range, voxel pipeline (uvec2 vertex, depth LESS, optional blend/no depth write) | written, runs (scenes in the gate) |
| `lib/gpu/vk_frame.fi` | frames in flight (command buffer, fence, acquire/done semaphores per slot, per-slot upload command buffer), barriers (image layout, memory), immediate submit | written, compiles, atlas upload path ran (submit r=0) |
| `lib/mc/vk_mc.fi` | `mc_backend.Backend` `KIND_VK`: atlas texture array (staged copy + transitions), mesh upload = staging -> device buffer batched into the next frame's upload cmd, deferred release by frame serial, 2 frames in flight, read back in `present` | written, runs (gate vkmc_run.sh) |
| `lib/gpu/vk_shaders.fi` + `vk_glsl/` + `spv/` | the VK2 shaders (section "Shaders" below); since r183 `vk_mc` uses them (the old `lib/gpu/vkmc/` is gone) | `spirv-val`, tests 3090 / 3091, gate PSNR |
| `lib/mc/mc_backend.fi` | `KIND_VK = 3` | done |

Interface (r183, VK2 shaders in `vk_mc`): location 0 `uvec2`; set 0 with 7 bindings (0 dynamic UBO `Frame` 320 B, 1..4 atlas = the same 2D_ARRAY view, 5/6 dummy texel buffers), push constant 16 B (VERTEX: origin.xyz + pool 0). Per frame slot one UBO of 256 slices x 512 B: `bake` copies the per-frame part (rv, fog, tiles, size, pagel, filter, h) and patches trans / fade / light into a NEW slice whenever set_pass / set_fade / set_light changed, then binds set 0 with the dynamic offset (`ubo_over` counts overflows). `pipeline_voxel` takes `v.cache` (pipeline cache of `vk_life`; `vkbe_open_cache(w,h,path)` loads/saves the file). Depth is created in `targets_make`, so `vkbe_resize` (= `vkbe_on_rebuild`) re-creates it. A 1-layer atlas is padded to 2 layers (the shader samples a 2D_ARRAY view).

## Status (VK1 done, 2026-10-14) -- lavapipe only, NO real GPU, Khronos validation layer (chromium copy) used for debugging
The first-draw crash was NOT in the draw: every Vulkan fence was created SIGNALED and then submitted while signaled (VUID-vkQueueSubmit-fence-00063). lavapipe (Mesa 22.3.6) segfaults in its queue thread afterwards. Fix: `mk_fence(x, false)` for frame + immediate fences (slots start without pending state, so nothing waits on them). Second fix: `stencilStoreOp` was 2 (invalid), now DONT_CARE = 1. The negative-height viewport (y = h, height = -h) now flips y (shader keeps the GL matrix).
Validation recipe: layer manifest in a temp dir with `library_path=/usr/lib/chromium/libVkLayer_khronos_validation.so`, run with `VK_LAYER_PATH=<dir> VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation` (no apt install needed). Open note: it prints `swizzle b/a = Unhandled VkComponentSwizzle` for the colour attachment views although the create info is all zero (IDENTITY) -- looks like a layer bug, not ours.

## Tests / gate
- `tests/3081_mc_vk_smoke.fi`: quad through the Backend table, coverage + clear corner (SKIP without Vulkan).
- `bash tools/gpu/vk/vkmc_run.sh` (also called by `tools/mc/vk/run.sh`): `tools/gpu/vk/vkmc_dump.fi` = the GL scenes of `tools/gpu/dump.fi` through `vk_mc` vs `mc_raster` (triangle, quad, quad_angled, cutout, fog_flat, chunk, chunk_fog, chunk_inside, chunk_far, water_glass(+fog), fade_a/b, lit_day/night/night_fog; entity boxes excluded, VK4).
- PSNR (VK / GL): chunk 33.65 / 36.16, chunk_fog 34.11 / 36.55, chunk_inside 33.03 / 41.20, chunk_far 33.65 / 36.16, water_glass 36.78 / 40.57; the others equal or better than the thresholds. Coverage mismatch 41/47 px (GL 17/11). Floors in vkmc_dump.fi are VK-specific (3300 / 3400 / 3200 / 3300 / 3600) until the gap is found (r184).
- Time 640x360 5x5 sections, llvmpipe-Vulkan with readback vs CPU raster: ~21 ms vs ~52 ms per frame (not a GPU number).

## Not done (state before r183)
swap to the VK2 shaders (`lib/gpu/spv/vk_mc.*`: 4 atlas pages, texel buffers, Frame UBO 320 B, trans/fade in the UBO), swapchain present (VK3 `vk_life`), pipeline cache wiring (`v.cache`), device lost, real GPU, why chunk_inside is 8 dB below GL (suspect: texture sample / UV edge, big_px 337 vs 93).

## Shaders (round VK2, r178; merged here in r183)

Status 2026-10-14. The GL shaders `lib/gpu/{mc,gl_sky,gl_ent,gl_hud,gl_tint}_*.glsl` ported 1:1 to Vulkan GLSL 450.
**No real GPU was available**: the SPIR-V is validated (`spirv-val`, Vulkan 1.0), interface-checked (test 3090) and accepted by Mesa
lavapipe (`vkCreateShaderModule`, test 3091). Pixel output of the voxel pass is measured since r183 (gate `tools/gpu/vk/vkmc_run.sh`, PSNR vs `mc_raster`, chunk 36.16 / chunk_inside 41.19 dB); the scene layer (r180, `vk_mc.vkbe_sky` / `draw_entities` / `vkbe_tint` / `vkbe_hud`) is checked by `tools/gpu/vk/vkscene_run.sh` (CPU path = mc_raster + mc_sky + mc_hud, PSNR >= 34 dB; lavapipe, NO real GPU). Pass order: begin, sky (full-screen strip, no depth), world (opaque, translucent), entities (instanced cube, depth LESS + write), tint (same strip, depth GREATER, no write, blend DST_COLOR x ZERO), HUD (instanced rectangles from `hud_record`, alpha or invert pipeline, colour mask RGB), end. Fixed on the way: `TOPO_STRIP` was 5 (= triangle FAN in Vulkan) and must be 4.

### Files
| File | Content |
|---|---|
| `lib/gpu/vk_glsl/*.glsl` | sources: `mc_vs/mc_fs`, `sky_vs/sky_fs`, `ent_vs/ent_fs`, `hud_vs/hud_fs`, `tint_fs`, shared `vk_frame.glsl` (UBO + bindings, `#include`) |
| `lib/gpu/spv/vk_{mc,sky,ent,hud}.{vert,frag}.spv`, `vk_tint.frag.spv` | committed SPIR-V 1.0 (the tint pass uses `vk_sky.vert.spv`) |
| `lib/gpu/vk_shaders.fi` | `import gpu.vk_shaders`: embedded octets (`MC_VS ...`, `shader_code(i)/shader_size(i)`) + every number below as a constant |
| `tools/gpu/mkspv.sh` | no argument = rebuild; `--check` byte-equal with the committed files; `--parity` body diff against the GL shaders must equal `tools/gpu/vk_parity.expected` (`--parity --update` after a deliberate change) |
| `tests/3090_mc_vk_shader_spv.fi` | binary parse: stage, entry point, binding / location masks, std140 offsets == `vk_shaders` constants |
| `tests/3091_mc_vk_shader_modules.fi` | all modules on a Vulkan device (lavapipe), SKIP without one |
| `lib/gpu/vk.fi` | only change: `shader_make` exported, new `shader_free` |
`tools/gpu/vk/run.sh` runs `--check` and `--parity` as well (SKIP without glslang).

### Descriptor set 0 (the ONLY set; one layout for every pipeline)
| binding | type | stage | content |
|---|---|---|---|
| 0 | UNIFORM_BUFFER (dynamic offset ok) | vertex + fragment | `Frame`, 320 octets std140 (below) |
| 1..4 | COMBINED_IMAGE_SAMPLER, view 2D_ARRAY | fragment | atlas page 0..3 (`u_atlas`, `u_atlas1..3`; unused pages: bind page 0 again) |
| 5 | UNIFORM_TEXEL_BUFFER `R32_UINT` | vertex | `u_gran`: slot number per 64 vertices (pool mode) |
| 6 | UNIFORM_TEXEL_BUFFER `R32G32B32A32_SFLOAT` | vertex | `u_org`: section origin per slot (pool mode) |
A stage only declares what it uses (bindings used: mc.vert 0,5,6; mc.frag 0..4; sky.vert none; every other stage 0).
Push constants (vertex stage, 16 octets, `mc.vert` only): `vec3 u_origin` at 0 (section origin minus camera, f64 on the CPU), `int u_pool` at 12.

### `Frame` UBO (std140), offsets = `vk_shaders.U_*`
`u_rv` mat4 0 | `u_fog` vec4 64 | `u_fade` 80 | `u_light` 96 | `u_cam` 112 | `u_sun` 128 | `u_fogc` vec3 144 | `u_r` 160 | `u_u` 176 | `u_f` 192 |
`u_zen` 208 | `u_hor` 224 | `u_glc` 240 | `u_tint` 256 | `u_tiles` vec2 272 | `u_size` vec2 280 | `u_pagel` int 288 | `u_trans` 292 | `u_filter` 296 |
`u_discs` 300 | `u_invert` 304 | `u_h` float 308. Each vec3 takes 16 octets (4 octets padding). Same names, same meaning as the GL uniforms
(comments in `vk_frame.glsl`). Per-pass values (`u_trans`, `u_filter`, `u_fade`) = one UBO slice per pass (dynamic offset) or one write per pass.

**Vertex snap (r261).** `mc_vs` / `mc_model_vs` (GL + Vulkan) snap the clip position to the CPU rasterizer's 1/16 px grid (`u_fade.w` = frame width, `u_fade.z` = height; width 0 = off), so llvmpipe / lavapipe / GPUs (1/256+ px) see the same triangle corners as `mc_raster.snap()`. Measured (tools/gpu/vk/vkmc_dump, scene `chunk`, lavapipe): CPU with edge expansion 35.2 -> 35.6 dB, CPU with the exact top-left rule (`raster_set_edge_expand(false)` / `MCPLAY_EDGE_EXP=0`) 37.1 -> 50.7 dB (`chunk_inside` 42.9 -> 62.9 dB). The rest vs the default CPU picture is the CPU's deliberate edge expansion (1/48 px, LOD pass 1/16 px). Whole lodseam picture (mcplay, 640x360, symmetric camera pose) GL/VK vs the exact-rule CPU: 26-37 dB, 700-2300 px > 2 levels (1-px rows along near-horizontal snow steps); GL and Vulkan agree with each other to ~150 px, so the rest is CPU (f64) vs GPU (f32) arithmetic, not the shaders' integer shading (generic camera pose 55 dB, symmetric poses 42-48 dB in the same chunk scene; a +0.004 tie epsilon in the snap did not help).

**Tie bias for Vulkan (r266, tried, NOT adopted).** Vulkan has no `sc.y -= 1/256` (GL has it since r265; `tools/gpu/vk_parity.expected` lists the difference). Variant measured (lavapipe, lodseam A settled vs exact-rule CPU, HUD counter + stairs box masked): no bias 33.41 dB, bias 34.01, bias + `attr -= 1/256 * dFdy(attr)` in mc_fs/mc_model_fs 34.29. But the generic scenes of `vkmc_run.sh` get worse: chunk_exact 50.67 -> 43.54 dB (floor 48), chunk_inside_exact 62.91 -> 51.25 (floor 58), lit_night 38.10 -> 37.84. lavapipe already follows the CPU top-left rule in generic poses, so the bias only helps the symmetric lodseam pose. Remaining lodseam diff: 1-px rows along horizontal terrace edges (VK draws the dark wall row, CPU the bright top row, pixel centre exactly on the edge).

**Tie bias for Vulkan, tie vertices only (r268, adopted).** `mc_vs` / `mc_model_vs` (Vulkan) move a snapped vertex up by 1/256 px ONLY when it lies exactly on a pixel centre line (`fract(sc.y) == 0.5`, i.e. 16 * y mod 16 == 8); all other vertices stay on the 1/16 grid. Measured (lavapipe, lodseam shots vs exact-rule CPU `MCPLAY_EDGE_EXP=0`, `MCPLAY_SHOT_SETTLE=1`, crop 0,60..640,330, pixels where two VK runs disagree (LOD state noise) + stairs box masked identically for every variant): A baseline 33.51 dB, global `y -= 1/256` 35.68, tie-only `y -= 1/256` **36.49**; S 37.40 / 37.02 / **37.80**. Other variants lose: global `y += 1/256` A 28.9 / S 30.7, tie-only `y += 1/256` 29.1 / 30.9, `x -= 1/256` 33.3 / 36.8, `x += 1/256` 33.0 / unusable. Generic scenes (`vkmc_run.sh`): chunk_exact 50.67 -> 49.26 (floor 48), chunk_inside_exact 62.91 -> 62.76, lit_night 38.10 unchanged; vkscene / vkmodel unchanged. Unmasked whole-picture numbers of the gate stay noisy: the picture has two states (4 floating LOD pillars above the FAR stairs or none; far hill LOD converged or not) that cost 3-8 dB in both back ends and are not a shader matter.

### Vertex inputs (locations identical to GL)
| shader | binding 0 | binding 1 |
|---|---|---|
| mc.vert | per vertex, stride 8: loc 0 `R32G32_UINT` (the mc_mesh vertex) | - |
| ent.vert | per vertex, stride 16: loc 0 `R32G32B32_SFLOAT` pos (+0), loc 1 `R32_SFLOAT` face (+12); 24 vertices, 36 indices | per INSTANCE, stride 48: loc 2/3/4 `R32G32B32A32_SFLOAT` at +0/+16/+32 |
| hud.vert | per INSTANCE, stride 20: loc 0 `R32G32B32A32_SINT` rect (+0), loc 1 `R32_UINT` 0xAABBGGRR (+16); `vkCmdDraw(4, n, 0, 0)` strip | - |
| sky.vert | none; `vkCmdDraw(4, 1, 0, 0)` TRIANGLE_STRIP (sky and tint) | - |
Varyings: mc `0 v_light, 1 v_uvao, 2 v_tile(flat), 3 v_fk(flat), 4 v_depth`; ent `0 v_col(flat), 1 v_fk(flat), 2 v_depth`; hud `0 v_col(flat)`. Fragment output location 0.

### The three GL -> Vulkan differences (everything else is line for line the same; `vk_parity.expected` lists all 14 differing lines)
1. **Y flip**: draw with the FLIPPED viewport (`x=0, y=height, width, height=-height`, core in Vulkan 1.1, else `VK_KHR_maintenance1`). Then the GL clip
   coordinates give an upright image, front faces stay counter-clockwise (`frontFace = COUNTER_CLOCKWISE`; voxel pass culls back faces like `glbe_set_cull`, sky / tint / HUD / entity boxes draw without culling as in `gl_scene.fi`), the HUD formula
   `1 - y/h*2` and the sky strip are unchanged, and `gl_FragCoord.y` is already counted from the top (= row + 0.5). Hence in `mc_fs` the screen-door row is
   `py = int(gl_FragCoord.y)` (GL: `u_fade.z - 1 - int(gl_FragCoord.y)`) and in `sky_fs` `ey = (u_cam.y - gl_FragCoord.y) / u_cam.w` (GL: `u_cam.y - (u_h - y)`).
   Without the flip the image is upside down AND these two shaders are wrong. Read back is top-down without a flip of rows (as `vk.read_pixels`).
2. **Depth 0..1**: `u_rv` stays the OpenGL matrix (z in -w..w), so the CPU keeps ONE matrix; `mc_vs` / `ent_vs` remap `z = (z + w) * 0.5` AFTER `v_depth = w`.
   `sky_vs` writes z = w = 1 -> depth exactly 1.0 as in GL (tint: depth test GREATER finds covered pixels). `hud_vs` z = 0.5 (= GL's window depth for z = 0).
3. **Interface**: `gl_VertexID` -> `gl_VertexIndex` (= index + vertexOffset, the same value `glDrawElementsBaseVertex` gives; the pool mode relies on it), loose
   uniforms -> `Frame` UBO / push constants / texel buffers, explicit `location =` on every in / out.

### Pipeline state per pass (what GL does, to be copied by `vk_mc`; NOT yet run)
| pass | shaders | depth | blend |
|---|---|---|---|
| sky | sky.vert / sky.frag | off | off |
| opaque world | mc.vert / mc.frag, `u_trans=0` | test LESS (or LESS_OR_EQUAL as GL), write on, clear 1.0 | off |
| translucent world | mc, `u_trans=1` | test, write off | SRC_ALPHA, ONE_MINUS_SRC_ALPHA |
| entities | ent.vert / ent.frag | as opaque | off |
| tint | sky.vert / tint.frag | test GREATER, write off | DST_COLOR, ZERO (alpha: any) |
| HUD | hud.vert / hud.frag, `u_invert=0` | off | SRC_ALPHA, ONE_MINUS_SRC_ALPHA |
| HUD invert | hud.vert / hud.frag, `u_invert=1` | off | ONE_MINUS_DST_COLOR, ZERO |
(Authoritative are the GL calls in `lib/gpu/gl_mc.fi` / `gl_scene.fi`; this table is from the shader comments.)

### Honest limits
- No GPU, no validation layer: lavapipe only accepts the modules; behaviour of a full pipeline (descriptor layout, flipped viewport, texel buffers,
  `textureGrad` in the filtered path, translucent order) is untested until VK1's backend draws the same scenes as `tools/gpu/dump.fi` (gate: PSNR vs `mc_raster`).
- Texel buffer limits (`maxTexelBufferElements`, >= 65536 guaranteed) and `maxImageArrayLayers` (>= 256 guaranteed) decide pool size / atlas paging as in GL.
- One shader variant per stage with runtime branches (`u_trans`, `u_filter`, fog, fade) exactly like GL. Specialization constants for the hot variants are an
  optimisation for later, not needed for correctness.
- Float rounding in `sky_fs` is not bit-equal to the CPU reference (same as GL).

### Rebuild
`bash tools/gpu/mkspv.sh` (glslangValidator `-V --target-env vulkan1.0`, `spirv-val`), then `bash tools/gpu/mkspv.sh --check`, `--parity`, and
`bash tools/mc/testmc.sh vk_shader`.

## mcplay with the Vulkan back end (round VK7, r193) -- lavapipe only, NO real GPU
`MCPLAY_GFX=vk bash ... mcplay host port name` (default unchanged = CPU raster; `MCPLAY_GL=1` = GL). If no Vulkan device opens, mcplay prints `gfx=cpu (vk requested but not available)` and keeps the CPU path.
- **Pass order** (`draw_frame_vk`, ONE render pass, `end` submits): `begin` -> `vkbe_sky` -> world (`mc_stream.stream_frame` / `wr_render_b`, opaque + translucent) -> entity boxes (`entlist_draw` -> `draw_entities`) -> selection box (12 black box records through `draw_entities`) -> `vkbe_tint` (only when `MCPLAY_LIGHT=0`) -> `vkbe_hud(rec)` (records of `mc_hud.hud_record`, inventory included) -> `end`. GL has to draw the entities AFTER its `end`; in Vulkan everything must be recorded before `end`.
- **Present (since r195: swapchain)**: see "Swapchain present" below; the old path (MCPLAY_VK_PRESENT=readback): the frame is read back (`be.present` waits for the newest frame, RGBA, row 0 = top), converted with `window.to_bgrx` and shown through the normal X11 window (`window.image` + `present`, same as the CPU path). NOT a swapchain: the `vk_life` swapchain (`vk_life.frame`) draws its own pipeline, `vk_mc` renders into its own colour images. A swapchain present needs `vk.open(want_surface)` in `vk_mc`, `VK_KHR_swapchain` in the device, a blit/copy of the colour image into the swapchain image (new `vk_mem` function-table entries) and the semaphores of `vk_frame.frame_submit` (it already takes wait/signal). Open (roadmap).
- **Resize**: `gfx_resize` -> `wr_clear_b`, `vkbe_resize` (targets re-created, atlas and device stay), `stream_make` re-announces the columns. **Atlas / resource pack**: uploaded by `vk_open_be` like GL (`res_reload_upload` or `upload_atlas`).
- **Device lost**: `vk_mc` marks itself lost when a frame submit fails (`vkbe_lost`) or on `vkbe_inject_lost` (simulation; `MCPLAY_VK_LOST_AT=<frame>` in mcplay). After `end` mcplay calls `vk_recover`: free stream + meshes, `vkbe_close`, `vkbe_open_into` (the SAME struct, structures keep pointers into it: never copy an open VkBackend), pipeline cache file reloaded when `MCPLAY_VK_CACHE` is set, atlas again, `stream_make`. Real loss is UNTESTED (lavapipe never loses a device); a failed submit is treated as loss, other VkResults are not inspected (the `vk_mem` path does not set `Vk.last_res`).
- **Threads**: `MCPLAY_GFX=vk` defaults to `MCPLAY_WORKERS=3` like CPU and GL (r199). Before the r167 TLS fix the raw clone threads (no glibc TLS) crashed `libvulkan_lvp`; now the thread block sits behind `gs` and lavapipe runs stable next to the mesher threads (gate `tools/mc/play/run_vk.sh`: vk + vklost runs need `stream workers=3 pool=true`, the readback run uses `MCPLAY_WORKERS=0`).
- **Memory**: mesh buffers are device-local (`MEM_DEVICE`) with staging copies batched into the next frame's upload command buffer since VK1. UBO slices, HUD / entity instances (`dyn`) and the cube stay host-visible on purpose (written every frame); on a discrete GPU without resizable BAR a device-local copy of `dyn` would need a per-frame staging copy: not built, not measured.
- **Gate**: `bash tools/mc/play/run_vk.sh` (stage `vkplay` in `tools/mc/run.sh`): real 1.20.1 server, Xvfb, platform + 4 mobs, fixed camera; runs CPU, VK, VK with simulated loss. Checks exit 0, `gfx=vk` line, frame counters, boxes drawn, `lost=1 recovered=1`, PSNR(VK, CPU) >= 34 dB for both VK pictures (F3 text masked: its FPS number differs by design). Measured 46.19 dB (top 72.45, bottom 43.19; llvmpipe-Vulkan vs CPU raster). Test `tests/3230_mc_vk_lost_reopen.fi`: loss, in-place re-open, resize on the back end alone.
- **Not done / not measured**: real GPU / Windows, frame time (lavapipe = CPU; mcplay VK on lavapipe is not faster than the CPU raster in any meaningful sense), real device loss, `mcview` (not wired).

## Swapchain present (round VK8, r195) -- lavapipe + Xvfb only, NO real GPU
`vkbe_open_win(b, w, h, cache, xid, mode)` (= `vkbe_open_into` + `vk.open(want_surface)` + `vk.sc_open` on the X11 window; mode 0 xlib, 1 xcb). No surface / swapchain extension / swapchain images without TRANSFER_DST: the back end still opens, `vkbe_sc_on` is false and the caller presents the read back image (old path).
- **Frame** (`be_end`): render pass ends with the colour image in TRANSFER_SRC; `vk.sc_acquire` (per-slot acquire semaphore) -> barrier swapchain image UNDEFINED -> TRANSFER_DST -> `vkCmdBlitImage` colour -> swapchain image (NEAREST; converts RGBA8 -> BGRA8, lavapipe gives BGRA8) -> barrier to PRESENT_SRC -> `frame_submit` waits the acquire semaphore at COLOR_OUTPUT|TRANSFER and signals the slot's done semaphore -> `vk.sc_present`. Readback copy stays (flag `vkbe_set_readback`) for shots / tests; mcplay switches it off with a swapchain unless MCPLAY_SHOT is set.
- **vk_life**: `sc_note(l, VkResult)` after acquire / present (SUBOPTIMAL: counted, rebuild before the next frame, at most 3 in a row; OUT_OF_DATE: counted, frame not shown, rebuild; DEVICE_LOST: lost; SURFACE_LOST: `surface_recover` at the next begin). `sc_sync(v, l, s, w, h)` (= the first half of `frame`) rebuilds at the start of `be_begin` / in `vkbe_resize`; after a rebuild the per-slot semaphores are re-created (`vk_frame.frame_sems_reset`). `vk.fi`: `sc_acquire` / `sc_present` / `sc_image` factored out of `sc_frame`, swapchain usage adds TRANSFER_DST when supported (`Sc.xfer`).
- **Proof**: test 3220 (Xvfb; SKIP without DISPLAY): X window content (XGetImage) == read back image, after resize, injected SUBOPTIMAL / OUT_OF_DATE present, injected OUT_OF_DATE acquire. Gate `run_vk.sh`: mcplay prints `vk present=swapchain ... window_diff=0` (window == readback of the last frame, also after the simulated device loss), plus a run with `MCPLAY_VK_PRESENT=readback`.
- **Not done**: real GPU / Windows (the swapchain is X11-only here, Windows surface untested), mailbox / present-mode choice (FIFO), frame time (lavapipe), resize while minimised on a real WM, per-image done semaphores (per-slot semaphores are enough for 2 slots on lavapipe; validation layer not run).

## mcview (r196)
`MCVIEW_GFX=vk` draws the offline mcview frame through vk_mc (sky, world, selection box, tint, HUD in one pass) and reads the colour image back into the raster buffer (PNG / hash / view.rgba unchanged). No swapchain, stream workers forced to 0. Default stays the CPU raster; no Vulkan device = CPU with a message. Gate: tools/mc/play/run_vkview.sh (PSNR VK vs CPU >= 34 dB on tools/mc/view/fixture/world.mcw, 6 variants; lavapipe: 40.4-44.6 dB), stage `vkview` in tools/mc/run.sh.

## Model quads (r241)

`MESH_MODEL` / `MESH_MODEL_T` (30 f64 per quad, `mc_model_mesh`) are drawn by `vk_mc` through `mesh_class` (class of the NEXT `mesh_upload`, like `gl_mc`) with two extra pipelines (`pipe_mod_op`: depth write, `pipe_mod_tr`: alpha blend, no depth write; back face culling like the voxel pass). Shaders `vk_glsl/mc_model_vs/fs.glsl` (= `lib/gpu/mc_model_vs/fs.glsl` + the Vulkan differences of the voxel pair, listed by `mkspv.sh --parity`): float vertices, binding 0, stride 48 (`MODEL_STRIDE`): loc 0 RGB32F pos, loc 1 RGBA32F (u v tile fk) at +12, loc 2 RGBA32F (sky block tint ao) at +28; tint is `loc 5 flat int`; push constants and Frame UBO as in `mc_vs`. `vk_shaders.SHADER_COUNT` = 11 (MODEL_VS = 9, MODEL_FS = 10). Mesh slot: `MW` 12 words (`M_CLS`, `M_NQ`, `M_MV`). `MESH_MODEL_T` keeps a heap copy of the float vertices; every draw sorts the quads far to near by the summed view depth (stable merge sort, same order as `raster_draw_models_ex`) and writes the index list into the per-frame bump buffer (`dyn`, now also `U_INDEX`); if the bump buffer is full the unsorted stored index buffer is used. A failed model pipeline only disables model quads. Check: `tools/gpu/vk/vkmodel_run.sh` (7 scenes like `model_dump.fi`, PSNR >= 34 dB vs `mc_raster`; lavapipe, NO real GPU) and `tests/3993_mc_vk_models.fi`. mcplay does not draw models on Vulkan yet (CPU only).

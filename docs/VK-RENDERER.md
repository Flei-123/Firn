# Vulkan renderer for the voxel client (round VK1, 2026-10-14, roadmap r177) -- DONE on lavapipe

Everything measured/tested here ran on **lavapipe (CPU Vulkan 1.x), no real GPU, no validation layer** (none installed). Nothing is claimed about GPU speed.

## What exists (all new files, `vk.fi` untouched)
| File | Content | State |
|---|---|---|
| `lib/gpu/vk_mem.fi` | extension function table (63 entries, via `vkGetInstanceProcAddr`, 10 int args through `dynlib.callx`), limits, **suballocating allocator** (64 MB blocks, first-fit free list, coalescing, buffers and images in separate blocks, host-visible blocks mapped once), `buf_make/img_make` | **tested** (`tests/3080_vk_mem.fi`: 200 buffers = 1 `vkAllocateMemory`, free + coalesce, 60 MB refit, staging map, image+array view, no leak) |
| `lib/gpu/vk_desc.fi` | render pass colour+D32 depth, depth image, framebuffer, descriptor set layout (b0 UBO, b1 atlas `sampler2DArray`), pool + sets, sampler, pipeline layout + 48 byte push range, voxel pipeline (uvec2 vertex, depth LESS, optional blend/no depth write) | written, runs (scenes in the gate) |
| `lib/gpu/vk_frame.fi` | frames in flight (command buffer, fence, acquire/done semaphores per slot, per-slot upload command buffer), barriers (image layout, memory), immediate submit | written, compiles, atlas upload path ran (submit r=0) |
| `lib/mc/vk_mc.fi` | `mc_backend.Backend` `KIND_VK`: atlas texture array (staged copy + transitions), mesh upload = staging -> device buffer batched into the next frame's upload cmd, deferred release by frame serial, 2 frames in flight, read back in `present` | written, runs (gate vkmc_run.sh) |
| `lib/gpu/vkmc/mc.vert/.frag(+.spv, build.sh)` | Vulkan GLSL 450 twin of `mc_vs/mc_fs.glsl` (y flip + z 0..1 in the vertex shader, push constants org/trans, fade, light; UBO rv/fog/tiles) | `glslangValidator` + `spirv-val` clean |
| `lib/mc/mc_backend.fi` | `KIND_VK = 3` | done |

Interface contract for VK2 (shader swap): location 0 `uvec2`; set 0 binding 0 UBO (112 B: rv, fog, fogc, tiles), binding 1 `sampler2DArray`; push constant 48 B (VERTEX|FRAGMENT).

## Status (VK1 done, 2026-10-14) -- lavapipe only, NO real GPU, Khronos validation layer (chromium copy) used for debugging
The first-draw crash was NOT in the draw: every Vulkan fence was created SIGNALED and then submitted while signaled (VUID-vkQueueSubmit-fence-00063). lavapipe (Mesa 22.3.6) segfaults in its queue thread afterwards. Fix: `mk_fence(x, false)` for frame + immediate fences (slots start without pending state, so nothing waits on them). Second fix: `stencilStoreOp` was 2 (invalid), now DONT_CARE = 1. The negative-height viewport (y = h, height = -h) now flips y (shader keeps the GL matrix).
Validation recipe: layer manifest in a temp dir with `library_path=/usr/lib/chromium/libVkLayer_khronos_validation.so`, run with `VK_LAYER_PATH=<dir> VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation` (no apt install needed). Open note: it prints `swizzle b/a = Unhandled VkComponentSwizzle` for the colour attachment views although the create info is all zero (IDENTITY) -- looks like a layer bug, not ours.

## Tests / gate
- `tests/3081_mc_vk_smoke.fi`: quad through the Backend table, coverage + clear corner (SKIP without Vulkan).
- `bash tools/gpu/vk/vkmc_run.sh` (also called by `tools/mc/vk/run.sh`): `tools/gpu/vk/vkmc_dump.fi` = the GL scenes of `tools/gpu/dump.fi` through `vk_mc` vs `mc_raster` (triangle, quad, quad_angled, cutout, fog_flat, chunk, chunk_fog, chunk_inside, chunk_far, water_glass(+fog), fade_a/b, lit_day/night/night_fog; entity boxes excluded, VK4).
- PSNR (VK / GL): chunk 33.65 / 36.16, chunk_fog 34.11 / 36.55, chunk_inside 33.03 / 41.20, chunk_far 33.65 / 36.16, water_glass 36.78 / 40.57; the others equal or better than the thresholds. Coverage mismatch 41/47 px (GL 17/11). Floors in vkmc_dump.fi are VK-specific (3300 / 3400 / 3200 / 3300 / 3600) until the gap is found (r184).
- Time 640x360 5x5 sections, llvmpipe-Vulkan with readback vs CPU raster: ~21 ms vs ~52 ms per frame (not a GPU number).

## Not done
Sky/HUD/entities (VK4 r180), swap to the VK2 shaders (`lib/gpu/spv/vk_mc.*`: 4 atlas pages, texel buffers, Frame UBO 320 B, trans/fade in the UBO), swapchain present (VK3 `vk_life`), pipeline cache wiring (`v.cache`), device lost, real GPU, why chunk_inside is 8 dB below GL (suspect: texture sample / UV edge, big_px 337 vs 93).

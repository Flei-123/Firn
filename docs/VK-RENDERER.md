# Vulkan renderer for the voxel client (round VK1, 2026-10-14, roadmap r177) -- PARTIAL

Everything measured/tested here ran on **lavapipe (CPU Vulkan 1.x), no real GPU, no validation layer** (none installed). Nothing is claimed about GPU speed.

## What exists (all new files, `vk.fi` untouched)
| File | Content | State |
|---|---|---|
| `lib/gpu/vk_mem.fi` | extension function table (63 entries, via `vkGetInstanceProcAddr`, 10 int args through `dynlib.callx`), limits, **suballocating allocator** (64 MB blocks, first-fit free list, coalescing, buffers and images in separate blocks, host-visible blocks mapped once), `buf_make/img_make` | **tested** (`tests/3080_vk_mem.fi`: 200 buffers = 1 `vkAllocateMemory`, free + coalesce, 60 MB refit, staging map, image+array view, no leak) |
| `lib/gpu/vk_desc.fi` | render pass colour+D32 depth, depth image, framebuffer, descriptor set layout (b0 UBO, b1 atlas `sampler2DArray`), pool + sets, sampler, pipeline layout + 48 byte push range, voxel pipeline (uvec2 vertex, depth LESS, optional blend/no depth write) | written, compiles, **not verified** |
| `lib/gpu/vk_frame.fi` | frames in flight (command buffer, fence, acquire/done semaphores per slot, per-slot upload command buffer), barriers (image layout, memory), immediate submit | written, compiles, atlas upload path ran (submit r=0) |
| `lib/mc/vk_mc.fi` | `mc_backend.Backend` `KIND_VK`: atlas texture array (staged copy + transitions), mesh upload = staging -> device buffer batched into the next frame's upload cmd, deferred release by frame serial, 2 frames in flight, read back in `present` | written, compiles, **crashes on first draw (see below)** |
| `lib/gpu/vkmc/mc.vert/.frag(+.spv, build.sh)` | Vulkan GLSL 450 twin of `mc_vs/mc_fs.glsl` (y flip + z 0..1 in the vertex shader, push constants org/trans, fade, light; UBO rv/fog/tiles) | `glslangValidator` + `spirv-val` clean |
| `lib/mc/mc_backend.fi` | `KIND_VK = 3` | done |

Interface contract for VK2 (shader swap): location 0 `uvec2`; set 0 binding 0 UBO (112 B: rv, fog, fogc, tiles), binding 1 `sampler2DArray`; push constant 48 B (VERTEX|FRAGMENT).

## Open bug (blocks r177)
`tools/gpu/vk/vkmc_smoke.fi`: open + atlas upload + mesh upload + begin work (all VkResult 0); the first frame segfaults inside `libvulkan_lvp.so` (queue thread) and the read back is all zero. Bisect: removing `vkCmdPushConstants` or `vkCmdBindIndexBuffer`+draw avoids the crash (read back still zero, so a second defect hides behind it: clear/copy never lands). Found + fixed one scratch-clobber bug in `frames_new` (command buffer handles lived in the call scratch). Next: print every VkResult in `be_end`, test the frame without upload cmd, check `PUSH_BYTES` range vs the shader block (both blocks must be byte identical), then the 8 GL scenes (`tools/gpu/dump.fi` minus the GL parts) for PSNR >= 34 dB.

## Not done
PSNR vs `mc_raster`, ms/frame, sky/HUD/entities (VK4 r180), swapchain/present path (VK3), pipeline cache (VK3), device lost (VK3), Windows.

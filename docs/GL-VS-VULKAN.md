# GL 3.3 vs Vulkan for the Minecraft renderer (decision paper, 2026-10-12)

Status of the facts: everything "measured" is Mesa **llvmpipe / lavapipe** (CPU renderers) on a busy server. **No real GPU was ever used.**
Code: `lib/gpu/gl_*.fi` (GL 3.3 core, proven on llvmpipe, EGL/GLX/WGL) and `lib/gpu/vk.fi` (Vulkan 1.0 probe, lavapipe, X11 swapchain). See `docs/GPU.md`.

## 1. What exists today
| | OpenGL 3.3 core | Vulkan 1.0 (probe) |
|---|---|---|
| Loader | EGL / GLX / WGL via `std.dynlib` | `libvulkan.so.1` / `vulkan-1.dll` via `std.dynlib` (`FIRN_VK_LIB` override) |
| Voxel back end (`mc_backend.Backend`) | **yes** (`gl_mc.fi`, 8 scenes vs `mc_raster` PSNR >= 34 dB) | **no** |
| Building blocks | UBO, instancing, multi indirect, mips/aniso/array, FBO MRT/MSAA, timer queries, culling | offscreen target, pipeline from SPIR-V, vertex buffer, X11 swapchain (xlib + xcb), resize |
| Window | Firn window + EGL/GLX/WGL surface, swap interval | Firn window + `VK_KHR_xlib/xcb_surface`, FIFO |
| Windows | wgl twin, green under Wine | `vkplat.windows.fi`, **compiles only** |
| Android | not wired (GLES would be needed) | not wired (`VK_KHR_android_surface` missing) |
| Shaders | GLSL text inside the binary, the driver compiles it | SPIR-V binaries (`glslangValidator`, committed `.spv`), no GLSL compiler in the driver needed |

Measured with lavapipe (320x180, one triangle): Vulkan flat vs `mc_raster` 99 dB / byte equal, gradient vs float reference 88 dB, swapchain window read back equal to the offscreen frame, also after a resize. Offscreen draw + read back ~0.45 ms (CPU renderer: **not a GPU figure**).

## 2. Effort (estimates, not measured)
* **GL**: the voxel back end exists. Remaining: sky/HUD/entities in the back end, real-GPU driver quirks (core profile on NVIDIA/AMD/Intel). Android needs GLES 3.0 (a second shader dialect, `#version 300 es`). Order of magnitude: days.
* **Vulkan**: the probe covers instance..swapchain (about 1500 lines). A renderer additionally needs: depth buffer, descriptor sets (atlas texture array, UBOs), push constants, a memory allocator (suballocation from big blocks; drivers limit the number of allocations), frames in flight (fences, semaphores), barriers/layout transitions for every texture upload and MSAA resolve, pipeline cache, swapchain recreation on resize/minimise/suboptimal, shader variants as SPIR-V, device-lost handling. Order of magnitude: weeks. Validation layers were not available here, so correctness beyond lavapipe is unchecked.

## 3. Portability
* **Linux**: GL via Mesa runs almost everywhere; Vulkan needs a Vulkan driver (Mesa lavapipe/anv/radv, NVIDIA proprietary). Old GPUs often have GL 3.3 but no Vulkan.
* **Windows**: both work on current drivers; wgl is universal, Vulkan ships with GPU drivers. Both only built/Wine-tested here, **real Windows untested for both**.
* **Android**: desktop GL 3.3 does not exist there. GLES 3.0 is the safe baseline, Vulkan is available on newer devices. The GL back end as built does not port; the core of `vk.fi` does (only the surface differs).
* **macOS/iOS/Web**: neither, not planned (MoltenVK / WebGL2 / WebGPU would be separate back ends).

## 4. Cost and benefit for the Minecraft renderer
Workload: static chunk meshes (51k triangles in the test), few draws per section, one atlas, simple shading, fog. With indirect/instancing (`gl_ext`, tested) it is not draw-call bound, so Vulkan's main win (cheap draws, multithreaded recording) matters little now. Possible wins: explicit uploads (no hidden stalls while chunks stream in), pipeline cache (no shader-compile hitch), better pacing on mobile. Costs: weeks of boilerplate and a second code path that must stay equal to the `mc_raster` reference.

## 5. Recommendation
1. **Stay on GL 3.3 core for Linux/Windows desktop.** Done, tested against the CPU reference, not draw-call bound.
2. **Android:** decide when an Android renderer becomes a goal. Preferred: a GLES 3.0 twin of `gl_mc` first (smaller); Vulkan only if real-device profiling shows GLES driver stalls.
3. **Keep `vk.fi` as probe/seed**, do not grow it into a renderer now. If wanted later: depth + texture array + UBO in `Pipe`, then an `mc_backend.Backend` table, gated by the same PSNR comparison with `mc_raster`.

## 6. Only measurable on a real GPU (all numbers above are lavapipe)
* ms/frame of the 51k-triangle scene, GL vs Vulkan, vsync off, on NVIDIA / AMD / Intel.
* CPU time per frame (driver overhead): the whole argument for Vulkan.
* Chunk-upload hitches (`glBufferData` vs staging copy), shader-compile hitch, memory use.
* Present behaviour / frame pacing (lavapipe + Xvfb has no vblank; FIFO was only checked for "works").
* Driver quirks of the GL core path (llvmpipe is permissive); validation cleanliness of `vk.fi`.
* Android: GLES 3.0 vs Vulkan on a real phone.
* Real Windows (wgl and `vulkan-1.dll`), real window manager.

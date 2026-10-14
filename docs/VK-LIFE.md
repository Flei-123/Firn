# Vulkan lifecycle (gpu.vk_life, roadmap r179)

All numbers: Mesa **lavapipe** (CPU) + Xvfb. **No real GPU** was used; real-hardware behaviour (pipeline cache contents, SUBOPTIMAL/OUT_OF_DATE timing, real device loss, Android) is UNTESTED.
Code: `lib/gpu/vk_life.fi` on top of `lib/gpu/vk.fi`. Gate: `bash tools/mc/vk/run.sh`. Tests 3100-3102, `tools/mc/vk/lifeprobe.fi`.

## vk.fi changes (small)
`Vk.cache` (pipeline_new passes it to vkCreateGraphicsPipelines; **a new pipeline function must use `v.cache` too**), `Vk.last_res` (last non-zero VkResult of submit / fence wait / acquire / present, reset by vk_life before each frame), fault injection `vk.inject(v, INJ_SUBMIT|ACQUIRE|PRESENT, result)` (next call at that site fails; acquire/submit are skipped, present really presents), `vk.release(v)`, helpers exported for vk_life (`call sa good r32 r64 w32 w64 cstr`).

## Pipeline cache
- `cache_open(v, l, path)`: file = 16 octet wrapper ("FNPC", version, length, CRC-32) + driver data. Checks, in order: wrapper (CACHE_BAD_FILE), Vulkan header v1 vs this device (size, version, vendor, device id, pipelineCacheUUID -> CACHE_BAD_HEADER), driver accepts. Any failure = empty cache, never an error. Sets `v.cache`.
- `cache_save` writes atomically (`fs.write_atomic`, creates the directory). `cache_close`.
- lavapipe only stores the 32 octet header (measured: 32 bytes after one pipeline) - the round trip is proven, the *speed-up* is not measurable here.

## Swapchain
`vk_life.frame(v, l, sc, pipe, vb, n, clear, w, h)` (w,h = current window size): size change -> rebuild; `OUT_OF_DATE` (acquire or present) -> rebuild + retry once; `SUBOPTIMAL` -> frame counts, rebuild before the next one (at most 3 times in a row, then ignored); 0x0 -> `LF_MINIMIZED` (nothing on the GPU), restore rebuilds. Rebuild = idle, new semaphores (a failed acquire/present may leave them signalled), `sc_resize`, `l.sc_gen += 1`, `l.on_rebuild(l.ud, w, h)`. **Depth / MSAA targets of vk_mc must be re-created from `on_rebuild` (or when `sc_gen` changes).** `SURFACE_LOST` -> `LF_SURFACE_LOST` -> `surface_recover`.

## Device lost
`VK_ERROR_DEVICE_LOST` from submit/fence/acquire/present -> `LF_DEVICE_LOST`, `l.lost` sticks (no GPU calls any more), `l.on_lost(ud)` once. Caller frees its objects, `recover_device(v, l, want_surface, allow_cpu, cache_path)` closes + re-opens instance/device, reloads the cache file, bumps `l.epoch` / `sc_gen`; caller re-creates Target/Pipe/Buf/Sc. Offscreen: `draw_checked`. Loss is only SIMULATED (injection): lavapipe never loses a device.

## Harness
`tools/mc/vk/run.sh` runs the above + the vk.fi regression (`tools/gpu/vk/run.sh`: flat/gradient triangle vs mc_raster, PSNR >= 34 dB, ~0.2-0.3 ms/frame offscreen on lavapipe) + Windows compile. **Open (r179 rest):** the 9 voxel scenes of `tools/gpu/dump.fi` (triangle, quad, quad_angled, cutout, fog_flat, chunk, chunk_fog, chunk_inside, chunk_far) through `vk_mc.vkbe_*` (names mirror gl_mc) with PSNR >= 34 dB and ms/frame; plan: derive `scenes.fi` from dump.fi by whitelisting top-level functions and renaming `gl_mc.glbe_*` -> `vk_mc.vkbe_*` (vk_mc was not on main when this was written).

## Windows
`vkplat.windows.fi` + vk_life compile (`--target=x86_64-windows`: lifeprobe, tests 3100-3102); test 3100 runs under Wine, 3101/3102 SKIP there (no vulkan-1.dll). Real Windows: untested.

## Android (plan, not built - r182)
- Surface: `VK_KHR_android_surface` (`vkCreateAndroidSurfaceKHR`, `VkAndroidSurfaceCreateInfoKHR{sType=1000008000, window=ANativeWindow*}`), added as case 2 of `vkplat` (`surface_ext_name`, `surface_create`); loader `libvulkan.so`.
- Lifecycle maps onto vk_life: `APP_CMD_TERM_WINDOW` = surface lost (destroy swapchain + surface, keep device), `INIT_WINDOW` = `surface_recover`, rotation = SUBOPTIMAL / pre-rotation transform (`currentTransform` is already passed to the swapchain), onPause may drop the device -> `recover_device`.
- Pipeline cache path = app-private dir; header validation matters most there (driver updates change the UUID).
- Open: ANativeWindow from the Firn android window layer, `VK_KHR_swapchain` surface formats (RGBA8 common), no validation layers.

## vk_mc in mcplay (r193)
`vk_mc` does not use `vk_life.frame` (no swapchain yet); it keeps `life` for the pipeline cache only and detects loss itself (failed frame submit or `vkbe_inject_lost`), see docs/VK-RENDERER.md "mcplay with the Vulkan back end". Recovery = close + `vkbe_open_into` + re-upload (mcplay `vk_recover`), simulated only.

# GPU back end (Linux): OpenGL 3.3 core through std.dynlib

Status 2026-10-11 (workers mc-a + mc-f). Built on `docs/FFI.md` (dlopen from a static Firn image) and
`docs/MC-RENDER.md` (CPU rasterizer = reference + fallback).

## What exists (tested = ran on Mesa llvmpipe, GL 4.5 core context requested as 3.3 core)
| File | Content | State |
|---|---|---|
| `lib/gpu/glctx.fi` | EGL context: Mesa surfaceless platform first, then `eglGetDisplay(0)`; pbuffer config RGBA8 + D24; `OpenGL 3.3 core` context; `ctx_proc` (eglGetProcAddress, libGL fallback); `ctx_open_x11` (Xlib via dynlib + EGL window surface, `ctx_swap`) | headless path **tested** (no DISPLAY and on Xvfb); `ctx_open_x11` **compiled only** |
| `lib/gpu/gl33.fi` | 71 GL 3.3 functions loaded by name; wrappers: state, VBO/IBO/VAO (`gl_attrib_i` for the 8-byte mc_mesh vertex), RGBA8 textures (NEAREST/MIPMAP), FBO (colour tex + D24), shader compile/link with log, uniforms (1i/1f/2f/3f/4f/mat4), `glDrawElements`, readback (bottom-up and top-down) | context + clear + `glReadPixels` **tested** (`tools/gpu/run.sh`); the rest **compiled only** |
| `lib/gpu/mc_vs.glsl`, `mc_fs.glsl` | voxel shaders: uvec2 vertex, integer shading identical to `mc_raster` (nearest texel with tile wrap, alpha cut-out, AO, face brightness, fog) | **tested** (`tools/gpu/dump.fi`); `u_tiles` is a `vec2` (no `glUniform2i` wrapper) |
| `lib/gpu/gl_mc.fi` | `GlBackend` + `glbe_backend()` = the `mc_backend.Backend` table on GL: FBO, program from the two embedded .glsl, mesh slots (VAO + VBO + IBO), atlas texture, `u_rv` = proj * view rotation (f64 -> f32), `u_origin` = section origin - camera (f64), fog (`glbe_set_fog`), cull, read back top-down in `present` | **tested** Linux + Windows twin under Wine |
| `tools/gpu/dump.fi`, `tools/gpu/run.sh` | 9 scenes through the SAME Backend calls on `mc_raster` and GL, PSNR + coverage thresholds, ms/frame; gate 119 in `test.sh` | **tested** |
| `lib/mc/mc_backend.fi` | `Backend` = table of function values (`upload_atlas, mesh_upload, mesh_free, begin, draw, end, present, read_pixels`) + `backend_cpu(raster)`; NO GPU import (programs stay static) | **tested** through `wr_render_b` |
| `lib/mc/mc_render_world.fi` | new `wr_render_b / wr_clear_b / wr_free_b / wr_drop_chunk_b` take a `Backend`; the old `Raster` functions are wrappers | tests 2753 + 2763 pass |

## Measured: GL vs mc_raster (Mesa llvmpipe, 320 x 180, `tools/gpu/dump.fi`)
GL and `mc_raster` are NOT byte equal (GL: float edge functions + float interpolation; mc_raster: 1/16 pixel snapping, double
interpolation). Differences: (1) a few edge pixels covered by only one of them (cov_mismatch <= 14 of 57600), (2) texel
rounding flips of `floor(u * tile)` on high contrast textures (grass), (3) +-1 in shaded flat colours (AO / fog `floor`).
| scene | PSNR | coverage mismatch | threshold |
|---|---|---|---|
| triangle (AO gradient) | 79.1 dB | 0 | >= 40 |
| quad 2 x 2 repeat / cut-out | 99 dB (identical) | 0 | >= 40 |
| quad at an angle | 42.8 dB | 6 px | >= 40 |
| fog on flat colour | 80.2 dB (max diff 1) | 0 | >= 75 (a fog formula off by 6/256 gives 61.9 and fails) |
| voxel chunk 2 x 2 sections | 37.1 dB | 14 px | >= 34 |
| chunk + fog / inside the field | 37.6 / 42.9 dB | 14 / 6 px | >= 34 |
| chunk 1000000 / -2000000 blocks from the origin | 37.1 dB (= the near-origin scene, exactly) | 14 px | >= 34 |
Rule of thumb: PSNR >= 34 dB and coverage mismatch <= 5 permille = "same picture". A damaged shader (AO base 100 -> 110,
fog 256 -> 250) is detected by these thresholds (counter-checked by hand).
Time per frame, 640 x 360, 5 x 5 sections (51050 triangles, median of 21 interleaved frames, busy 7-10 load server):
CPU raster ~70-79 ms, GL on llvmpipe ~16-17 ms (with read back; the read back itself is ~0.2 ms). llvmpipe is a CPU renderer
that uses all cores, the raster is one thread: the number is NOT a GPU number, only a check that the GL path is not slower.
The same test as a Windows exe under Wine (wgl through the Win64 gate, Wine -> host Mesa) gives IDENTICAL scene statistics.

## Not done
1. Window: X11 stays on the Firn window + `PutImage` of the read back frame (variant b of `docs/FFI.md`); `ctx_open_x11`
   (Xlib alternative, swap without read back) is compiled only. Win32 window with `SwapBuffers` (MC-B) is not wired to `GlBackend`:
   `glbe_open_with(ctx, w, h)` takes any current context.
2. No real GPU run (only llvmpipe): ms/frame on a GPU, GL ES / driver quirks, core profile on NVIDIA/AMD/Intel is untested.
3. Entities / sky / HUD still go through the CPU (`mc_sky`, `mc_hud`) and are not part of the Backend table; the GL back end only
   draws the voxel meshes. Textures use NEAREST (no mipmaps) like the raster; fog is set by `glbe_set_fog`, not through the table.
4. Vulkan: `lib/gpu/vk.fi` exists (round gpu-g, gate 126, below + `docs/GL-VS-VULKAN.md`); it is a probe layer, not a renderer.

## Build notes
* `firnc` of the main checkout (Oct 8) is too old (`link_lib` unknown): build `compiler/` (`cargo build --release`, 23 s, through `/root/jarvis/bin/heavy`).
* Run everything: `bash tools/gpu/run.sh [firnc]` (smoke + GL vs raster + Windows twin under Wine; SKIP when libEGL/Wine/Xvfb are missing).
* Escape check: pointers to locals passed through the `Gl` scratch array need `#[allow_escape]` (`compile`, `gl_uniform_loc`).
* Call GL from the main thread only (docs/FFI.md, TLS).

## Renderer building blocks (2026-10-12, worker firn-gpu-b) -- `tools/gpu/ext/run.sh`, gate 122
ALL numbers/"tested" below are Mesa llvmpipe (GL 4.5 core, requested as 3.3). Real GPU: UNTESTED. gl33.fi got one fix only
(`gl_u1i` panicked for negative values); everything else is new files.
| File | Content | State |
|---|---|---|
| `lib/gpu/gl_ext.fi` | `GlX` (embeds `gl33.Gl`) + probe (version, extension list, limits, flags `has_ubo/ssbo/indirect/multi_indirect/timer/msaa/aniso/tex_array`); `glUniform2i/3i/4i` (`u_tiles` may become an ivec2); UBO (`glx_block_index/binding/size`, `glx_ubo_new/update`, std140 helpers); SSBO (>= 4.3 or ARB_shader_storage_buffer_object, else "not available": `glx_ssbo_ok` false, calls return false/0); `glVertexAttribDivisor`, `glDrawElements/ArraysInstanced`; indirect (`glDrawElementsIndirect` >= 4.0, `glMultiDrawElementsIndirect` >= 4.3) | UBO, uniforms, instancing, multi indirect **tested**; SSBO **compiled only** (probe true on llvmpipe, no shader test written) |
| `lib/gpu/gl_prog.fi` | program from memory or files, driver log in `p.log`, failed build keeps the old program, 64-entry uniform cache (FNV-1a name hash), `prog_poll` = hot reload by content hash | build/log/cache **tested**; `prog_poll` **compiled only** (no edit-and-reload test) |
| `lib/gpu/gl_tex.fi` | RGBA8 2D + mips (trilinear), 2D array texture, `tex_array_from_atlas`, filter/wrap/max-level setters, anisotropy (`GL_EXT_texture_filter_anisotropic`, clamped to the driver max), sub-upload, level readback | mips, aniso set/get, array-from-atlas **tested** |
| `lib/gpu/gl_fbo.fi` | `Target`: 1-4 RGBA8 colour textures (MRT), depth none / renderbuffer / **texture**, MSAA (renderbuffers + per-attachment `glBlitFramebuffer` resolve), resize | MRT, depth texture, resize, MSAA 4x **tested** |
| `lib/gpu/gl_query.fi` | `Timer`: ring of 4 frames x 16 passes, `GL_TIMESTAMP` pairs or `GL_TIME_ELAPSED`, results polled with `GL_QUERY_RESULT_AVAILABLE` (never waits; unfinished sets are dropped and counted), `pass_ms/pass_avg_ms/pass_print` | timestamp mode **tested** (values > 0); `MODE_ELAPSED` compiled only |
| `lib/gpu/cull.fi` | camera-relative f64 frustum culling (mc_math planes + frustum-corner bounding box), sphere, software Hi-Z occlusion (box occluders, shrunk coverage at silhouette edges only, max pyramid) | **tested** CPU only |

### Atlas, mipmaps, bleeding
Two bleed sources, measured in `tools/gpu/ext/gl.fi`: (1) the mip chain of a tile atlas mixes tiles below level log2(tile) (test: 2x2
atlas, lowest level = grey 128/128/128); cure `GL_TEXTURE_MAX_LEVEL = atlas_max_level(tile)`. (2) bilinear taps across a tile border at every
level; cure = `tex_array_from_atlas` (one layer per tile; test: lowest level of every layer keeps its own colour). Gutters are not implemented.

### Measured (llvmpipe, 128 x 128 target, 256 quads, 6 frames)
Instanced draw == 256 single draws: image byte equal. Wall time incl. `glFinish`: instanced ~1.9 ms vs single ~1.6 ms (llvmpipe is a CPU
renderer, the per-draw driver cost that instancing saves is tiny there; NOT a GPU result). Timer queries: pass 0 ~0.09 ms, pass 1 ~0.05 ms (> 0).
Cull (tools/gpu/ext/cull.fi): 72000 random boxes/spheres, 0 visible ones rejected; plain plane test keeps 120 outside boxes, refined test 90;
verdicts identical 1,000,000 / -3,000,000 blocks from the origin (0 of 72000 differ); occlusion: 45000 boxes, 16246 occluded, 0 of them have a
visible fragment in the reference raster (grown coverage), 0 differences far from the origin. Occluders are axis-aligned boxes only.

### Not done
Vulkan probe (`lib/gpu/vk.fi`; `libvulkan.so.1` + `lvp_icd` are installed here, so `vkCreateInstance`/`vkEnumeratePhysicalDevices` via dynlib is feasible:
needs ~40 structs/handles by pointer, no GL context; real renderer would need SPIR-V compile (glslang) and a swapchain per window system).
GL image hash (PSNR) for the new paths is not added: the byte-equality checks above are on GL itself. SSBO shader test, `prog_poll` edit test,
`MODE_ELAPSED` test, real GPU, indirect `baseInstance` (needs GL 4.2 `ARB_shader_draw_parameters` semantics) untested.

## GL window on X11 (round gpu-a, 2026-10-12) -- `lib/gpu/glwindow.fi`, `glshow.fi`, `tools/gpu/win/run.sh` (gate 125)
Decision: **(a) the Firn window of `lib/window` + a GL context on its XID**. The window keeps events, input, DPI,
clipboard, drops and the EWMH full screen request; only the context is new (`glctx.ctx_attach_x11` = EGL window surface via a
second Xlib connection that exists only for Mesa's X11 platform; `glctx.ctx_attach_glx` = libGL GLX fallback,
`glXCreateContextAttribsARB` 3.3 core, FBConfig chosen by the window's visual id). **An XID of another connection works on
Mesa (EGL and GLX), proven.** (b) `ctx_open_x11` (its own Xlib window) would need XNextEvent -> window.EV_* translation,
DPI, input and fullscreen again: more code, less function. Not run in the gate (still "compiled only").
API (`import gpu.glwindow`): `open(&g,w,h,title,opts)`, `make_current`, `swap`, `swap_interval(n)` (0/1/-1; clamped to the
driver's range, returns the value in force or -99), `pump(ms)` (events -> `g.w/g.h/g.resized/g.closed`), `resize(w,h,ms)`
(ConfigureWindow + wait for EV_CONFIGURE + glViewport), `set_fullscreen/is_fullscreen` (EWMH `_NET_WM_STATE_FULLSCREEN`
when a WM runs, else the window is sized over the screen), `dpi`, `surface_size`, `take_ctx` (hand the context to a
`GlBackend`), `close`. `glshow` draws a texture (`glbe_fbo_texture`) or a CPU image (BGRX/RGBA, top row first) over the
default framebuffer: the frame path **without read back** = `show_pixels` + `glwindow.swap` instead of `window.image`.
`gl_mc.glbe_resize` makes the FBO again after a resize. Window additions: `r_native/r_native_visual/r_set_size`
(x11.fi, win32.fi), `r_fullscreen/r_is_fullscreen` now real on X11.
Measured (Xvfb 800x600 -dpi 120, Mesa llvmpipe, busy host): open 86-109 ms; clear+swap per frame: EGL ~1000 us (min swap
interval of Mesa's EGL X11 swrast is 1: `swap_interval(0)` reports 1), GLX ~480-500 us (interval 0 accepted); no real vblank
under Xvfb, so VSync on/off cannot be told apart here. EGL `-1` (adaptive) falls back to 1; GLX only with
GLX_EXT_swap_control_tear. dpi = 120 (from Xvfb -dpi), surface size = window size after resize (EGL_WIDTH/HEIGHT).
GLX cannot ask the surface size (`glXQueryDrawable` crashes on a plain X window with Mesa's dispatch): use `g.w/g.h`.
**Only Mesa/Xvfb proven. Real GPU, a real window manager (EWMH path) and real Windows are UNTESTED.**
Module name is `glwindow` (not `glwin`: `lib/plat/glwin.fi` = fUi's GlApi table for Windows/Android has that name and
module names are global). Windows twin `lib/gpu/glwindow.windows.fi` (same names; window = Firn window of lib/window, context =
`window.gpu_gl` WGL 3.3 core, depth 24/stencil 8, `g.ctx` is a borrowed `glctx` context, swap = `window.gpu_swap`,
`swap_interval` has two states, `api_name` = "wgl"): `winprobe.exe wgl` under Wine + Xvfb + Mesa is green (open, clear + read
back, resize via `r_set_size`, fullscreen on/off), part of gate 125 (SKIP without Wine/mingw ld). Real Windows UNTESTED.
Window framebuffer vs mc_raster (`tools/gpu/win/shot.fi`, EGL, GLX and wgl/Wine, same numbers): (A) CPU image through `glshow.show_pixels` into
the window and read back = byte equal (RGB); (B) GL back end FBO texture through `glshow.show_draw` into the window = mc_raster with PSNR
79.1 / 99.0 / 42.8 dB (triangle / quad / quad_angled; threshold 34).

## Scene layer groundwork (round gpu-h, 2026-10-12) -- PARTIAL
`lib/gpu/gl_mc.fi` keeps the `mc_backend.Backend` table and every existing `glbe_*` name unchanged (a WebGL2 twin only has to
match the table). Additions (all additive; `tools/gpu/dump.fi` still gives the same 9 scene lines and identical world checksums):
* `glbe_x(gb)` -> the `gl_ext.GlX` table loaded next to the base table (instancing, queries, MSAA blit); `glbe_width/height`.
* `glbe_bind_draw(gb)` = bind the draw target + full viewport; `glbe_rebind(gb)` = put the voxel pass state back after a pass
  with its own program (sky, entities, HUD). In `MODE_MULTI` the collected draws stay queued until `b.end`.
* `glbe_camera_rv(gb, out, &cx, &cy, &cz)` = the frame's `u_rv` (f32, 16 values) + camera position (f64) for extra passes.
* MSAA: `glbe_set_msaa(gb, samples)` (0 = off; returns samples in force, 0 when the driver has none) builds a `gl_fbo.Target`
  (renderbuffers) that all passes draw into; `glbe_resolve(gb)` blits it into the plain FBO; `present` and `glbe_fbo_texture`
  resolve by themselves; `glbe_resize` rebuilds both. NOT yet run in an image test (compiled + default path unchanged only).
* `mc_hud.hud_record(&buf)` (lib/mc/mc_hud.fi): with a record buffer set `hud_rect` / `hud_invert` append the CLIPPED rectangle as
  6 u32 (x0 y0 x1 y1 colour mode) instead of drawing, so a GPU overlay reuses all layout code (glyphs, hotbar, crosshair,
  debug block). `hud_record(0)` = draw again. tests/2762 unchanged and green.
* Shader sources written, NOT yet wired/compiled by a program: `gl_sky_{vs,fs}.glsl` (mc_sky background per pixel, integer lerps),
  `gl_tint_fs.glsl` (light tint, blend DST_COLOR/ZERO at depth 1.0 with GL_GREATER), `gl_ent_{vs,fs}.glsl` (instanced entity
  boxes, shading = mc_raster with ao 3, fog), `gl_hud_{vs,fs}.glsl` (instanced rectangles, blend or invert).
Still to do (roadmap): `lib/gpu/gl_scene.fi` (sky/tint/entity/HUD passes, timer ring per pass, cull.fi wrapper, glshow window
present), the PSNR test for sky/HUD/entity scenes, gate 126. No real GPU here (`/dev/dri` missing): every time is llvmpipe.

## Vulkan probe (round gpu-g, 2026-10-12) -- `lib/gpu/vk.fi`, `vkplat.fi`, `tools/gpu/vk/run.sh`, gate 126
Decision paper: `docs/GL-VS-VULKAN.md`. ALL numbers are Mesa lavapipe (CPU, Vulkan 1.3 device, requested 1.0). Real GPU: UNTESTED.
* `gpu.vk`: loader (dlopen `libvulkan.so.1`, override `FIRN_VK_LIB`), instance (+ `VK_KHR_surface/xlib/xcb` when present), best device (discrete > integrated > virtual > other > CPU; CPU only with `allow_cpu`), queue, command pool/buffer, fence; offscreen `Target` (RGBA8 image, render pass, framebuffer, mapped read back buffer), `Pipe` from SPIR-V (dynamic viewport/scissor, vertex = vec2 + vec3), host visible `Buf`, `draw`, swapchain `Sc` (`sc_open` by XID, `sc_frame`, `sc_resize`, `window_pixels` = XGetImage).
* `gpu.vkplat` (Linux) / `vkplat.windows.fi` (vulkan-1.dll + `VK_KHR_win32_surface`; BUILD ONLY, not run): the platform part. Without a loader `open` returns false with `STEP_NO_LOADER` (tested with `FIRN_VK_LIB`).
* Shaders: `lib/gpu/spv/tri.vert|frag` (GLSL 450) -> committed `.spv` (`build.sh` needs glslangValidator + spirv-val; the tests do not), embedded with `__include_str`.
* Struct layouts are byte offsets of the Vulkan 1.0 headers (x86_64); wrong offsets would crash lavapipe, so the green run is the check. No validation layer was run.
Measured (lavapipe, 320x180): flat triangle vs `mc_raster` (same vertices projected with the raster's own formulas, colour = the raster's pixel): PSNR 99 dB, 0 coverage mismatch, byte equal;
RGB gradient vs a float CPU reference: 87.95 dB (max diff 1); swapchain (xlib and xcb surface, BGRA8, 3 images, FIFO): window read back with XGetImage equals the offscreen render (99 dB), also after a resize to 400x240 (`sc_resize`);
offscreen draw + read back ~0.4-0.45 ms/frame (CPU renderer, NOT a GPU number).
Not done: depth/textures/descriptors/push constants/uniform buffers, mc_backend table on Vulkan, memory suballocation, validation layers, Android (`VK_KHR_android_surface`), Wayland, real Windows, real GPU, `OUT_OF_DATE` is only handled, not provoked by a test.

## fUi over a GL window (round gpu-f, 2026-10-12, roadmap r422) -- `lib/plat/fuiwingl.fi`, `tools/gpu/fuigl/run.sh` (gate 128)
fUi still paints on the CPU into a canvas; only the LAST step changes. `fuiwin.window_set_present(fx, ud, f)` is a present hook:
`f(ud, rgba_pixels, w, h) -> bool` replaces RGBA->BGRX + `window.image` + `window.present` in `window_step` (`window_clear_present`
undoes it, `window_present_hooked` asks). A hook that returns false is dropped for good and THAT frame is painted on the CPU path, so
a broken GL never leaves the window empty. `fuiwingl.glhost_attach(&gh, &fx)` (after `window_open`) makes a GL 3.3 core context
on the XID of the window lib/window made (`window_window(fx)`; EGL, GLX as fallback), loads `gl33`, inits `glshow`, sets the hook;
it returns false (nothing changed) if GL does not start. Per frame: `glshow.show_pixels(.., bgrx = false)` (one texture updated in
place, one triangle) + `ctx_swap`. `glhost_detach` BEFORE `window_close`; `glhost_set_interval`, `glhost_api`, `glhost_renderer`.
Windows twin `lib/plat/fuiwingl.windows.fi` (WGL through `window.gpu_gl`, borrowed context, swap = `window.gpu_swap`).
```
var gh: fuiwingl.GlHost = fuiwingl.glhost_new()
fuiwin.window_open(&fx, ...)
fuiwingl.glhost_attach(&gh, &fx)        // false: stays on PutImage
... window_step loop ... ; fuiwingl.glhost_detach(&gh, &fx); fuiwin.window_close(&fx)
```
Test `tools/gpu/fuigl/fuigl.fi` (= hello_lowlevel + switch `cpu|gl|glbreak`) under Xvfb + llvmpipe: the X server's own picture (`xwd -root`)
of the GL path equals its canvas AND the CPU path's picture (0 differing pixels, PSNR 99; need >= 34), also after a resize
640x400 -> 500x300 (new canvas, new texture); `glbreak` (hook fails on purpose) falls back to the CPU path with an intact picture;
WGL under Wine: hook in use, canvas identical to the CPU run. Repaint time (forced repaints incl. fUi painting, 1280x800, min of 3,
busy shared host, llvmpipe: software GL does the upload copy on the CPU too): see the `metric` line of the gate log -- cpu ~7.8 ms,
gl ~7.0 ms in the run that was written down; **no real GPU measured**, so no claim about GPU speed-up. Real GPU, real Windows UNTESTED.
Not done: damage regions (the whole canvas is uploaded; `gl_tex_update` could take a sub-rectangle), vsync-paced loop, HiDPI scale of the
texture (the canvas is window size), `fui.gpu` (painting ON the GPU) for desktop.

### EWMH under openbox (gate 129, `tools/gpu/win/wm_run.sh`, `wmprobe.fi`)
openbox on Xvfb (`openbox --sm-disable`): (1) `set_fullscreen(true)` -> `_NET_WM_STATE_FULLSCREEN` ClientMessage -> window = screen size
within 20 ms (EV_CONFIGURE), `is_fullscreen` true, and from OUTSIDE `xprop -id` shows `_NET_WM_STATE_FULLSCREEN` and `xwininfo` the
screen size; the GL surface follows (1024x768); off -> 320x200 again, property gone. (2) The WM focuses the new window (`xdotool
getwindowfocus`); `xdotool windowfocus` to a second window and back arrives as EV_FOCUS out=1 / in=1 (needs `window.raw_input(true)`).
Note: the first FocusIn happens while `glwindow.open` maps the window, i.e. BEFORE a program can call `raw_input` -- ask the server
(`getwindowfocus`) for the initial state, not the event. Only openbox tried; other WMs (mutter, kwin, i3) and real displays UNTESTED.


## WebGL2 back end for the browser (round gpu-m, 2026-10-12, roadmap r425) -- `lib/gpu/glweb.fi`, gate 131
Same `mc_backend.Backend` table as `gl_mc` on WebGL2 (`--target=wasm32-browser`, JS host `demos/mcweb/glweb.js`, ES 3.00 shaders `glweb_vs/fs.glsl`),
legacy draw mode only. Headless Chromium on SwiftShader: 7 scenes vs mc_raster, PSNR 46-99 dB (need >= 34). Plan, missing features
(buffer textures, base vertex, mapped buffers, indirect, timer queries, Hi-Z), the `lib/@web/` twin idea and untested items: `docs/WEBGL2.md`.
Real GPU, Firefox, Safari, mobile UNTESTED.

### Scene layer: lib/gpu/gl_scene.fi (round gpu-l, 2026-10-12) -- gate 132 (`tools/gpu/mc/run.sh`)
`gpu.gl_scene` wires the shaders `gl_{sky,tint,ent,hud}_*.glsl`: `scene_open(&gb)`, `scene_frame` (timer ring), `scene_sky(&sc,&cam,&sky)` (after b.begin),
`scene_ent_add/scene_ent_occluder` + `scene_ent_flush(&sc,&cam)` (ONE instanced draw; frustum + Hi-Z culling via gpu.cull, occluders = yaw-0 boxes),
`scene_tint` (depth < 1 pixels, blend DST_COLOR/ZERO), `scene_hud(&sc,&rec)` (mc_hud.hud_record rectangles, runs per blend mode, alpha not written),
`scene_present_window` (resolve + FBO texture -> window, no read back; caller swaps). Timer passes PASS_* (stamp queries, never waits).
gl_mc addition: `glbe_flush` (issue collected MODE_MULTI draws before an extra pass; `glbe_rebind` alone forgets them).
Smoke run `tools/gpu/mc/scene_smoke.fi`. Image tests `tools/gpu/mc/scene.fi` (llvmpipe, 320x180, CPU = mc_raster + mc_sky + mc_hud, entities = 12 mc_raster triangles per box, solid tiles, ao 3): sky 99 dB (byte equal); world+sky+fog+tint+HUD 53.5 dB; HUD only 57.4 dB (max diff 1); entities 56.3 dB, +fog 59.5 dB, +wall 58.9 dB; MSAA x4 vs CPU 43.7 dB (354 edge pixels differ from the plain frame); window present (EGL and GLX, FBO texture -> window, read back) 55.6 dB. Cull on/off: byte-identical frames (2 of 6 boxes sent: 1 frustum, 3 occluded). Gate 132 also prints `upload_probe.fi` (see docs/mc/gpu-world.md). Only Mesa llvmpipe / Xvfb; real GPU, real WM and Windows (WGL) untested for this layer.

## Model quads on the GL back end (r236c)

`MESH_MODEL` / `MESH_MODEL_T` (lib/mc/mc_model_mesh.fi quads, 30 f64 each) are drawn by `gl_mc` with their own program (`mc_model_vs/fs.glsl`): float vertices (12 f32), tint (`texel * (t + 1) >> 8`), AO per corner, per quad sky/block light and face shade, the same integer shading and fog as `mc_fs.glsl`. The voxel program is untouched. Model meshes live in private VAO/VBO/IBO (not in the pool pages); each draw submits the collected voxel draws first. `MESH_MODEL_T` is sorted far to near by view depth on the CPU at every draw. Check: `tools/gpu/model_dump.fi` (7 scenes, PSNR >= 34 dB vs mc_raster, measured 59-94 dB, one angled scene 39 dB) and `tests/3992_mc_gl_models.fi`. Vulkan: done (r241), see docs/VK-RENDERER.md.

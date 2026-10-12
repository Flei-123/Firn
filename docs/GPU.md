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
4. Vulkan: only an option (`libvulkan.so.1` loads the same way; no probe written, no plan beyond this line).

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

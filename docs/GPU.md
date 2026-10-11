# GPU back end (Linux): OpenGL 3.3 core through std.dynlib

Status 2026-10-11 (worker mc-a, PARTIAL). Built on `docs/FFI.md` (dlopen from a static Firn image) and
`docs/MC-RENDER.md` (CPU rasterizer = reference + fallback).

## What exists (tested = ran on Mesa llvmpipe, GL 4.5 core context requested as 3.3 core)
| File | Content | State |
|---|---|---|
| `lib/gpu/glctx.fi` | EGL context: Mesa surfaceless platform first, then `eglGetDisplay(0)`; pbuffer config RGBA8 + D24; `OpenGL 3.3 core` context; `ctx_proc` (eglGetProcAddress, libGL fallback); `ctx_open_x11` (Xlib via dynlib + EGL window surface, `ctx_swap`) | headless path **tested** (no DISPLAY and on Xvfb); `ctx_open_x11` **compiled only** |
| `lib/gpu/gl33.fi` | 71 GL 3.3 functions loaded by name; wrappers: state, VBO/IBO/VAO (`gl_attrib_i` for the 8-byte mc_mesh vertex), RGBA8 textures (NEAREST/MIPMAP), FBO (colour tex + D24), shader compile/link with log, uniforms (1i/1f/2f/3f/4f/mat4), `glDrawElements`, readback (bottom-up and top-down) | context + clear + `glReadPixels` **tested** (`tools/gpu/run.sh`); the rest **compiled only** |
| `lib/gpu/mc_vs.glsl`, `mc_fs.glsl` | voxel shaders: uvec2 vertex, integer shading identical to `mc_raster` (nearest texel with tile wrap, alpha cut-out, AO, face brightness, fog) | **written, never run** |
| `lib/mc/mc_backend.fi` | `Backend` = table of function values (`upload_atlas, mesh_upload, mesh_free, begin, draw, end, present, read_pixels`) + `backend_cpu(raster)`; NO GPU import (programs stay static) | **tested** through `wr_render_b` |
| `lib/mc/mc_render_world.fi` | new `wr_render_b / wr_clear_b / wr_free_b / wr_drop_chunk_b` take a `Backend`; the old `Raster` functions are wrappers | tests 2753 + 2763 pass |

## Not done (next worker)
1. `lib/gpu/gl_mc.fi`: `GlBackend` (FBO, program from the two .glsl files via `__include_str`, per-mesh VAO/VBO/IBO, atlas texture,
   camera uniforms: `u_rv = proj * view-without-translation` in f64 -> `gl_mat4_f64_to_f32`, `u_origin = (ox,oy,oz) - camera` in f64),
   plus `glbe_backend(&gb) -> mc_backend.Backend`. GL clip space is y-up: read back with `gl_read_rgba_top_down`.
2. Tests under Xvfb/llvmpipe: triangle, textured quad, voxel chunk GPU vs `mc_raster`; expect NOT byte equal (GL rasterizer is
   float, mc_raster snaps to 1/16 pixel): compare with PSNR (document the threshold after measuring).
3. Measure ms/frame GPU(llvmpipe) vs CPU raster (`tools/mc/raster/bench.fi` scene) -- llvmpipe is a CPU renderer, the number says nothing about a real GPU.
4. Window: X11 stays on the Firn window + `PutImage` of the read back frame (variant b of `docs/FFI.md`); `ctx_open_x11` is the Xlib alternative.
   Win32 side (`wglCreateContextAttribsARB`, `glctx.windows.fi`, `SwapBuffers`) belongs to MC-B.
5. Vulkan: only an option (`libvulkan.so.1` loads the same way; no probe written, no plan beyond this line).

## Build notes
* `firnc` of the main checkout (Oct 8) is too old (`link_lib` unknown): build `compiler/` (`cargo build --release`, 23 s).
* Escape check: pointers to locals passed through the `Gl` scratch array need `#[allow_escape]` (`compile`, `gl_uniform_loc`).
* Call GL from the main thread only (docs/FFI.md, TLS).

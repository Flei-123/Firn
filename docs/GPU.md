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

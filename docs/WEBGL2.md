# WebGL2 back end for the browser build of Minecraft (round gpu-m, 2026-10-12, roadmap r425)

Goal: the voxel client in a browser page (`--target=wasm32-browser`) draws through the SAME `mc_backend.Backend` table as on the
desktop (`gl_mc.glbe_backend`), on WebGL2. **Status: prototype runs** (`lib/gpu/glweb.fi`, gate 131); plan for the rest below.

## 1. What exists

| Piece | File | State |
|---|---|---|
| WebGL2 back end, same `Backend` table | `lib/gpu/glweb.fi` (`gpu.glweb`, names `glweb_*` like `glbe_*`) | runs; legacy mode only (one VAO/VBO/IBO per section) |
| ES 3.00 shaders | `lib/gpu/glweb_vs.glsl`, `glweb_fs.glsl` | same integer shading as `mc_vs/mc_fs.glsl` (u_filter 0) |
| JS host (1:1 WebGL2 calls, no logic) | `demos/mcweb/glweb.js`, page `demos/mcweb/index.html` | runs |
| Scenes + gate | `tools/gpu/web/{webscene,web_main,native_main}.fi`, `check.py`, `run.sh` | gate 131 in `test.sh` |
| fUi's 2D WebGL2 (older, other job) | `lib/plat/webgl.fi` + `demos/webdemo/firn.js` (`firn_gl_*`) | untouched; mcweb uses its own `firn_glw_*` namespace |

Usage (same call sequence as `gl_mc`): `glweb_open(w,h)` -> `glweb_backend(&gw)` -> `upload_atlas / mesh_upload / begin / draw / end`
-> `glweb_show(&gw)` (FBO -> canvas by `blitFramebuffer`, no readback) or `present` (readPixels, row 0 = top).
Build: `firnc --opt-level=release-safe --target=wasm32-browser -o demos/mcweb/mcscene.wasm tools/gpu/web/web_main.fi`, serve `demos/mcweb/` over http.

## 2. glbe_* / gl33 function by function

| gl_mc / gl33 feature | WebGL2 | Prototype |
|---|---|---|
| `glbe_open` (context + FBO + program) | context = `canvas.getContext('webgl2')` in the host (`firn_glw_init`); FBO RGBA8 + DEPTH24 renderbuffer 1:1 | done |
| `glbe_backend` table (atlas, mesh_upload/free, begin, draw, end, present, read_pixels) | 1:1 | done |
| `glbe_set_fog`, `set_cull`, `resize`, `close` | 1:1 (`glweb_set_fog/_set_cull/_resize/_close`) | done |
| texture array atlas (`TEXTURE_2D_ARRAY`, texelFetch) | 1:1 (`texImage3D`, ES 3.0) | done |
| `glVertexAttribIPointer` (uvec2 vertex) | 1:1 | done |
| `glDrawElements`, VAO | 1:1 | done (MODE_LEGACY) |
| `glbe_set_mode` POOL (`glDrawElementsBaseVertex`) | **missing**: base vertex only via extension `WEBGL_draw_instanced_base_vertex_base_instance` (Chrome/Firefox yes, Safari partly) | no. Fallback: index offset baked at upload (indices += voff, a CPU pass) |
| MODE_MULTI (`glMultiDrawElementsBaseVertex`) | `WEBGL_multi_draw` (+ `..._base_vertex_base_instance`); extension, not core | no |
| MODE_INDIRECT (GL 4.3) | **missing** | never; MULTI fallback |
| `gl_tex_buffer` (`usamplerBuffer` granule table / origin table) | **missing** (no buffer textures). Replace by RGBA32F 2D texture + `texelFetch(ivec2)` (width 4096) | no |
| `glBufferStorage`, mapped buffers (`glMapBufferRange`, persistent) | **missing**: only `bufferSubData` / `bufferData`; WebGL2 has `getBufferSubData` for readback only | uploads via `bufferSubData` into the page buffers (copy, no persistent map) |
| `glCopyBufferSubData` (defrag) | 1:1 (WebGL2 core) | easy once pools exist |
| `glFenceSync` / `glClientWaitSync` | core, but `clientWaitSync` may not block (timeout must be 0) | poll per frame |
| Timer queries (`gl_query`) | extension `EXT_disjoint_timer_query_webgl2`, often disabled (timing side channel), coarse | optional, off by default |
| SSBO, compute, `glMultiDrawElementsIndirect` | **missing** (no compute, no SSBO) | never; Hi-Z/culling stays on the CPU (`cull.fi`) |
| Hi-Z occlusion (depth pyramid + compute / TF) | no compute; fragment-shader pyramid possible (render to R32F texture needs `EXT_color_buffer_float`) | later, CPU `cull.fi` first |
| UBO (`gl_ext` UBO) | core in ES 3.0 | later, with scene layer |
| MSAA (`glRenderbufferStorageMultisample` + blit) | core (`renderbufferStorageMultisample`, `blitFramebuffer`) | later |
| anisotropic filtering | extension `EXT_texture_filter_anisotropic` | later (`u_filter` path) |
| `glbe_defrag`, upload budget (staged uploads) | pure CPU logic of `gl_mc.fi`, no GL feature missing | together with pool modes |

## 3. Shader variants (`#version 300 es`)

- Header: `#version 300 es` + `precision highp float; precision highp int;` (+ `highp sampler2DArray` in the fragment shader). Everything else
  of `mc_vs.glsl / mc_fs.glsl` compiles unchanged: `uvec2` input, `flat out int`, constant `int[8]` array, `texelFetch` on `sampler2DArray`,
  integer shifts on `ivec3`.
- Pool path removed (`usamplerBuffer`, `u_pool`, `gl_VertexID` lookup): does not exist in ES. Pool modes need the RGBA32F-texture replacement above.
- `u_filter == 1` (mipmaps/aniso) not ported. `gl_{sky,tint,ent,hud}_*.glsl` of the scene layer need the same header change only (check for `layout(std140)` UBOs: allowed).
- Selection: today a second pair of files; later `lib/@web/gpu/...` twin (below) with the dialect picked in the twin.

## 4. Sharing `gl_mc.fi` instead of a copy: the `lib/@web/` platform directory (NOT done)

`firnc` resolves `import gpu.gl33` first in `lib/@web/gpu/gl33.fi` when the target is `wasm32-browser` (`compiler/src/modules.rs`, step 5a;
today `lib/@web/` holds `std/clipboard.fi`, `std/dialogos.fi`, `std/toast.fi`, `fui/apphost.fi`). So `gl_mc.fi` can compile unchanged for the
browser if `lib/@web/gpu/{glctx,gl33,gl_ext,gl_fbo}.fi` exist with the SAME exports, built on `firn_glw_*` imports instead of
`dynlib.callx`. Work: ~90 `gl_*` wrappers (mechanical, the glw imports are the same calls), `GlX` feature probes answering `false` for
indirect/SSBO/timer/buffer texture, `gl_mc` choosing LEGACY when `has_tex_buffer` is false and the ES shader files when `gl_es()` is true.
Until then `glweb.fi` is the maintained twin: **any change of the Backend table semantics in `gl_mc.fi` must be mirrored there.**
(Parallel worker gpu-l extends `gl_mc`/`gl_scene` additively; scene-layer functions `glbe_x/bind_draw/rebind/camera_rv/msaa` have NO `glweb_` twin yet.)

## 5. Context, canvas, resize, loss

- Context: `canvas.getContext('webgl2', {alpha:false, antialias:false, depth:false, stencil:false, preserveDrawingBuffer:true})` -- the frame
  lives in our own FBO, the default framebuffer only gets the `blitFramebuffer`. `preserveDrawingBuffer` is only needed for snapshots (`toDataURL`); production: false.
- Resize: `glweb_resize(w,h)` remakes FBO + readback buffers (`firn_glw_size` sets `canvas.width/height`). The host must pass DEVICE pixels
  (`cssSize * devicePixelRatio`, as `lib/plat/web.fi` does for fUi). The camera aspect is the caller's job (as in `glbe_resize`).
- Loop: the prototype renders synchronously in `_start`. A real page needs `requestAnimationFrame` calling an exported frame function
  (`#[export_c]`, like `firn_web_frame` in `lib/plat/web.fi`) -- NOT built.
- Context loss (`webglcontextlost/restored`): handled since gate 133 and PROVEN in a browser by gate 141 (section 10); the old plan was: Plan like `webgl.fi`/gpucheck (`WEBGL_lose_context`): on restore re-run `glweb_open`
  work (program, FBO, atlas, re-upload of all sections from the CPU meshes; `gl_mc` already keeps CPU copies only when staged -> keep meshes in `mc_mesh` memory).

## 6. Input / audio / network (interface only)

- Input: browser events (`keydown/up`, `mousemove` with Pointer Lock `requestPointerLock` + `movementX/Y`, `wheel`, touch, Gamepad API `navigator.getGamepads`)
  -> host pushes into a ring in module memory -> same event structs as `lib/input` (`padcore` logical pad, `EV_RAWKEY`). Mirror of `lib/plat/web.fi` events. Not built.
- Audio: `lib/audio` engine has `audio.engine/mixer/synth` pure-Firn; the sink would be an `AudioWorklet` pulling interleaved f32 from a ring in wasm memory
  (SharedArrayBuffer needs cross-origin isolation: COOP/COEP headers), fallback `ScriptProcessor`. `mc_audio` (3D sound) unchanged. Not built.
- Gateway (later, in the Minecraft project): browsers have no raw TCP. Interface to keep: `net_open(url) / net_send(p,n) / net_poll(p,cap) -> n / net_close`
  as host imports over `WebSocket` (binary frames); a gateway process (WebSocket <-> Minecraft TCP, also doing the login/encryption side or passing it through) runs next to the server. Not built, no protocol work done.

## 7. Measured / tested (gate 131, `tools/gpu/web/run.sh`)

Chromium (headless, `--use-angle=swiftshader`: WebGL 2.0 on the SwiftShader software renderer; this server has no GPU), 320x180, 7 scenes
(triangle, textured quad, cut-out, fog on flat colour, hills chunk 2x2 sections, with fog, camera 1 000 000 / 2 000 000 blocks from the origin).
Two comparisons per run: (a) in the wasm module `mc_raster` (compiled to wasm) vs `glweb` with the thresholds of `tools/gpu/dump.fi`;
(b) the PNG of the canvas vs frames of the NATIVE `mc_raster` (`native_main.fi`), PSNR >= 34 dB and coverage within 5 permille.
Result of the run written down: triangle/cutout/fog_flat 99.0 dB (identical), quad_angled 46.3 dB (4 edge pixels), chunk 58.4 dB, chunk_fog 58.5 dB, chunk_far 58.4 dB;
GL 3.3 on llvmpipe has the same class of differences (float raster vs 1/16 pixel snapping).
**Not tested:** a real GPU (ANGLE/D3D/Metal/Vulkan drivers round differently; the integer shading should be exact, the rasteriser edges not), Firefox, Safari, mobile browsers,
WebGL2 context loss, frame time (the `--dump-dom` run uses virtual time; no honest ms/frame number was taken), large worlds, memory limits of the wasm heap.

## 8. Next steps

1. `requestAnimationFrame` loop + export of a frame function + resize/DPR + pointer lock input (a playable camera in the page).
2. `lib/@web/gpu/*` twins so `gl_mc.fi` itself runs in the browser (section 4); then MODE_POOL via `WEBGL_draw_instanced_base_vertex_base_instance` feature test, fallback baked indices.
3. Scene layer (sky/tint/entity/HUD shaders) in ES 3.00, MSAA, filtered texture path.
4. Frame-time measurement with a real browser session (CDP `Performance`, rAF timestamps), Firefox/Safari smoke, real GPU run.
5. Context-loss recovery, WebSocket gateway interface, AudioWorklet sink.

## 9. Scene layer, frame loop, input, context loss (round gpu-q, 2026-10-13, gate 133)

**Scene layer** `lib/gpu/glweb_scene.fi` (`gpu.glweb_scene`, `wscene_*`): twin of `gl_scene.fi` -- sky, light tint, entity instancing (frustum + software Hi-Z
culling by `gpu.cull`, pure Firn), HUD rectangles from `mc_hud.hud_record`. ES 3.00 shaders `glweb_{sky_vs,sky_fs,tint_fs,ent_vs,ent_fs,hud_vs,hud_fs}.glsl`
(copies of `gl_*.glsl`, new header; HUD rect attribute `uvec4` because ES checks the attribute base type -- **a change in `gl_*.glsl` must be mirrored**).
`glweb.glweb_rebind` (voxel state back after a pass) and `glweb_camera_rv` are the `glbe_rebind` / `glbe_camera_rv` twins. `glbe_flush` is a no-op (legacy mode draws at once).
Not there: timer queries (`wscene_timers()` is false, `wscene_pass_ms` 0.0; EXT_disjoint_timer_query_webgl2 not wired), MSAA/resolve, `scene_present_window`.
Gate 133 (`tools/gpu/web/run_scene.sh`, scenes in `sceneref.fi`): 7 scenes in headless Chromium/SwiftShader vs mc_raster+mc_sky+mc_hud, in-module PSNR 54-99 dB
(sky byte equal, max channel diff 2), canvas PNG vs native frames 54-99 dB, cull on/off byte equal (drawn 2 vs 6). Threshold 34 dB.

**Frame loop / input / resize**: `startMcWeb` (glweb.js) now runs a `requestAnimationFrame` loop when the module exports `firn_mc_frame(ms) -> u32` (0 = stop) and calls
`firn_mc_resize(cssW, cssH, dprMilli)` (ResizeObserver + dpr change); the program calls `glweb_resize(round(css*dpr))`. Input: `lib/input/webraw.fi` (`input.webraw`) exports
`firn_in_key/button/motion/wheel/focus/lock_state`, queues the arguments of `raw.raw_feed` in a ring and `webraw_poll(&raw)` = raw_begin + feed + raw_end, so game code is the
same as with `lib/window` (`raw.key_pressed`, `raw.mouse_dx`). Pointer Lock: `webraw_lock(true)` -> `canvas.requestPointerLock` (user gesture only), delta counts only while locked.
**Context loss**: host keeps `ctxLost/ctxGen` (`webglcontextlost` preventDefault, restored drops all ids); `glweb_poll_context` returns CTX_LOST (skip frame) / CTX_RESTORED
(program, FBO, atlas, meshes with CPU copy rebuilt; `keep_meshes` default on) -- then call `wscene_recreate`. **These three (loop, input, loss) compile but have NO automated browser test yet**
(pointer lock cannot run headless; rAF/loss test page is the next step).

**Minecraft browser build (interface only)**: entry = `main` builds world/back end once, exports `firn_mc_frame` (rAF), `firn_mc_resize`; input = `webraw_poll`; assets = host `fetch`
-> wasm memory (`firn_web_alloc` style) feeding `mc_assets` (no file system); network = WebSocket gateway imports `net_open/net_send/net_poll/net_close` (section 6), not built.

**@web twins (section 4)**: still NOT done. `gl_mc.fi` pulls gl33 (1011 lines), gl_ext (662), glctx (669), gl_fbo, gl_tex, gl_prog, gl_query, glpool = ~3500 lines of `dynlib` wrappers to re-implement on `firn_glw_*`.
Smallest clean step: `lib/@web/gpu/gl33.fi` + `gl_ext.fi` twins with identical exports; until then `glweb.fi` + `glweb_scene.fi` are the maintained twins.

## 10. Context loss test, frame loop fix, gamepads (round G5, gate 141)

**Gate 141** (`tools/gpu/web/run_loss.sh`, `web_loss_main.fi`, `loss.html`, `check_loss.py`): headless Chromium/SwiftShader. The page takes the context away with
`WEBGL_lose_context.loseContext()` after 4 frames and gives it back with `restoreContext()` once `webglcontextlost` arrived; the Firn frame function
calls `glweb_poll_context` (22 frames of `CTX_LOST` in the last run, then exactly one `CTX_RESTORED`) and the frame after the restore is **byte-equal**
(FNV-32 of the readback) to the one before the loss: program, FBO, atlas and the mesh (CPU copy, `keep_meshes`) were rebuilt. Not covered: a real GPU reset
(driver crash, GPU process kill), Firefox / Safari / mobile, loss in the middle of a frame, loss while a texture upload is in flight.
Headless Chromium under `--virtual-time-budget` never fires `requestAnimationFrame`; `loss.html` drives the loop with a timer-based `requestAnimationFrame`
shim (same frame function, other clock). A real `requestAnimationFrame` loop was therefore NOT exercised by any test.

**Bug found while writing gate 141 (fixed in parallel on main, same fix):** `glweb.js` returned right after `main` (every wasm program ends in `exit(0)`, thrown as `Exit`), so a program that exports
`firn_mc_frame` never got its frame loop. Now an `Exit` with code 0 from a module that exports `firn_mc_frame` continues into the loop (the module memory is intact).

**Gamepads (browser Gamepad API)**: `lib/input/webpad.fi` (`input.webpad`). `glweb.js` polls `navigator.getGamepads()` once per frame before `firn_mc_frame`
and calls `firn_pad_state(slot, buttons_mask, lt, rt, lx, ly, rx, ry)` for every pad with `mapping == "standard"`, `firn_pad_gone(slot)` when it left.
`webpad_poll(slot, &pad)` feeds the logical `padcore.Pad` through `pad_feed_xinput` (radial stick deadzone, trigger deadzone, edges, PE_CONNECTED / PE_DISCONNECTED).
Tests: `tests/2930_input_webpad.fi` (button map, scaling, clamps, y direction, slots, unplug / replug, 3000 random frames vs a naive model; x86-64 3 opt levels,
aarch64/qemu and Windows/Wine run it too) and the browser smoke in gate 141 (a FAKED `navigator.getGamepads`: A held, stick right, trigger 0.5 reaches Firn as
one connected pad). Not done: rumble (`vibrationActuator`), non-standard mappings (ignored, not guessed), a real pad in a real browser (untested), disconnect in
the browser run (only the native test covers it).

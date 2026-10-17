# fUi GPU path with CPU fallback (concept, W18-E)

## Rule
- The CPU rasteriser stays the DEFAULT and the reference. The GPU path is opt-in (`FUI_GPU=1`) and may only be switched on when its output is pixel-near to the CPU frame.
- Every GPU failure (no GL library, shader compile error, context lost) falls back to the CPU painter for the rest of the run; the window keeps its content (a full repaint follows).

## What exists
- `lib/fui/gpu.fi` (Gpu, targets, `gpu_attach(g, canvas)`, `gpu_flush`), shaders `gpu_vs.glsl`, `gpu_fs*.glsl`, `lib/plat/gles.fi`. Works on Android (GLES) and the web build; X11/Windows/OrientOS answer "no" because no system GL is loaded (r156).
- `painter_clear`, `surface`, `round_rect` already branch on `gpu.gpu_of(canvas)`: a canvas with a Gpu is painted by the GPU, else by the CPU.

## Minimal entry (next steps, smallest first)
1. `gpu.gpu_probe(api) -> bool`: try to open the system GL (needs a way to load libGL/libEGL: Firn programs are linked statically and have none yet, see the header of `gpu.fi`; first step may be a tiny helper process or a Firn dynamic loader), create a 16x16 target, clear, read back one pixel. Pure yes/no, no window. Test: on a host without GL it returns false and the program continues on the CPU.
2. Host switch in `apphost`: `FUI_GPU` unset or 0 -> never touch GL. 1 -> probe; true -> `gpu_attach`, false -> CPU (print one line to stderr).
3. Parity gate `tools/fui/gpuparity.sh`: paint the showcase sheets once on the CPU and once through the GPU path (llvmpipe / Xvfb+GLX is enough), compare per channel; threshold: mean abs diff <= 1.0, max diff <= 12 (AA / blur rounding), glyph pixels excluded until the GPU text path matches the CPU glyph cache. CPU frames stay golden-exact (`showcase.sh`).
4. Fallback test: force a failing shader (env `FUI_GPU_FAIL=compile`) -> frame must equal the CPU frame bit for bit.

## Not in scope here
- Glass blur / merge groups on the GPU (r71, r244 offline), Vulkan (`tools/gpu/vk`).

## Vulkan backend (W18-F): measured facts + the plan that fits the code
- DONE: `tools/fui/gpu_probe_main.fi` -- loader, instance, device (Mesa lavapipe 1.3 here), 16x16 clear + read-back: OK. Run with `XDG_RUNTIME_DIR` set. Exit 4 = no Vulkan (gate prints SKIP).
- DONE: `lib/fui/spv/build.sh` turns the SAME GLSL ES sources of the GLES backend (`gpu_vs.glsl`, `gpu_fs.glsl`, `gpu_fs_photo.glsl`) into Vulkan SPIR-V (committed `gpu_vk.vert.spv`, `gpu_vk.frag.spv`, `gpu_vk_photo.frag.spv`, spirv-val clean). Interface: set 0 binding 0 std140 block `P { vec2 u_size; vec2 u_srcsize; int u_nspot; vec4 u_spot[8]; }` (160 octets), bindings 1/2/3 = `u_atlas`/`u_stream`/`u_src` (combined image sampler), varyings `v_c0 v_c1 v_g v_p v_t v_clip v_k` = locations 0..6, vertex inputs 0 (vec2 corner, per vertex) + 1..8 (vec4, per instance, stride 128).
- NEXT (not built yet): `lib/fui/gpu_vk.fi` = a `paint.gl.GlApi` table filled by Vulkan (the table is only ~42 slots), so `gpu.fi` (batching, atlas, glass/frost passes, read-back) runs unchanged on X11/Windows/OrientOS. Design decisions:
  - Memory layout of GL and Vulkan agree WITHOUT any viewport flip (texel row 0 = NDC y -1, `gl_FragCoord.y` counts the same way), so the shaders and `gpu_read` stay as they are.
  - All images stay in layout GENERAL (render pass initial = final = GENERAL, descriptors use GENERAL): no layout tracking.
  - Every GL operation that touches the GPU (draw, clear, upload, blit, read) is ONE command buffer, submitted and fenced (`vk_frame.imm_begin/imm_submit`); descriptor set and uniform buffer are rewritten per draw. Simple and correct; batching is gpu.fi's job already (one instanced draw per batch).
  - Clear (also with scissor, `gpu_clear_rows`) = render pass with loadOp CLEAR and renderArea = scissor. Blit/copy = `vkCmdBlitImage` (X_CMD_BLIT). Blend = premultiplied (ONE, ONE_MINUS_SRC_ALPHA) -> needs one new blend mode in `vk_desc.pipeline_make`; `PipeSpec` has only 6 attribute slots, this pipeline needs 9 (enlarge to 12).
  - Program 1 = main FS, program 2 = photo FS (tell them apart by the length of the fragment source); uniform locations are (program << 8 | slot); `uniform1i` on a sampler = texture unit; a program owns its 160-octet uniform image. Texture units 0..2 -> bindings 1..3 (photo: `u_src` = unit 0 -> binding 3); unbound units get a 1x1 dummy.
  - Present: FB 0 is not a window; X11/Windows hosts call `gpu_read` and use their normal CPU present (or a swapchain blit later).
  - Fallback: `FUI_GPU_FAIL=open|compile` makes the table's open / `program` fail -> `gpu_open` false -> the CPU painter keeps running, frame bit-equal to a CPU-only run.

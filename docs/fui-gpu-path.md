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

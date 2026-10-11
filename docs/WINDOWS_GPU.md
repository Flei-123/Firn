# Windows GPU: WGL context, GL loader, full screen, mouse capture (round MC-B)

Closes GAPS B16 (Windows import list) and the Windows half of
`window.gpu_open`. Code: `lib/gpu/wgl.windows.fi`, `lib/plat/glwin.fi`
(generated), `lib/window/win32.fi` (`r_gpu_*`, `r_fullscreen`, `r_cursor_*`),
API in `lib/window/window.fi`. Probe: `tools/ffi/win_wgl_probe.fi` (step in
`tools/ffi/run.sh`).

**Tested under Wine 8.0 + Xvfb + Mesa 22.3 llvmpipe only. Real Windows with a
real GPU driver: UNTESTED (P8).**

## B16: is the import list still a problem?

No compiler patch is needed any more. `std.dynlib` (LoadLibraryA /
GetProcAddress + the Win64 call gate `win64_call`, `docs/FFI.md`) reaches any
DLL function. Decision: **no new attribute** (`#[import(dll, name)]` would only
be sugar over what the gate does, and every new `extern fn` form costs
compiler code in `win.rs`). The rule now:

* a function the compiler already imports (213 names: user32 window basics,
  gdi32 DIB, kernel32, ...) stays an `extern fn` (fast, typed);
* everything else is looked up at run time: `wgl.dll_sym("user32.dll",
  "ShowCursor")` returns the address, `wgl.call0..call10(f, a0..)` calls it
  (up to 10 integer/pointer slots; floats as f32 bits in the low half of a
  slot, `wgl.bits32`). Used here for `ClipCursor`, `SetCursorPos`,
  `GetCursorPos`, `ClientToScreen`, `MonitorFromWindow`, `GetMonitorInfoW`,
  `ShowCursor`, all of opengl32/gdi32 WGL and every GL function.
* Cost per call: one table lookup + the gate (a few ns); nothing measured.
* Limits: no variadic functions, no struct-by-value, no callbacks without
  `#[win_callback]`. Call from the thread that owns the window/context.

## Window API (all back ends have it; only Windows answers true)

```
window.gpu_open(f, api)               fUi: context + gl.GlApi table (lib/plat/glwin.fi)
window.gpu_gl(f, major, minor, depth, stencil)   own GL: context >= major.minor core
window.gpu_proc(f, "glCreateShader")  address (wglGetProcAddress, then opengl32 export)
window.gpu_swap(f) / gpu_vsync(f, on) SwapBuffers / wglSwapIntervalEXT
window.fullscreen(f, on)              borderless, on the window's monitor, old box restored
window.cursor_mode(f, CURSOR_NORMAL|HIDDEN|CAPTURED)
window.cursor_delta(f, out)           captured: (dx, dy) since last call, pointer back to centre
```

`gpu_gl` / `gpu_open` first call fixes the pixel format (Windows allows
`SetPixelFormat` once per window): double buffered RGBA8, `depth`/`stencil`
bits from the first call (fUi asks 0/0, a game 24/8). X11/Android/Osum return
false/0 (stubs); MC-A fills the Linux side.

## gpu.glctx on Windows (twin of MC-A's lib/gpu/glctx.fi)

`lib/gpu/glctx.windows.fi` has the names of the EGL file (`GlCtx`, `ctx_open`,
`ctx_open_x11`, `ctx_proc`, `ctx_swap`, `ctx_swap_interval`, `ctx_close`,
`ctx_x11_pending/pump`) plus `ctx_borrow()` (use the context that
`window.gpu_gl` made). `lib/gpu/gl33.fi` only needs `ctx_proc` + `dynlib.callx`,
so it builds for Windows unchanged once `glctx.fi` is on main (it is on branch
staff/firn-mv47zsn1 today; `import gpu.glctx` on Linux fails until then).
`ctx_open` = hidden helper window; `ctx_open_x11` = visible window (name kept
for the shared API). Probe: `tools/ffi/win_glctx_probe.fi` (Wine: headless
clear+readback, window + swap, borrow: ok). gl33 itself was not run on Windows.

## How the context is made (`lib/gpu/wgl.windows.fi`)

GetDC, ChoosePixelFormat/SetPixelFormat, legacy `wglCreateContext` to be able
to ask `wglGetProcAddress("wglCreateContextAttribsARB")`, then the core context
of the requested version, legacy one deleted. A driver without the ARB entry
point keeps the legacy context (only if version < 3). No MSAA format yet
(`wglChoosePixelFormatARB` is not used).

## Measured (Wine 8.0, Xvfb 800x600, llvmpipe)

* request 3.3 core -> `GL_VERSION 4.5 (Core Profile) Mesa 22.3.6` (drivers may
  hand out a higher compatible version; check `Wgl.major/minor`).
* 128x128 window: red clear, green triangle with raw GL calls: 2048 green
  pixels (exact half-area), mid pixel (0,255,0), corner (255,0,0); same triangle
  in blue through fUi's `GlApi` table: 2048 blue pixels; readback hashes stable
  across runs (FNV 4091656133 / 3262751173 on this Mesa).
* `#version 300 es` shaders (fUi's own) are accepted by Mesa in the core
  context. Other drivers: unknown (ARB_ES3_compatibility); fix would be a
  `#version 330 core` variant in `lib/fui/gpu.fi`.
* swap, vsync on/off, full screen (window grows to the 800x600 screen, back to
  the old box), mouse capture: hand-moved pointer +(5,3) reads delta (5,3), the
  next read 0.

## HONEST: not done / not proven

* Real Windows + NVIDIA/AMD/Intel drivers, multi-monitor, high DPI scale with
  `fullscreen`, a window with the custom frame (`frame != 0`, round C-049) in
  full screen.
* Mouse capture is polled (`GetCursorPos` + `SetCursorPos` to the centre), not
  raw input: acceleration is applied, fast flicks can hit the clip edge. Raw
  input (`WM_INPUT`) would be the next step for a shooter.
* `ClipCursor` is released on `cursor_mode(NORMAL)`, not when the window loses
  focus: a game must call `cursor_mode` on focus change.
* `lib/plat/glwin.fi` is generated: `python3 tools/ffi/gen_glwin.py >
  lib/plat/glwin.fi` after changing `lib/plat/gles.fi`.
* Not wired: fUi's desktop picker (`lib/plat/fuiwin.fi`) does not call
  `gpu_open` on Windows yet.

# FFI: how a static Firn binary reaches system libraries (round FFI)

Roadmap anchor: r114 (fUi GPU back end for the desktop), GAPS B16 (Windows
import list in `win.rs`), GAPS R9. Code: `lib/std/dynlib.fi`,
`lib/std/dynlib.windows.fi`, `compiler/src/win.rs` (`dyncall_asm`),
`compiler/src/extfn.rs` (`#[link_lib]`), `compiler/src/main.rs` (link step).
Probes: `tests/2260_dynlib_linux.fi`, `tools/ffi/` (run by `tools/ffi/run.sh`,
section 118 of `test.sh`).

## The question

A Firn program is a static image with no C library (`ROADMAP.md`: dynamic
libraries "permanently excluded", r34). GL, Vulkan and EGL are shared
libraries. The two systems differ:

* **Windows**: the program already has a PE import table that `win.rs` writes
  itself. Only the *list* of known functions was closed (216 entries).
* **Linux**: the image has no `PT_INTERP`, no `DT_NEEDED`: the kernel starts it
  directly, so nothing can load `libEGL.so`.

## Design: the result

```firn
import std.dynlib                       // same API on both systems

let h: u64 = dynlib.open("libEGL.so.1") // dlopen / LoadLibraryA, 0 = failed
let f: u64 = dynlib.sym(h, "eglGetDisplay")
let d: u64 = dynlib.ci1(f, 0)           // integer/pointer shortcuts ci0..ci4
dynlib.callx(fn, "iiff", argv, false)   // general: sig 'i' 'f' 'd', 1 slot each
```

* **Windows** gets `LoadLibraryA/W` and `FreeLibrary` in the import list, and a
  generic **Win64 call gate** (`win64_call`, `win64_callf`) in the runtime asm.
  A Firn indirect call is System V, so a pointer from `GetProcAddress` could not
  be called before (that is why every Win32 function had to be a compiler patch:
  B16). The gate takes `(fn, argv, n)`, puts slots 0-3 into `rcx rdx r8 r9` *and*
  `xmm0-3` (Win64 counts by position, so floats work), the rest onto the stack
  above the shadow space (up to 16 arguments). `win64_callf` returns `xmm0`.
  **A new Win32 call now needs no compiler patch.**
* **Linux** gets the attribute `#[link_lib(c)]` on an `extern fn`. A program that
  contains one is linked as a *dynamic* ELF: `ld -dynamic-linker
  /lib64/ld-linux-x86-64.so.2 ... -l:libc.so.6` (`PT_INTERP` + `DT_NEEDED`).
  `dynlib.fi` declares `dlopen/dlsym/dlclose` this way. Every other program stays
  byte for byte static (checked in `tools/ffi/run.sh`). The calls go through a
  typed function value with 6 integer + 8 float registers + 6 stack words
  (System V), so 12 integer or 8 float arguments, no variadics.

## The options, with what was measured

| | Option | Verdict |
|-|-|-|
| W1 | Add `LoadLibraryA/W`, `FreeLibrary` to the import list + a generic call gate | **built**, works under Wine (below) |
| W2 | Only add more entries to `KNOWN` per function | what B16 complained about, no |
| L1 | Dynamic ELF on demand (`PT_INTERP` + `DT_NEEDED libc.so.6`), `dlopen` from there | **built**, works with real Mesa |
| L2 | Own ELF loader / own `dlopen` in Firn | not built. It would have to reproduce `ld.so` (relocations, IFUNC, TLS, init order) for Mesa and its dependencies: weeks, and fragile. Rejected without a spike. |
| L3 | Helper process that owns the GL library: it still needs libGL itself, so it only moves the problem, and every frame crosses a socket/shm | not built, not measured. Rejected by reasoning. |

Measurements (this server, Debian 12, glibc 2.36):

| | static | dynamic (`#[link_lib(c)]`) |
|-|-|-|
| size of `fn main() { return 7 }` | 1328 bytes | 13 944 bytes |
| process start (500 runs, mean) | 231 us | 643 us (+0.41 ms, `ld.so` + libc) |

## What was really tested

**Real runs (not just compiled):**

* Linux `tests/2260_dynlib_linux.fi` (also in the main test loop, all four build
  levels): `dlopen("libc.so.6")`, `getpid`, `strlen` (pointer), `labs` (negative
  integer), `libm` `floor` (f64 in/out), `ldexp` (f64 + int), refusal of 13
  integer arguments, `dlclose`, a missing library/symbol gives 0. Exit 42.
* Linux `tools/ffi/egl_probe.fi`: `libEGL.so.1` + `libGLESv2.so.2` via dlopen,
  `eglGetDisplay/Initialize/ChooseConfig/CreatePbufferSurface/CreateContext/
  MakeCurrent`, `glClearColor` (4 floats), `glClear`, `glReadPixels` (7 integer
  arguments, 1 on the stack): the red pixel (255,0,0,255) is read back. Run
  headless (`EGL_PLATFORM=surfaceless`) and on an X server (Xvfb).
* Windows under Wine 8.0 (`tools/ffi/win_probe.fi`, `win_gl_probe.fi`):
  `LoadLibraryA` of kernel32, msvcrt, user32, gdi32, opengl32; `GetProcAddress`;
  calls with 0, 1, 2, 8 arguments (the extra ones on the stack,
  `WideCharToMultiByte`), floats in and out (`floor`, `ldexp`), a window,
  `ChoosePixelFormat`, `SetPixelFormat`, `wglCreateContext`, `wglMakeCurrent`,
  `glClear`, `glReadPixels`: red pixel read back.
* `cargo test`: `win::` (call gate asm, gates are no imports), `attrs::`,
  `extfn::`. Negative test `tests/neg/2260_link_lib_not_extern.fi`.

**Compiled only / not tested:** Windows on real hardware (Wine only); a real
GPU (Mesa llvmpipe only); aarch64 Linux (`#[link_lib]` is ignored there: the
loader path `/lib/ld-linux-aarch64.so.1` and an `aarch64` call gate are not
written); musl and other non-glibc systems (`libc.so.6` is a glibc name);
macOS; Vulkan (`libvulkan.so.1` loads the same way, no probe written).

## Two traps found while building it

1. **`exit` vs `exit_group`.** A Firn `_start` ends with `exit` (syscall 60).
   Mesa starts worker threads (llvmpipe); after `main` returned the process
   stayed alive, hung, with only those threads left. A dynamic image now ends
   with `exit_group` (231); static images are unchanged.
2. **Threads.** Foreign code runs on glibc's TLS. A thread made by Firn's own
   `thread.spawn` has its own `fs` base (the collector's block), so a foreign call
   from there would read garbage. Call foreign libraries from the main thread (or
   from threads the foreign library made itself). Not enforced, only documented.
   The probe runs with `gc_init()` on the main thread and is fine.

## What r114 (fUi GPU back end, desktop) still needs

The loading problem is solved; these are left:

1. **A desktop `GlApi` table** (`lib/plat/gldyn.fi`): fill every entry of
   `lib/paint/gl.fi` with a function value that calls `dynlib.callx` on the symbol
   (`libGLESv2.so.2` / `libGL.so.1` / `opengl32.dll`; core 1.1 functions are
   exported, everything newer comes from `eglGetProcAddress` /
   `glXGetProcAddress` / `wglGetProcAddress` and has to be called after a
   context is current). The table is all-64-bit on purpose, so the shapes are
   few: ints, a float, pointers.
2. **Windows window**: `window.gpu_open` for `backend.windows.fi`: the HWND exists,
   so `GetDC`, `ChoosePixelFormat`/`SetPixelFormat`, `wglCreateContext` is the
   sequence `win_gl_probe.fi` already runs; for GL 3.3 core
   `wglCreateContextAttribsARB` through `wglGetProcAddress`; `SwapBuffers` from
   gdi32 for `gpu_swap`.
3. **X11 window**: the real hole. `lib/window/x11.fi` speaks the X protocol on
   its own socket (no Xlib, no xcb). An EGL/GLX window surface needs a native
   window created through *Xlib's* `Display*` or xcb's connection. Options:
   (a) open Xlib/xcb through `dynlib` too and create the window with it (the
   window code would be written twice); (b) keep the Firn window, render with a
   **surfaceless/pbuffer EGL context into an FBO**, `glReadPixels` and send the
   result with the existing `PutImage` (GPU does the work, one readback per
   frame; the probe above is exactly this context; cost not measured);
   (c) Vulkan with `VK_KHR_xcb_surface` has the same connection problem as (a).
   This is a product/design decision for r114.
4. **`gpu_lost`/context loss, DPI, vsync** (`eglSwapInterval`, `wglSwapIntervalEXT`).
5. **Floats with more than 8 arguments, variadics** (`glGetString` is fine;
   `printf`-like are not needed for GL).
6. **Callbacks into Firn** from foreign code (e.g. `glDebugMessageCallback`,
   GLFW-like callbacks): Windows has `#[win_callback]`; Linux function values are
   plain System V and work as callbacks without a thunk, but this was not tested.
7. **Migrating the old pointer hacks** in `lib/window/win32.fi` (`ruf_*`,
   `user32_get`, the `KNOWN` entries bound "because an indirect call is System V")
   to `dynlib`. Not done here; they work and are not part of this round.

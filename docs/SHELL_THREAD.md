# Shell thread (lib/window/shellthread.fi)

Status: final for the first integration (23.10.2026). Source of truth is the code; every name below was checked against it (`grep`), and the example in section 5 is test `tests/4860_shellthread_app_loop.fi`.

## 1. Purpose

The OS window lives on its own thread (the **shell thread**). The app thread can block for 500 ms (long frame, GC, disk) and the window still answers, repaints its last frame and resizes. On Windows the modal drag/resize loop (the title-bar drag) no longer freezes together with the app.

Files:

| file | role |
|---|---|
| `lib/window/shellthread.fi` | the library: app API `shell_*` and the shell thread body `shell_main` |
| `lib/window/shellproto.fi` | constants shared by both sides: `SHELL_KIND`, record sizes, command kinds, block offsets, states, stat ids |
| `lib/window/shellos.fi` | OS seam, Linux/X11 twin (no hook, no event, polls every 4 ms) |
| `lib/window/shellos.windows.fi` | OS seam, Windows (wake event + `MsgWaitForMultipleObjectsEx`, paint hook, drag timer) |
| `lib/window/win32.fi` | `r_paint_hook(w, fn(u64), ud)` and `r_timer(w, ms)` used by the seam |
| `lib/chan/chan.fi` | the four rings (see `docs/CHAN.md`) |
| `tools/shellthread/demo.fi` | demo and measurement program |

## 2. Architecture

```
 app thread                                    shell thread (owns the OS window)
 ----------                                    --------------------------------
 shell_start  --- thread_start(SHELL_KIND) --> shell_main: open window, loop
 shell_poll   <-- ev   (512 x 8 words)  ----   events from the window
 shell_set_*  --- cmd  (64 x 16 words)  --->   title / cursor / size / close / redraw
 frame_submit --- frm  (8 x 8 words, 5 used) --> keeps only the NEWEST frame, blits it
 frame_acquire <-- free (16 x 2 words)  ----   hands older frames back
```

- The two sides share one block (`sh`, a `u64` handle from `rt.heap_alloc`, offsets in `shellproto.B_*`) and four `chan.chan` rings. No locks.
- **App thread owns**: the frame buffers (allocates and frees them), the `sh` block until `shell_stop`.
- **Shell thread owns**: the `window.Window`, the OS handle, the "current" frame pointer it shows (`B_CUR`) while it is displayed.
- **Wake-up**: after a command or a frame the app wakes the shell (Windows: `SetEvent`; Linux: nothing, the shell polls every 4 ms).
- **Windows modal loop**: `win32.fi` calls the shell hook after WM_PAINT/SIZE/MOVE/WINDOWPOSCHANGED/EXITSIZEMOVE/TIMER, so events, commands and frames are serviced INSIDE the OS drag/size loop. While a drag is on, `r_timer(8 ms)` keeps the hook running. A busy flag blocks re-entry (`SetWindowPos` from a command triggers WM_SIZE synchronously).
- The shell repaints the last shown frame on expose, resize and drag; older queued frames are dropped (`STAT_DROPPED`).

Records (`shellproto`):

| ring | direction | cap | record words |
|---|---|---|---|
| ev | shell -> app | 512 | 8: `kind, a, x, y, state, dpi, seq, t_ms` |
| cmd | app -> shell | 64 | 16: `kind, args...` (`CMD_TITLE`: byte length in word 1, up to `TITLE_MAX` = 112 bytes inline from word 2) |
| frm | app -> shell | 8 | 5 used of 8: `ptr, w, h, stride, id` |
| free | shell -> app | 16 | 2: `ptr, id` |

Event record: `kind` is a `window.EV_*` value (`EV_KEY`, `EV_CHAR`, `EV_BUTTON`, `EV_MOTION`, `EV_CONFIGURE`, `EV_CLOSED`, `EV_EXPOSE`, `EV_THEME`, ...; `full_input` is switched on, so raw keys, focus, release/enter/leave arrive too) or `shellproto.SE_DRAG` (= 100: `a` = 1 drag/size loop started, 0 ended). `a`, `x`, `y`, `state` are `window.ev_a/ev_x/ev_y/ev_state` of that event (meaning per kind: see `lib/window/window.fi`, e.g. `EV_CONFIGURE` x/y = new width/height). `dpi` = window DPI at that moment, `seq` counts events, `t_ms` = `monotonic_ns() / 1000000` taken in the shell thread.

## 3. API reference (app thread; `import window.shellthread`, `import window.shellproto`)

Return type `u64` handles are plain numbers; all functions below run on the app thread only and never block unless stated.

| function | semantics |
|---|---|
| `shell_start(title: str, w: usize, h: usize) -> u64` | allocates the block and four rings, starts the shell thread. Returns the handle, `0` only if allocation or `thread_start` failed. It does NOT wait: the window is not open yet. Title is cut at 112 bytes. |
| `shell_wait_open(sh, ms: u64) -> bool` | BLOCKS (sleeps 1 ms steps) up to `ms`. `true` = window open (`ST_OPEN`); `false` = window failed (`ST_FAILED`, e.g. no display), thread ended, or timeout. |
| `shell_state(sh) -> u64` | `ST_STARTING` 0, `ST_OPEN` 1, `ST_FAILED` 2, `ST_ENDED` 3. |
| `shell_poll(sh, rec: *mut u64) -> bool` | non blocking; copies one 8 word event into `rec`; `false` = none. |
| `shell_wait(sh, rec, ms: u64) -> u64` | BLOCKS up to `ms`; `chan.RECV_OK` (0) / `RECV_TIMEOUT` (1) / `RECV_CLOSED` (2). `ms == 0` is a try. Polling granularity: see `docs/CHAN.md` C-1. |
| `shell_frame_acquire(sh, w, h) -> u64` | returns a pointer to `w*h` BGRX pixels (32 bit, stride `w*4`, memory is NOT cleared). `0` = `w` or `h` is 0, all `MAX_BUFS` (4) buffers are in flight, or allocation failed. Reuses a free buffer when big enough, else reallocates it. |
| `shell_frame_submit(sh, pix, w, h) -> bool` | gives the buffer to the shell. `false` = `pix` is not a buffer of this shell, or the frame ring is full (buffer stays yours). After `true` the app must NOT touch `pix` until it comes back from `acquire`. `w`, `h` are the size you really painted. |
| `shell_set_title(sh, title: str) -> bool` | `false` = command ring full (64). |
| `shell_set_cursor(sh, shape: u32) -> bool` | `window.SHAPE_ARROW/IBEAM/HAND/CROSS/WAIT/SIZE_WE/SIZE_NS/MOVE` (0..7; larger is ignored by the backend). |
| `shell_set_size(sh, w, h) -> bool` | resizes the OS window; an `EV_CONFIGURE` event follows. |
| `shell_redraw(sh) -> bool` | repaint the last frame. |
| `shell_close(sh) -> bool` | closes the OS window (does not stop the thread). |
| `shell_stop(sh)` | BLOCKS: sets the stop flag, wakes the shell, `thread_wait`, frees all buffers, rings and the block. `sh` is invalid afterwards. Call it exactly once, also after `shell_wait_open` returned `false`. |
| `shell_stat(sh, which: u64) -> u64` | counters: `STAT_SHOWN` (frames taken), `STAT_DROPPED` (replaced before shown), `STAT_PAINTS` (blits incl. repaints), `STAT_EVDROP` (events lost). |
| `shell_width(sh)`, `shell_height(sh)`, `shell_dpi(sh)` | last known client size / DPI, updated by the shell on `EV_CONFIGURE`. |
| `shell_hwnd(sh) -> u64` | OS window handle (Windows HWND), valid once open. |
| `shell_main(sh) -> u64` | the shell thread body; only for the dispatcher. Returns 0 normally, 1 if the window could not open. |

## 4. Thread rules

- A library cannot define `__thread_work`. The APP defines it and routes the shell kind:

```
fn __thread_work(kind: u64, arg: u64) -> u64 {
    if kind == shellproto.SHELL_KIND {
        return shellthread.shell_main(arg)
    }
    return 9
}
```

  `SHELL_KIND` = 1936222316. Other kinds are free for the app.
- `shell_main` calls `thread_blocking_an()` first and stays BLOCKED until it ends: the collector never waits for the shell thread, not even inside the Win32 modal loop. Therefore the shell thread must not allocate on the GC heap and must not hold GC pointers. The code there only uses `rt.heap_alloc`, its own stack, and `chan_try_send` / `chan_try_recv` (never the blocking ones).
- `lib/window/win32.fi` has no GC allocation on that path (one `heap_alloc`, checked by grep). Do not add a GC-allocating callback to the paint hook (limit H8).
- Frame buffers and the `sh` block are `rt.heap_alloc` memory (outside the GC heap). The app frees them only through `shell_stop`.
- Do not call any `shell_*` from the shell thread; do not call `window.*` on the same window from the app thread.
- The app should send at most one frame per paint and read `shell_width/height` right before `frame_acquire`; a frame of another size than the window is NOT scaled (limit H3).

## 5. Minimal program (app side)

This is `tests/4860_shellthread_app_loop.fi` (builds for Linux and `--target=x86_64-windows`; runs headless too: if no window can open it ends with exit 0 after `shell_stop`).

```
import std.rt
import std.time
import window.window
import window.shellthread
import window.shellproto

fn __thread_work(kind: u64, arg: u64) -> u64 {
    if kind == shellproto.SHELL_KIND {
        return shellthread.shell_main(arg)
    }
    return 9
}

fn paint(pix: u64, w: usize, h: usize, frame: u32) {
    var y: usize = 0
    while y < h {
        var x: usize = 0
        while x < w {
            rt.st32(pix + (y * w * 4) as u64, x, 0xFF000000 | (frame * 8) | ((x as u32) & 255) << 8)
            x = x + 1
        }
        y = y + 1
    }
}

fn main() -> i32 {
    let sh: u64 = shellthread.shell_start("app loop", 320, 200)
    if sh == 0 {
        return 1
    }
    if !shellthread.shell_wait_open(sh, 8000) {
        shellthread.shell_stop(sh)      // headless: still cleans up
        return 0
    }
    var rec: [u64; 8] = [0; 8]
    var frame: u32 = 0
    var closed: bool = false
    while frame < 30 && !closed {
        while shellthread.shell_poll(sh, &rec[0]) {          // events
            if rec[0] == window.EV_CLOSED as u64 {
                closed = true
            }
        }
        let w: usize = shellthread.shell_width(sh)          // CURRENT size (resize / DPI)
        let h: usize = shellthread.shell_height(sh)
        let pix: u64 = shellthread.shell_frame_acquire(sh, w, h)
        if pix != 0 {                                        // 0 = all buffers in flight: skip
            paint(pix, w, h, frame)
            shellthread.shell_frame_submit(sh, pix, w, h)
        }
        if frame == 5 {
            shellthread.shell_set_title(sh, "app loop 2")
            shellthread.shell_set_size(sh, 400, 300)
        }
        frame = frame + 1
        thread_sleep(10)
    }
    shellthread.shell_stop(sh)
    return 0
}
```

Handling the usual cases:

- **Resize**: an `EV_CONFIGURE` event arrives (`x`,`y` = new size); `shell_width/height` already return it. Paint the next frame at that size.
- **DPI change**: `rec[5]` and `shell_dpi` change; the app must re-render itself (no scaling by the shell).
- **Close**: `EV_CLOSED` arrives; leave the loop and call `shell_stop`. `shell_close` closes from the app side.
- **Drag/size loop** (Windows): `SE_DRAG` with `a == 1` / `a == 0`. The app may lower its frame rate meanwhile; the window keeps repainting the last frame by itself.
- **Waiting instead of polling**: `shell_wait(sh, &rec[0], 16)` replaces `thread_sleep` in the loop.

## 6. Integration checklist for Certus

1. Add the `__thread_work` dispatcher with the `SHELL_KIND` branch (section 4) to the program (browser main). Keep the other kinds.
2. Import `window.shellthread` and `window.shellproto`. `window.shellos` is pulled in by the library; the file `shellos.windows.fi` is selected automatically for the Windows target.
3. Windows imports: the Certus compiler (`firnc-gc`, FIRNLIB `/root/firnlib-c062`) must know `CreateEventW`, `SetEvent`, `MsgWaitForMultipleObjectsEx` (the last one is already in its `win.rs`), `WaitOnAddress`, `WakeByAddressSingle/All` (arrive with the WIN-THREADS patch `patches-pending/compiler-0027-win-threads.patch`, see `WIN-THREADS-README.md`; the Firn main `compiler/src/win.rs` lists all of them). `SetTimer` is resolved at run time by `win32.fi` (`u32_once`).
4. Windows needs `lib/chan` threads: Windows 8+ (`WaitOnAddress`).
5. Replace direct `window.*` calls in the browser by: events from `shell_poll/shell_wait`, drawing into `frame_acquire` buffers, title/cursor/size via `shell_set_*`. All `window.*` calls on the SAME window must move to the shell thread: the app has no `Window` any more.
6. Linux/X11 twin works but only polls every 4 ms and has no modal-loop benefit.
7. GPU (wgl/GL) windows are NOT routed through the shell thread (only the DIB/BGRX path). A GPU window stays on its own thread design.
8. Run `tests/4840`, `4841`, `4842`, `4860` after the integration; run `tools/shellthread/demo.fi` for numbers (section 8).

## 7. Known limits (HONEST)

- H1 The modal loop was driven by keyboard `SC_MOVE` posted by the demo, not by a mouse drag. A hand drag/resize with the mouse was not measured. Wine did not enter the modal loop (`drag_starts=0`).
- H2 The ping measures that the window procedure answers, not frame latency; events reach the app at its own pace.
- H3 No frame scaling: a smaller frame is cropped/padded with a grey gap (`GAP_COLOUR`), a bigger one is cut. DPI changes arrive as events, the app must re-render.
- H4 The wake event handle is not closed at `shell_stop` (one handle leaked per shell).
- H5 `EV_MOTION` events are dropped when the event ring is more than half full (counted in `STAT_EVDROP`); other events are dropped only if the ring is full.
- H6 x86-64 Windows only (Wine, one laptop). Not tested on aarch64, OrientOS or Android (no hook there; the twin polls).
- H7 GPU windows (wgl) are not routed through the shell thread.
- H8 The shell thread has no own GC safepoints: a GC-allocating callback inside the hook would break the rules of section 4.
- H9 At most 4 frame buffers exist; when all are in flight `shell_frame_acquire` returns 0 and the app must skip that frame.
- H10 A title is limited to 112 bytes; commands are `try_send` and fail (`false`) when the 64 slot ring is full.
- H11 The frame being shown is held by the shell until a NEWER frame arrives (it must repaint it on expose). So with `MAX_BUFS` = 4 the app effectively has 3 buffers while a frame is on screen; a single submitted frame never comes back (test 4841 submits two).

## 8. Measurements

Demo: `tools/shellthread/demo.fi`. The app thread paints a gradient + `gc_collect()` + `thread_sleep(500)` per loop; a ping thread sends `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG, 2000)` every ~10 ms and times the answer. Build: `firnc --target=x86_64-windows --opt-level=release-fast tools/shellthread/demo.fi -o demo.exe`. Run: `demo.exe --selftest [--move]` (shell thread; `--move` drives the keyboard `SC_MOVE` modal loop), `demo.exe --direct --short` (window on the app thread).

| run | mode | samples | p50 | p95 | p99 | max |
|---|---|---|---|---|---|---|
| HOLO-LAPTOP-3 (Win, i7-1185G7), 6 s, first measurement | shell thread | 380 | 0.24 ms | **0.54 ms** | 0.81 ms | 1.03 ms |
| HOLO-LAPTOP-3, 6 s, `--move`, first measurement (drag_starts=1) | shell thread | 380 | 0.26 ms | **0.59 ms** | 0.96 ms | 1.48 ms |
| HOLO-LAPTOP-3, 23.10.2026 re-run, laptop under 37-57 % load, 0 timeouts, drag_starts=1 | shell thread | - | 0.55-0.66 ms | **1.06-1.52 ms** | - | 2.7-4.0 ms |
| HOLO-LAPTOP-3, 3 s | window on the app thread | 7 | 492 ms | 2000 ms (timeout) | - | 2000 ms |
| Wine 8 + Xvfb, 3 s | shell thread | 290 | 0.22 ms | 0.40 ms | 0.52 ms | 0.54 ms |
| Wine 8 + Xvfb, 3 s | window on the app thread | 7 | 493 ms | 2000 ms (timeout) | - | 2000 ms |

Reading: the window answers within about a millisecond even though the app is blocked 500 ms per loop; without the shell thread it answers once per frame and runs into the 2 s hang limit. The higher numbers of the re-run are the laptop load, not a change in the code.

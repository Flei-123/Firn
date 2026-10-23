# Shell thread (lib/window/shellthread.fi)

The window lives on its own thread. The app thread can block for 500 ms (long frame, GC, disk) and the window still answers, repaints and resizes. On Windows the modal drag/resize loop no longer freezes with the app.

## Thread model

- **Shell thread**: creates the OS window, runs the message loop, forwards events, applies commands, blits the newest frame. Started with `thread_start(shellproto.SHELL_KIND, blk)`.
- **App thread**: everything else. Talks to the shell only through four `chan.chan` rings (`docs/CHAN.md`), lock-free.
- Wiring (a library cannot define `__thread_work`): `if kind == shellproto.SHELL_KIND { return shellthread.shell_main(arg) }`.

| channel | direction | cap | record |
|---|---|---|---|
| ev | shell -> app | 512 | 8 words: kind (`window.EV_*` or `SE_DRAG`=100), a, x, y, state, dpi, seq, t_ms (monotonic ms) |
| cmd | app -> shell | 64 | 16 words: kind, args; `CMD_TITLE` carries up to 112 bytes inline |
| frm | app -> shell | 8 | ptr, w, h, stride, id |
| free | shell -> app | 16 | ptr, id |

## API (app thread)

- `shell_start(title: str, w, h) -> u64` handle (0 = failed). `shell_wait_open(sh, ms) -> bool` (false = window failed or timeout).
- `shell_poll(sh, rec) -> bool`, `shell_wait(sh, rec, ms) -> u64` (`chan.RECV_OK/TIMEOUT/CLOSED`).
- `shell_frame_acquire(sh, w, h) -> u64` BGRX pixel pointer (0 = all 4 buffers in flight), `shell_frame_submit(sh, ptr, w, h) -> bool`.
- `shell_set_title(sh, str)`, `shell_set_cursor(sh, shape)`, `shell_set_size(sh, w, h)`, `shell_redraw(sh)`, `shell_close(sh)`.
- `shell_stop(sh)`: stops the thread, waits, frees buffers, channels and block. `shell_stat(sh, which)` with `STAT_SHOWN/DROPPED/PAINTS/EVDROP`; `shell_width/height/dpi/hwnd/state`.

## Frame ownership

- A buffer is `rt.heap_alloc(64 + w*h*4)`; the app gets the pixel pointer (after the 64-byte head). At most `MAX_BUFS` (4) exist.
- `acquire` -> app owns -> `submit` -> shell owns -> shell hands the older frame back through `free` when a newer one arrives.
- Allocation and release happen only on the app thread. The shell keeps the last shown frame and repaints it on expose/resize/drag timer. Older queued frames are dropped (`STAT_DROPPED`).
- The shell does not scale: a smaller frame is cropped/padded with a grey gap, a bigger one is cut.

## GC rules

- The shell thread calls `thread_blocking_an()` first and stays BLOCKED until it ends: the collector never waits for it, not even in the Win32 modal loop.
- So the shell thread must not allocate on the GC heap or hold GC pointers. Everything is `rt.heap_alloc` or its own stack; only `chan_try_send/try_recv` (never blocking send/recv).
- `lib/window/win32.fi` has no GC allocation (checked by grep: only one `heap_alloc`).

## Windows specifics

- `shellos.windows.fi`: auto-reset event (`CreateEventW`/`SetEvent`), wait with `MsgWaitForMultipleObjectsEx(1, &evt, 250, QS_ALLINPUT, MWMO_INPUTAVAILABLE)`.
- `win32.fi` hook `r_paint_hook`: after WM_PAINT/SIZE/MOVE/WINDOWPOSCHANGED/EXITSIZEMOVE/TIMER the shell drains events, commands and frames, also INSIDE the modal drag/size loop. `r_timer(8 ms)` runs only while dragging. A busy flag prevents re-entry (`SetWindowPos` from a command triggers WM_SIZE synchronously).
- `shellos.fi` is the Linux twin: no hook, no event (polls every 4 ms), X11 backend. Works, but without the Windows modal-loop benefit.

## Measurement (tools/shellthread/demo.fi, Windows exe)

App thread: gradient frame + `gc_collect()` + `thread_sleep(500)` per loop. Ping thread: `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG, 2000)` every ~10 ms.

| run | mode | samples | p50 | p95 | p99 | max |
|---|---|---|---|---|---|---|
| HOLO-LAPTOP-3 (Win, i7-1185G7), 6 s | shell thread | 380 | 0.24 ms | **0.54 ms** | 0.81 ms | 1.03 ms |
| HOLO-LAPTOP-3, 6 s, keyboard SC_MOVE modal loop (drag_starts=1) | shell thread | 380 | 0.26 ms | **0.59 ms** | 0.96 ms | 1.48 ms |
| HOLO-LAPTOP-3, 3 s | no shell thread (window on the app thread) | 7 | 492 ms | 2000 ms (timeout) | - | 2000 ms |
| Wine 8 + Xvfb, 3 s | shell thread | 290 | 0.22 ms | 0.40 ms | 0.52 ms | 0.54 ms |
| Wine 8 + Xvfb, 3 s | no shell thread | 7 | 493 ms | 2000 ms (timeout) | - | 2000 ms |

Run: `demo.exe --selftest [--move]`, `demo.exe --direct --short`.

## HONEST

- H1 The modal loop was driven by keyboard `SC_MOVE` posted by the demo, not by a mouse drag. A hand drag/resize with the mouse was not measured. Wine did not enter the modal loop (`drag_starts=0`).
- H2 The ping measures that the window procedure answers, not frame latency; events reach the app at its own pace (it sleeps 500 ms in the demo).
- H3 No frame scaling; DPI changes arrive as events, the app must re-render.
- H4 The wake event handle is not closed at `shell_stop` (one handle leak per shell).
- H5 `MOTION` events are dropped when the event ring is more than half full (counted in `STAT_EVDROP`); other events are dropped only if the ring is full.
- H6 Tested on x86-64 Windows (Wine, one laptop). Not on aarch64, not on OrientOS/Android (no hook there; the stub twin polls).
- H7 GPU windows (wgl) were not routed through the shell thread; only the DIB path.
- H8 The shell thread has no own GC safepoints: a GC-allocating callback inside the hook would break the rule above.

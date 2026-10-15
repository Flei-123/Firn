# Window extras (lib/window/extras.fi)

Small helpers for an always-on-top tool window (the JARVIS mini chat box). Platform neutral API in
`lib/window/extras.fi` (`import window.extras`); the X11 half is at the end of `lib/window/x11.fi`, the Win32 half at
the end of `lib/window/win32.fi`. Android/Osum do not link the module.

| Function | X11 | Win32 |
|---|---|---|
| `set_topmost / is_topmost` | `_NET_WM_STATE_ABOVE` (client message with a WM, property by hand before map / without WM) | `SetWindowPos(HWND_TOPMOST)`, `WS_EX_TOPMOST` |
| `hide / show / is_visible` | UnmapWindow + ICCCM withdraw / MapWindow | `ShowWindow(SW_HIDE / SW_SHOW)` |
| `set_frameless` | `_MOTIF_WM_HINTS` | style `WS_POPUP`, client size kept |
| `work_area` | `_NET_WORKAREA` (current desktop), else screen | `SPI_GETWORKAREA` |
| `outer_rect / move_to / set_height` | client origin minus `_NET_FRAME_EXTENTS`; ConfigureWindow (x, y = OUTER corner) | `GetWindowRect` / `SetWindowPos` |
| `place_corner(corner, margin)` | CORNER_TL/TR/BL/BR of the work area | same |
| `fit_height(corner, margin, content_h, min_h, max_h)` | clamps (max 0 = work area), keeps the anchor corner (bottom right grows upward) | same |
| `hotkey_set(id, mods, key) / hotkey_clear` | `GrabKey` on the root window (Caps/NumLock ignored), event `window.EV_HOTKEY` (`ev_a` = id) | `RegisterHotKey` (loaded at run time), `WM_HOTKEY` -> `EV_HOTKEY` |

`mods` = `window.HK_ALT | HK_CTRL | HK_SHIFT | HK_SUPER`; `key` = ASCII letter/digit or `window.HKEY_*` (Space, Tab, Return,
Escape, F1..F12). Max 8 hotkeys per window. A combination owned by another program is refused (`false`).

Tests: `tests/3541_window_extras_x11.fi` (private Xvfb, run without and under openbox; xprop/xwininfo/xdotool optional),
`tools/window/extras_wine.sh` (Win32 half under Wine).

Limits: Wayland-only sessions (XWayland ignores topmost and key grabs); under a WM the state is a request, read it back with
`is_topmost`; Wine 8 refuses `RegisterHotKey`, so the Win32 hotkey path is not verified yet (needs a real Windows run).

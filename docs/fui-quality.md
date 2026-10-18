# fUi quality tools (W17)

## Glass on surfaces (theme token `glass`)
- Theme file key `glass = 0|1|2` (off = default, subtle, strong); range checked 0..2 (`themefile.fi`).
- `lib/fui/glasspane.fi`: `pane(c, slot, x, y, w, h, r, kind)` returns true if a glass pane was painted, false = paint the flat fill (level 0, or GPU canvas).
- Wired: tooltip (`wave2.draw_tooltip`, acrylic), title bar (`titlebar_draw`, mica), in-window modal (`kit.modal_draw`, acrylic, only at full opacity), menu levels (`menuview.mv_draw`, acrylic; the level fill is switched off for that draw).
- NOT wired: the standalone message/colour/font/file dialogs (own native window, nothing behind them), menus in own X windows (`mv_present`), GPU canvas.
- Tests: `tools/fui/glasspane_main.fi check|shot <dir>|bench`; menu: `menuview_main` (`run_glass`); goldens `tools/fui/golden/glasspane-<level>-<a|b>.png` (a = title bar + tooltip, b = modal).

## Fuzz + soak
- `tools/fui/fuzz_main.fi [events] [seed]`: random pointer / key / char / tick events plus dialogs into a window with menu bar, tooltip, context menu, checkbox, toggle, slider, spin box, drop-down, expander, entry, text area; repaint every 5th event.
- Pass = no crash and VmRSS grows by less than 6 MB after the first quarter. In `run.sh` section 14d (100000 + 20000 events).

## Test recorder / live preview (W18-E: building blocks built)
- `lib/fui/recorder.fi`: line log `<kind> <x> <y> <arg> <ms>` (M move, D down, U up, L leave, C cancel, K key, H char, T tick, F frame marker); `rec_move/rec_down/...` record AND forward to `fuiwirt`; `replay_with(text, n, ud, cb)` / `replay(text, n, host)` play it back; `hash_pixels` = FNV-1a of a frame. Bad line -> replay stops, returns -(line). Test `tools/fui/recorder_main.fi`: 600 seeded random events + 85 frame markers are recorded in app A, replayed into a fresh app B -> frames pixel-identical, replay twice = same hash. Frame markers are REQUIRED for identical hover/layout state (the host repaints between events).
- `lib/fui/hotreload.fi`: `Watch` (mtime+size, a change counts after two equal looks = settled; deleted files do not trigger; `watch_poll` rate-limited to 250 ms) and `State` (key=value i64 table, stable keys <= 31 chars, atomic save via `fs.write_atomic`). `tools/fui/hotreload.sh <main.fi> [state]` rebuilds + restarts on change (old program stays when the build fails), sets `FUI_STATE`. `sh tools/fui/hotreload.sh --test` runs `hotreload_main`.
- Not done: host hook that records a real session (`FUI_RECORD=<file>` in apphost), capturing scroll/focus/selection of an App into `State`, trying the loop on a real app.

## Ripple, splitter, XRandR (W18-E)
- `lib/fui/ripple.fi` + `ripple_main`: time-based circle (decelerate curve 450 ms, fade 300 ms), 4 slots, off switch; wired into buttons via `Shape.ripple` (theme key `ripple = 1`, default off) and `appctl.ripple_on`; `ripplewire_main` checks it.
- `lib/fui/splitter.fi` + `splitter_main`: drag with grab offset, grab margin, min sizes, keyboard, RTL, a11y values; not a scene node yet.
- X11 refresh rate: `RRGetScreenInfo` rate (octet 26) -> `window.refresh_hz` -> `apphost` calls `host_set_refresh_hz` (unless FUI_REFRESH_HZ is set). Xvfb reports 0 = unknown -> stays 60.
- Sheet: `tools/fui/ecosystem_main.fi check|shot <dir>`.

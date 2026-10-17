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

## Test recorder / live preview (concept only, not built)
- Recorder (r159): wrap `fuiwirt.host_pointer_*`, `host_key`, `host_char`, `host_tick` and append `kind x y btn ms` lines; replay = the same calls from the file, then a pixel hash. `fuzz_main.fi` is the generator half (seeded: same seed = same run).
- Hot reload (idea): rebuild on file change and reopen with saved scroll/focus; needs a state dump keyed by stable node ids. Not prototyped.

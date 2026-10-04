# The fUi app kit (`lib/fui/kit.fi`)

What a launcher-style program needs on top of the desktop controls of
`wave2.fi` / `wave3.fi`. One file, immediate mode: every frame the program
calls `*_draw(ctx, ...)` with the state it keeps, and `*_hit` answers which
piece is at a point **from the same geometry** that painted it.

## Inventory (what fUi had, what was missing)

| need | already in fUi | in the kit |
|---|---|---|
| button, checkbox, radio, toggle, slider, dropdown, scrollbar, menu, tooltip | `wave2.fi` | -- (not repeated) |
| text field, text area, undo, selection | `editor.fi`, `textarea.fi`, `render.draw_textfield` | the search field wraps them |
| list view, table, tree, card, badge, dialog frame, file/colour/date dialogs | `wave3.fi`, `listrow.fi` | -- |
| scrolling | `viewport.fi` | the tile grid and the Markdown view use it |
| Lucide symbols | `lucide.fi` + `icons.fi` (74) | 19 more (play, package, puzzle, layout-grid, library, triangle-alert, loader-circle, folder-open, gamepad-2, server, hard-drive, circle-x, file-text, rocket, sparkles, layers, list, terminal, box) |
| progress bar | `wave2.draw_progress` (no text) | bar with a label that reads on fill and track, indeterminate sweep |
| tabs | `wave2.draw_tab` (text only) | tab bar with symbol, label, count badge |
| **toast / snackbar** | -- | `toasts_*` |
| **modal with focus trap** | dialog frame + scrim in `wave3` (no trap) | `modal_*` + `ring_next` |
| **sidebar navigation** | -- | `nav_*` |
| **tile grid, avatar** | `KIND_TILE` (a button) | `grid_*`, `avatar_draw` |
| **search field with clear button** | -- | `search_*` |
| button with symbol, primary / danger | plain `KIND_BUTTON` | `button_draw` |
| Markdown | -- | `fui.markdownview` (docs/MARKDOWN.md) |

## The look

Only theme tokens (`style.TOK_*`); the launcher accent `#1bd96a` is set with
`kitcolor.kit_theme(&m, dark)` (and re-applied each frame with `kit_apply`,
because the window host's `theme_set_dark` resets the accent). An accent is
too light as text on a white page (1.7:1) -- `kitcolor.accent_text(m, bg)` pulls
it towards black/white until it has 4.5:1, `accent_ui(m, bg)` until 3:1 (WCAG 2.1
1.4.11, parts of a control), `on_color(m, fill)` picks the readable one of
`text`/`base` for a fill. Lengths are design points on a 4-point grid through
`render.len_of`. Every symbol is optional (`ICON_NONE`): the part lays itself
out without it -- the icon-free variant. The soft shadow is a few translucent
rounded rectangles, not a blur (`fui.effect`'s blur costs ~100 ms for a dialog).

## Parts

| part | draw | hit / input | state |
|---|---|---|---|
| button (`BTN_PRIMARY/SECONDARY/GHOST/DANGER`) | `button_draw(c, x, y, w, h, label, icon, kind, st)` | the program's rectangle | `ST_HOVER / ACTIVE / FOCUS / DISABLED` |
| toasts | `toasts_draw(c, q, win_w, win_h, now)` | `toasts_hit -> id, part (BODY/ACTION/CLOSE)` | `Toasts`: `toast_push(kind, text, action, now, dur)`, `toast_dismiss`, `toasts_tick` |
| modal | `modal_draw`, `modal_content_rect` | `modal_hit -> SCRIM/PANEL/CLOSE`, `modal_key -> FOCUS/CLOSE/ACTIVATE` | `Modal`: `modal_open(title, w, h, n_focus, now)`, `modal_focus`, `modal_blocks` |
| tab bar | `tabbar_draw(c, tabs, n, sel, hover, focus, x, y, w)` | `tabbar_hit`, `tabbar_key` | `Tab { icon, label, badge }` |
| sidebar | `nav_draw(c, items, n, active, hover, focus, x, y, w, h, collapsed)` | `nav_hit`, `nav_key` (skips headers) | `NavItem { icon, label, badge, header }` |
| tile grid | `grid_paint(c, g, tiles, n)` | `grid_hit/move/leave/wheel/key`, scrollbar press/drag | `TileGrid`: `grid_select`, `grid_selected` |
| avatar | `avatar_draw(c, x, y, d, seed, name, icon)` | -- | eight hues, all >= 4.5:1 for their initials |
| search field | `search_draw(c, s, x, y, w, h, hint, hover_clear)` | `search_press -> FIELD/CLEAR`, `search_key` (Esc clears), `search_char` | `Search`: `search_new/free`, `search_focus` |
| progress | `progress_draw(c, x, y, w, h, permille, label, phase_ms, kind)` | -- | `permille < 0` = indeterminate |

**The focus trap.** While `modal_blocks(m)` the program sends every key to
`modal_key` and every press to `modal_hit`; Tab / Shift+Tab walk
`ring_next(cur, n, back)` -- wrapping, never leaving the dialog -- Escape asks
to close, Enter asks to activate the focused thing (`modal_focus(m)` is its
index; the program draws its own focus ring on it with `ST_FOCUS`).

**Keys** are the numbers of `fui.editor` (`editor.KEY_*`), which is what
`fui.apphost` hands a program. **State lives in long-lived storage** (a
`static`, the heap, the program's struct): the painter keeps the text pointers
it is given, and the compiler's escape check refuses a state struct on a
frame that returns.

## Proof

* `tools/fui/kit_main.fi` (section 18p5): every part painted in light and dark,
  the pixels compared with the theme, the hit functions walked, the ring
  walked, the icon-free variants, narrow widths, and **every colour pair**
  recomputed with the WCAG 2 formula (4.5:1 text, 3:1 graphics).
* `examples/fui/launcher_kit.fi`: a window using all of it (sidebar, tabs,
  search, tile grid, progress bars, toasts, dialog, Markdown page); builds
  native and `--target=wasm32-browser` (section 18p).
* `tools/fui/kitlive.py` (section 18p6): that window on a real X server
  (Xvfb), operated with xdotool, checked with `xwd` pixels and the CPU it used
  while idle.

## Since the UI-extras wave (docs/UI_EXTRAS.md)

* **Right to left.** `kit.kit_set_rtl(true)` mirrors every part; `tools/fui/kitrtl_main.fi`
  proves the hit functions, the decorations and the keys against the left-to-right picture.
* **Touch.** `lib/fui/kittouch.fi` (tap, double tap, long press, pan, fling, pinch) and the
  `grid_touch_*` / `mdv_touch_*` calls; `tools/fui/touch_main.fi` feeds synthetic pointer streams.
* **Accessibility.** `lib/fui/kita11y.fi` describes every part into the a11y tree from the very
  state the frame paints; `examples/fui/launcher_kit.fi` audits through it
  (`tools/fui/kita11y_main.fi` compares the dump).

## Honest limits

One line of text per toast and per tile line (cut with "..."); hover does not animate; in RTL the
grid's scrollbar stays at the right and pointing symbols are not flipped; the a11y tree has no
geometry and is not connected to AT-SPI / UIA yet (a screen reader still cannot read the
window -- the tree is what it will read); touch is tested with synthetic pointer streams, not on a
phone in this wave.

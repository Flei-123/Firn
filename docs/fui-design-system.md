# fUi design system

Built-in tokens and components. Plan: one source for every value, apps read roles, never numbers.

## Layers

| Layer | Module | Content |
|---|---|---|
| Primitive | `lib/fui/ds_tokens_prim.fi` | raw scales: neutral/status/accent colours, spacing ramp, radius ramp, durations, easing curves, shadow specs |
| Semantic colour | `lib/fui/ds_tokens_color.fi` | the 20 core roles, light + dark, built from primitives |
| Semantic type | `lib/fui/ds_tokens_type.fi` | Fluent 2 / WinUI 3 type ramp |
| Semantic motion | `lib/fui/ds_tokens_motion.fi` | motion roles = duration + easing |
| Facade | `lib/fui/ds_tokens.fi` | getters on a `theme.Theme`, derived colour roles, accent switch |
| Component | `lib/fui/ds_tokens_comp.fi` | button, card, row, input, nav, infobar, toast |

`lib/fui/theme.fi` imports primitive/semantic modules: `colors_light()`, `colors_dark()`, `type_px`, `elev_*` are built from them. `palette.fi` and `theme_set_accent` keep working and tokens follow (derived roles are computed from the live theme colours). The semantic modules import nothing from `theme.fi`, so there is no cycle.

## Getters (all take the live `*mut theme.Theme`; `render.ctx_theme(c)` in a painter)

| Call | Returns |
|---|---|
| `ds_tokens.color(m, C_*)` | `0xRRGGBB`; roles >= `C_ALPHA_FIRST` are `0xAARRGGBB` overlays |
| `ds_tokens.paint(m, C_*)` | always `0xAARRGGBB` (solid roles get alpha 255) |
| `ds_tokens.space(m, SP_*)` | device px (theme scale applied) |
| `ds_tokens.radius(m, RAD_*)` | device px; follows the theme shape (`RAD_PILL` = 9999) |
| `ds_tokens.type_of(m, T_*)` | `TypeTok{size, line, weight}`; size/line include the text scale |
| `ds_tokens.elevation(m, 0..4)` | `ElevTok{dy, blur, alpha, ambient_blur, ambient_alpha}`; all 0 if the shape has no shadow |
| `ds_tokens.motion(m, M_*)` | `MotionTok{ms, ease, x1, y1, x2, y2}` (permille bezier); `ms` 0 if the shape has no motion |
| `ds_tokens.apply(m, dark, accent)` | light/dark + accent preset in one call |
| `ds_tokens.set_accent(m, idx)` / `accent_count()` / `accent_color(idx, dark)` | accent switch; index 0 = Windows blue (default) |
| `ds_tokens.on_fill(bg)` / `readable(fg, bg, toward)` | text colour for an arbitrary fill / a colour pulled toward `toward` until contrast >= 4.5 |

## Colour roles

Core (from `theme.Colors`): `C_BASE` page, `C_SURFACE` bar/panel, `C_RAISED` card/menu/dialog, `C_BUTTON`, `C_BUTTON_HOVER`, `C_BUTTON_ACTIVE`, `C_FIELD`, `C_FIELD_FOCUS`, `C_BORDER`, `C_SEPARATOR`, `C_TEXT`, `C_TEXT_MUTED`, `C_TEXT_DISABLED`, `C_TEXT_ON_ACCENT`, `C_ACCENT`, `C_SELECTION`, `C_SELECTION_TEXT`, `C_ERROR`, `C_WARNING`, `C_SUCCESS`.

Derived (computed from the live theme, follow accent/palette):

| Role | Rule |
|---|---|
| `C_ACCENT_HOVER` / `C_ACCENT_ACTIVE` | accent mixed 10% / 18% toward black (light) or white (dark) |
| `C_ACCENT_TEXT`, `C_INFO` | accent pulled toward text until 4.5:1 on the ground |
| `C_FOCUS_RING` | primary text colour (>= 3:1 on base) |
| `C_CARD`, `C_CARD_BORDER` | raised surface, separator |
| `C_INFO_BG`, `C_SUCCESS_BG`, `C_WARNING_BG`, `C_ERROR_BG` | raised surface mixed 10% with the status colour |
| `C_SUBTLE_HOVER`, `C_SUBTLE_ACTIVE` | alpha overlays (black light, white dark) |
| `C_SCRIM` | `0x4D000000` |

Accents (8): blue, teal, green, orange, red, pink, purple, gray. Each has a light and a dark value; `text_on_accent` is white or black, whichever has more contrast.

## Scales

Spacing `SP_*` (design px): NONE 0, XXS 2, XS 4, SNUDGE 6, S 8, MNUDGE 10, M 12, L 16, XL 20, XXL 24, XXXL 32, HUGE 40, HUGER 48, MAX 64.

Radius `RAD_*`: NONE, SMALL (2), CONTROL (shape.r_button), FIELD (shape.r_field), CARD and FLYOUT (shape.r_panel), WINDOW (shape.r_window), PILL.

Type `T_*` (size/line/weight): CAPTION 12/16/400, BODY 14/20/400, BODY_STRONG 14/20/600, BODY_LARGE 18/24/400, SUBTITLE 20/28/600, TITLE 28/36/600, TITLE_LARGE 40/52/600, DISPLAY 68/92/600. `theme.TS_*` keep their numbers (`TS_DISPLAY` = `T_TITLE_LARGE`).

Elevation 0..4 (dy/blur/alpha): 1 = 2/4/40, 2 = 4/8/56, 3 = 8/16/72, 4 = 16/32/88; ambient shadow 2px blur alpha 31 (level 4: 4px, 36).

Motion `M_*` (ms, easing): PRESS 50 linear, HOVER 83 linear, FOCUS 100 standard, FADE 150 standard, EXPAND 167 decelerate, COLLAPSE 83 accelerate, FLYOUT_OPEN 167 decelerate, FLYOUT_CLOSE 83 accelerate, DIALOG_OPEN 250 decelerate, DIALOG_CLOSE 167 accelerate, SLIDE 250 decelerate, PAGE 300 standard, TOAST_IN 250 decelerate, TOAST_OUT 167 accelerate. Curves: standard (0.33, 0, 0.67, 1), decelerate (0.1, 0.9, 0.2, 1), accelerate (0.9, 0.1, 1, 0.2).

## Tests

`tools/fui/ds_tokens_main.fi` (run.sh section 2a): WCAG AA (4.5:1) for every text pair in light and dark for all 8 accents, 3:1 for focus ring and accent against the page, all roles defined, accent switch, theme equals tokens, scales monotone, motion matches `anim.DUR_*`.

## Component tokens (`lib/fui/ds_tokens_comp.fi`)

All colours are `0xAARRGGBB` (alpha 0 = no fill), all lengths device px, text sizes via `ds_tokens.type_of(m, tok.text_role)`.

| Call | Struct fields |
|---|---|
| `button(m, BTN_STANDARD/PRIMARY/SUBTLE/CRITICAL)` | bg, bg_hover, bg_pressed, bg_disabled, fg, fg_disabled, border, border_disabled, ring, radius, height, pad_x, pad_y, gap, icon_px, border_w, ring_w, text_role |
| `card(m)` | bg, bg_hover, border, radius, pad, gap, border_w, elevation |
| `row(m)` | bg, bg_hover, bg_pressed, bg_selected, fg, fg_secondary, divider, indicator, height, pad_x, gap, radius, indicator_w, indicator_h, icon_px |
| `input(m)` | bg, bg_hover, bg_focus, bg_disabled, fg, fg_placeholder, fg_disabled, border, border_error, accent_line, ring, radius, height, pad_x, border_w, line_w, text_role |
| `nav(m)` | pane_bg, bg_hover, bg_selected, fg, fg_selected, indicator, pane_w, pane_w_compact, item_h, item_radius, item_pad_x, item_gap, indicator_w, indicator_h, icon_px, text_role |
| `infobar(m, SEV_INFO/SUCCESS/WARNING/ERROR)` | bg, border, icon, fg, fg_secondary, radius, min_h, pad_x, pad_y, gap, icon_px, border_w, title_role, body_role |
| `toast(m, SEV_*)` | bg, fg, fg_secondary, border, accent_bar, radius, pad_x, pad_y, gap, width, icon_px, border_w, elevation, enter_ms, exit_ms, duration_ms, title_role, body_role |
| `severity_color(m, SEV_*)` | status colour for icons and bars |

Every text/background pair of these tokens is checked at 4.5:1 for light and dark and all accents in `ds_tokens_main.fi`.

## Components

(Append the public API of each component below, one line per function.)

DS2c catalog components (immediate mode, colours and sizes via lib/fui/ds_catalog_tok.fi, which reads ds_tokens / ds_tokens_comp card, nav, row from the live theme).

- `ds_appcard`: `card_new`, `draw(k,c,x,y,w,h,icon,name,sub,meta,px,py)`, `preferred_h(c)`, `set_state(S_INSTALL|S_OPEN|S_UPDATE|S_INSTALLING|S_UNAVAILABLE)`, `set_progress(permille, -1 = unknown)`, `press`/`release`/`click` -> `ACT_*`, `key`/`text`, `set_focus`, `hit`, `button_rect`.
- `ds_navview`: `nv_new`, `add`/`add_header`/`add_footer`, `set_badge`, `set_back`, `layout(v,c,x,y,w,h)` (expanded >= 1008 dp, compact >= 641, else minimal), `draw`, `draw_overlay`, `press`/`key` -> `EV_*`, `select`/`selected`, `content_x/y/w/h`, `mode`. The body is unbounded and scrolls when it does not fit: `wheel(v,px,py,dy)`, `pan(v,px,py,dy,now_ms)` + `fling` + `tick` (touch), arrow keys / `select` keep the selected row visible, `scroll_y`/`scroll_max`. Footer stays fixed (max 4).
- Icons: `ds_icons_core` (5 icons) is always linked; every other icon needs `ds_icons_full.install()` in the app's skin function. `designlint.py` rule `ICON_NOT_INSTALLED` flags an app that references non-core icons and never installs; a draw without install prints one `ds_icons: icon needs ds_icons_full.install(): <name>` line and counts in `missing_count`.
- `ds_listdetail`: `ld_new`, `set_count`, `set_break`, `set_list_width`, `set_row`, `layout`, `begin_list`/`row_draw`/`finish_list`, `draw_chrome`, `press`/`key` -> `EV_*`, `open_detail`, `back`, `detail_x/y/w/h`, `stacked`.
- `ds_vgrid`: `grid_new`, `set_count`, `set_cell(min_w_pt,h_pt,gap_pt)`, `set_pad`, `layout`, `first`/`end_index`, `cell_rect`, `index_at`, `begin`/`finish`, `move`, `press`, `drag`/`release`, `wheel`, `pan`/`fling`, `tick`, `key`, `select`, `scroll_to`.
- `ds_vgrid` OrientOS store API: `set_count(n)` (5000+ is cheap, only visible rows are drawn); `set_cell(min_w_pt,h_pt,gap_pt)` (columns fit to the width, 1 on phones); `draw_each(g,c,name,cb,user)` with `cb(c,index,x,y,w,h,state,user)` per visible cell, `state` = `ST_SELECTED|ST_HOVERED|ST_KEYFOCUS`; hit index via `index_at(x,y)` / `press(x,y)` (`NONE` for gaps, bars, misses); `scroll_to(i)`; `key(code,mods)` (arrows, Home/End, PgUp/PgDn); `select`/`selected`; `stop` ends a running fling.
- `ds_vgrid_touch`: `touch_new`, `touch_down(t,g,x,y,us)`, `touch_move(t,g,c,y,us)` (8 dp slop, then 1:1 pan with overscroll), `touch_up(t,g,us)` -> tapped index or `NONE`; starts a fling when the finger was still moving (`FLING_MIN_PER_MS`, none after 80 ms rest). Timestamps in microseconds come from the caller (host event time; tests pass a virtual clock).
- Goldens: `tools/fui/ds_catalog_golden.sh` paints appcard (install/open/update/progress), navview (expanded/compact/minimal/overlay), listdetail (split/stacked/detail), vgrid (360/800/1240), light and dark, and diffs them against `tools/fui/golden_ds/` (limit 0.5 %).

DS2a settings, icon button, breadcrumb (immediate mode, colours/sizes via the adapter lib/fui/ds_look.fi; switch to ds_tokens there).

- `ds_settings`: `panel_new`, `begin(p,c,x,y,w,px,py)`, `end(p,c)`, `y_end(p)`, `body_height(c,plain,with_sub)`.
- `ds_settings` rows: `row_plain`, `row_chevron`, `row_toggle`, `row_combo(list "a|b")`, `row_slider(lo,hi,unit)`, `row_button(label)`, `row_value(value,chevron)` (all `(p,c,id,icon,title,sub,...)`, id < 48).
- `ds_settings` groups: `group_begin(p,c,id,icon,title,sub,body_h) -> expanded`, `group_end`, `expanded`, `set_expanded`.
- `ds_settings` input: `press`/`release`/`click` -> activated id, `fired` (consumed), `pointer(kind,x,y)` for the slider drag, `key`, `text`, `focus_id`, `set_focus`, `reset`.
- `ds_settings` state: `val`, `set_val`, `set_enabled`, `enabled`, `revision`, `row_rect`, `control_rect`, `item_rect`, `combo_open`.
- `ds_iconbtn`: `btns_new`, `begin(b,c,px,py)`, `add(b,c,id,x,y,glyph,name,big,enabled) -> size`, `add_tip(...,tip,...)`, `end(b,c)` (tooltip), `set_bounds`, `tip_shown`.
- `ds_iconbtn` input: `press`/`release`/`click`, `hit`, `key`, `text`, `fired`, `focus_id`, `set_focus`, `rect_of`, `revision`; glyph helpers `back_glyph`, `forward_glyph`, `close_glyph`, `more_glyph`.
- `ds_crumb`: `crumbs_new`, `begin(k,c,x,y,w,px,py)`, `draw(k,c,"a|b|c") -> height`, `end(k,c)` (overflow menu), `set_more_name`.
- `ds_crumb` input: `press`/`release`/`click` -> absolute segment index, `key`, `text`, `fired`, `focus_id`, `set_focus`, `hidden_count`, `menu_open`, `close_menu`, `count`, `revision`.

DS2b feedback components (immediate mode, colours via lib/fui/ds_fbtok.fi -> ds_tokens; time = caller-passed ms, virtual-clock friendly).

- `ds_infobar`: `bar_new`, `draw(b,c,x,y,w,kind,title,msg,action,closable,px,py) -> height` (kind `ds_fbtok.INFO|SUCCESS|WARNING|ERROR`), `press`/`key`/`text` -> `EV_ACTION|EV_CLOSE|EV_FOCUS|EV_NONE`, `is_open`/`set_open`, `set_focus`, `action_count`/`close_count`, `action_rect`/`close_rect`/`bar_rect`.
- `ds_toast` (wraps `kit.Toasts`): `feed_new`, `push(f,kind,title,msg,action,now_ms,dur_ms|DUR_AUTO) -> id`, `tick(f,now_ms,hover_id)` (hover and keyboard focus pause the timer), `draw`/`frame(f,c,win_w,win_h,px,py,now_ms)`, `hover_at`, `press`/`key`/`text` -> `EV_ACTION|EV_CLOSE|EV_BODY|EV_FOCUS`, `focus_newest`, `dismiss`, `state_of` (`SHOWING|CLOSING|GONE`), `live_text`/`live_rev`/`live_urgent` (live region), `ms_of_us(app.mono_us())`.
- `ds_swatch`: `swatch_new`, `set_palette(pal,names)`, `set_custom(on)`, `draw(s,c,x,y,w,label,px,py) -> height`, `press`/`key`/`text`, `selected`/`set_selected`, `rgb`/`set_rgb`, `count`, `columns`, `cell_rect`, `editing`, `revision`; `accent_palette`/`accent_names` (32 Fluent accents); hex helpers `kitcolor.parse_hex`/`hex_digits`.
- `ds_combo`: `combo_new`, `set_items("a|b|c")`, `set_rows`, `draw(cb,c,x,y,w,label,hint,px,py,win_h) -> height`, `end(cb,c)` (popup, flips above near the window bottom), `press`/`key`/`text`/`wheel`, `focus`/`blur`, `value`/`set_value`, `text_of`, `is_open`, `active_row`, `result_count`/`result_item`, `announce_text`/`announce_rev`, `revision`.
DS3b page templates (immediate mode; reference HTML + SSIM gate in tools/fui/ds_ref, golden refs in tools/fui/golden/ds_tpl).

- `ds_tpl_settings`: `new(title, apply, fire, ud)`, `page(icon,title)`, `section`, `toggle`/`combo`/`slider`/`link`/`value`/`button`(id,icon,title,sub,...), `set`/`get`, `swatch(id,title,sub)` (accent picker row, applies `ds_tokens.set_accent` live and calls `apply(ud,id,index)`), `set_texts`, `run(start,t,w,h)`; NavigationView at >= 641 px, stacked list + back below; search across all pages; live apply via `apply(ud,id,value)` / `fire(ud,id)`; example `examples/fui_apps/tpl_settings`. Icons: pass `ds_icons.gid(ds_icons_names.X)` (ids >= `ds_icons.BASE` are drawn by `kit.sym` from lib/fui/ds_icons). Logic test: `tools/fui/ds_tpl_main.fi`.
- `ds_tpl_wizard`: `new(title, draw_step, can_next, finish, ud)`, `welcome(icon,title,lines)`, `step(title,lines)`, `on_input(press,key,text)`, `set_labels`, `current`, `run(start,w,w,h)`; one task per page, progress dots, Back/Next, Finish on the last step; example `examples/fui_apps/tpl_wizard`.

DS4 gallery (`examples/fui_apps/gallery`): NavigationView app showing every token table and component, plus the four templates (settings, wizard, store, files) nested in the content pane (`gal_host.add_pane`, painted via `paint_in`). Window 1200x900. `--page N` (0-15), `--theme light|dark`, `--accent N`, `--shot file.png`. Footer: breadcrumb, accent combo, previous/next page, light/dark switch (all live). Modules: `gal_host` (registry + chrome), `gal_ui` (token helpers), `gal_p_*` (one file per page group; each exposes `register(host)`). Gate: `tools/fui/gallery_golden.sh` (32 PNG in `tools/fui/golden_gallery`, design lint must be clean).

Nesting (r590): every template exposes `paint_in(ud, ctx, x, y, w, h, px, py)` that paints into a rect (all hit rects are absolute window coordinates); `paint(ud, ctx, w, h, px, py)` is `paint_in` at 0,0. Hosts clip to the rect and pass pointer coordinates only while the pointer is inside it. Demo data: `ds_tpl_demo_store`, `ds_tpl_demo_files` (`spec()`), shared by the tpl_* apps and the gallery.

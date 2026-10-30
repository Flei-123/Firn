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

## Components

(Append the public API of each component below, one line per function.)

DS2c catalog components (immediate mode, colours via lib/fui/ds_catalog_tok.fi).

- `ds_appcard`: `card_new`, `draw(k,c,x,y,w,h,icon,name,sub,meta,px,py)`, `preferred_h(c)`, `set_state(S_INSTALL|S_OPEN|S_UPDATE|S_INSTALLING|S_UNAVAILABLE)`, `set_progress(permille, -1 = unknown)`, `press`/`release`/`click` -> `ACT_*`, `key`/`text`, `set_focus`, `hit`, `button_rect`.
- `ds_navview`: `nv_new`, `add`/`add_header`/`add_footer`, `set_badge`, `set_back`, `layout(v,c,x,y,w,h)` (expanded >= 1008 dp, compact >= 641, else minimal), `draw`, `draw_overlay`, `press`/`key` -> `EV_*`, `select`/`selected`, `content_x/y/w/h`, `mode`.
- `ds_listdetail`: `ld_new`, `set_count`, `set_break`, `set_list_width`, `set_row`, `layout`, `begin_list`/`row_draw`/`finish_list`, `draw_chrome`, `press`/`key` -> `EV_*`, `open_detail`, `back`, `detail_x/y/w/h`, `stacked`.
- `ds_vgrid`: `grid_new`, `set_count`, `set_cell(min_w_pt,h_pt,gap_pt)`, `set_pad`, `layout`, `first`/`end_index`, `cell_rect`, `index_at`, `begin`/`finish`, `move`, `press`, `drag`/`release`, `wheel`, `pan`/`fling`, `tick`, `key`, `select`, `scroll_to`.

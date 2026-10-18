# fUi mobile gap: CSS flex/grid level vs fUi (stand 18.10.2026, main bb0199156)

Goal: the mobile Jarvis web app (`/root/jarvis/public`, 412x915 CSS px) must be reproducible 1:1 with fUi.
Reference images: `tools/fui/webref/*.png` (mock data, see its README). Compare tool: `tools/fui/pixcmp.py`.
Plan owners: **M1-a** `lib/fui/lay.fi` (flex layout API), **M1-b** `lib/fui/mobile.fi` (components), **M1-c** `lib/fui/webtheme.fi` (tokens/type/themes).

Known base fact: `app.row/column/card` open boxes with justify CENTER + align CENTER (app.fi `open_box`); the app level has
NO setters for flex grow/shrink/basis, padding, margin, min/max. They exist only one layer down (`scene.node_set_*`).

## 1. Feature table

| CSS feature | fUi status (evidence) | Plan / gap |
|---|---|---|
| `display:flex`, `flex-direction` | yes: flex.fi `FlexBox`, `app.row/column` (center/center default!) | M1-a: `lay` rows/columns with CSS defaults (start/stretch) |
| `display:grid` (tracks, fr, span) | yes: grid.fi, `scene.node_set_grid/_cell`, `app.grid_box` | M1-a exposes; r148 archive row 2: auto-fill/minmax open |
| `align-items`, `align-self` | yes: `flex_set_align_items`, `scene.node_set_align_self`; no app-level setter | M1-a |
| `justify-content` (start/center/end/between/around/evenly) | yes: flex.fi `justify_*`; `scene.node_set_flex`; no app setter | M1-a |
| `flex-grow/shrink/basis` | yes: `scene.node_set_grow/_base/_flexitem` (permille); no app setter; Jarvis app wraps it in `jshell.fill/set_shown` | M1-a |
| `gap`, `row-gap/column-gap` | yes: `scene.node_set_gap`; app default `ROW_GAP` fixed | M1-a |
| `flex-wrap`, `align-content` | yes: `scene.node_set_wrap`, `flex_set_align_content` | M1-a (chips rows) |
| `min/max-width/height` | partly: `scene.node_set_bounds` (main axis only); style `width/height` fixed | **GAP** cross-axis min/max, `max-width:100%` -> roadmap |
| `width:100%` / `height:100%` | yes via `align_self STRETCH` / `grow`; no percentage | M1-a (`fill`, `fill_w`) |
| `padding` per side | yes: `scene.node_set_padding(l,t,r,b)`; style only pad_x/pad_y | M1-a app setter |
| `margin` | **GAP**: only padding + gap exist (scene.fi, flex.fi: no margin field) | roadmap: margin (per child) in flex.fi + scene + app |
| `aspect-ratio`, `fit-content`/`min-content` | **GAP** (archive row 4, r148) | roadmap |
| `position: sticky/fixed/absolute` | partly: `scene.node_set_offset` (translate), layer.fi overlays; no sticky | M1-a: header/composer via column + grow; sticky = **GAP** (roadmap) |
| `overflow:auto/scroll` | yes: `scene.node_set_viewport`, viewport.fi (fling, bar, overscroll); `vlist` | M1-a/b wrapper `scroll()` |
| `overflow:hidden` (clip a box, rounded) | unclear: no node-level clip found (only viewport, canvas.clip_push_rect) | **GAP** check + roadmap |
| pull-to-refresh | **GAP** in toolkit; Jarvis app does it by hand (`jlist.fi pull_*`) | M1-b |
| `text-overflow:ellipsis` | partly: `painter.run_draw_ellipsis` (kit labels), not for a flex text node | M1-b `label(ellipsis)` |
| `-webkit-line-clamp` | partly: `scene.node_set_text_wrap(max_lines)`; ellipsis on the last line unchecked | check in phase 2 |
| `white-space` (nowrap/pre-wrap) | partly: wrap=0 one line, wrap=n paragraph; no `pre` | M1-b |
| `text-align` | yes: style `align` (L/C/R) | -- |
| `line-height` | yes: `style_set_line_height` (permille / absolute) | M1-c tokens |
| `letter-spacing` | yes: `style_set_tracking` (signed) | -- |
| `font-weight` | partly: 300/400/700 only, no variable fonts (archive 50, r141/r192): web uses 500/600/800 | **GAP**: weights 500/600/800 -> r141 |
| `font-size`, fluid `clamp()/vw` | yes: `style_set_font_fluid` + `scene_set_view` | -- |
| font family / web fonts | partly: `font_id`; Inter/SpaceGrotesk woff2 in web; fUi loads TTF | M1-c |
| `border` (width/color/style) | yes: style border/border_w; per-side border **GAP** | M1-c (uniform ok for cards) |
| `border-radius` | yes: style radius (one value, no per-corner) | pills ok; per-corner **GAP** (bubbles with tail corner) |
| `box-shadow` | yes: style `SF_BOX_SHADOW` (offset/blur/alpha/colour) | -- |
| `linear-gradient` background | yes: style `SF_BG_GRAD` (ramp, direction) | M1-c bubble gradient |
| radial gradient / glow | yes: `SF_BG_GLOW` | -- |
| `opacity` | yes: `node_set_opacity`, style alpha | -- |
| `transition` | yes: anim.fi transreg (Hover 0-1 frame) | -- |
| `:hover` `:active` `:focus` `:disabled` | yes: StyleSet overrides (HOVER, ACTIVE, FOCUS, DISABLED, SELECTED) | M1-c |
| `env(safe-area-inset-*)` | partly: `app.set_insets`, `follow_host`, Android `hostinfo.fi` (emulator only) | M1-a/b top/bottom bars use it; real device test open |
| `@media` / container queries | partly: sizeclass.fi (600/900 dp, hysteresis), not wired into the scene tree (archive row 5) | M1-a `lay.when_width` |
| `backdrop-filter` | yes: glass (effect.fi), expensive on CPU | -- |
| `<svg>` / icons | yes: lucide 93 + uisvg | M1-b icon set of the web app |
| `contenteditable` composer (auto-grow) | partly: textarea.fi; auto-grow height `area_height_pt` | M1-b TextField multiline grow |
| Markdown + code block in bubbles | yes: markdownview.fi | M1-b `bubble` |
| keyboard avoidance | partly: wave8m `insets_lift` (field above keyboard) | real-device test |
| node budget | `SCENE_MAX 128`, `KIDS_MAX 64` per window (archive row 9, r102) | **GAP/P0**: lists must use vlist |

## 2. Archive table 'fUi Lueckenliste vs Qt/Flutter/...' (mobile-relevant rows, status then)

Layout: 2 grid auto/fr partly (P1 r148) | 4 aspect/min-max-content missing (P2) | 5 media/size classes not in scene (P1) | 7 pull-to-refresh/scroll-snap missing (P3) |
8 vlist not in the scene tree (P0 r134/r145) | 9 node capacity 128 (P0 r102) | 11 layout animation missing | 12 safe-area partly.
Widgets: 17 spinner not in appctl | 24 tabs ok | 25 nav hamburger/collapse missing | 30 Chips/Segmented/Skeleton/Carousel missing (r45) | 41 bottom-sheet missing (P2) |
31 badge/avatar/card ok | 20 textarea partial (r127) | 40 toast/modal/infobar ok.
Theme: 76 follow system dark/accent partly (r150) | 78 density partly | 80 icons: Lucide 93, no Material set | 81 token export missing (r161) | 82 elevation ok | 73-75 tokens/themes ok.
Text: 50 variable fonts/real weights missing (r141/r192) | 51 fallback chain not in painter (P0 r139: CJK/emoji in chat) | 52 colour emoji partial (r140) | 53 UAX14 done since (7654/7654) |
60 selectable labels missing | 89 text scale host readers partly.
Input: 92 touch gestures only emulator/protocol (r121/r122) | 98 clipboard Android missing | 100 on-screen keyboard Android emulator only.
Perf: 153 high density full repaint 13.2 ms (r83, phone 1080x2400).

## 3. App hand work in examples/jarvis_android -> toolkit primitive

| Hand work in the app (file) | Toolkit primitive needed |
|---|---|
| `jshell.fill/fill_stretch` (grow 1 + stretch) | `lay.fill()`, `lay.fill_w()` |
| `jshell.set_shown` (hidden keeps room -> flexitem 0,0,0,1 trick) | real `display:none` (node removed from layout) |
| `jtheme.install_rules`: ~100 lines of Style rules for pill/chip/card/tab/composer/icon button | `mobile.*` components with web look (Chip, Card, ListTile, AppBar, BottomNav, Segmented, Field) |
| `jtheme.apply_colors` + `jtokens.fi` (palette, radii, fonts, tracking by hand) | `webtheme.fi` tokens (dark/claude) from the CSS variables |
| `style_set_size(52, ICON_BTN)` magic sizes for round buttons | square/circle `icon_button(size)` |
| `jlist.fi pull_*` pull-to-refresh with own hook | `mobile.pull_refresh(list)` |
| `screen_chat.fi` custom node `app.custom(KIND_TILE) + node_set_draw` for the whole message list | vlist/markdown bubbles in the scene (node budget r102) or `mobile.bubble_list` |
| `SCREEN_PAD = 8`, per-screen `node_set_padding` (6x), `node_set_gap` (4x) | page padding + `section()` spacing tokens, safe-area padding automatic |
| 5 text fields max, 128 nodes per window (jshell contract header) | node capacity (r102), more text fields |
| Tab bar built from buttons + `set_tab_on` | `mobile.bottom_nav` with active state, badge |
| status pill / "Live" / "Offline - as of 5 min" text by hand | `mobile.pill(kind)` |

## 4. Measured (pixcmp) -- phase 2

Not yet measured: `tools/fui/appmobile_screens_main.fi` (fUi re-creation of the screens) can only be built after
`lay.fi`, `mobile.fi`, `webtheme.fi` are in main (not the case at the time of writing, `git log main`). Table to fill:

| screen | diff % | ssim | lines cand/ref | dy px | score |
|---|---|---|---|---|---|
| chat | - | - | - | - | - |
| board | - | - | - | - | - |
| tasks | - | - | - | - | - |
| devices | - | - | - | - | - |
| needs | - | - | - | - | - |
| settings | - | - | - | - | - |
| login | - | - | - | - | - |

Method: `python3 tools/fui/pixcmp.py --dir <fui-shots> tools/fui/webref --suffix -dark --out-dir <diffs>`.

## 5. Findings in the web app itself (not fUi)

- `public/app.js renderTasks`: inside `make = (t) => ...` the task object `t` shadows the i18n function `t()`, so `t('gen.delete')` throws -> the task list stays empty (real tasks). Mock server patches it in memory only.
- `public/app.js renderCommandCenter`: tasks `.map(t => ... t('hud.overdue'))` has the same shadowing: with an OVERDUE task with a due date the whole command-center render throws (devices, memory, activity cards stay empty).

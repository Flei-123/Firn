# fUi -- Roadmap "Ultimatives Toolkit" (21.10.2026)

Goal: a web-design UI (FleiLauncher, 1106 lines Svelte/CSS) in a few lines of Firn, looking the same, starting fast, small in RAM.
Roadmap items live in the JARVIS project fui, group "Ultimatives Toolkit" (r481..r493). Gap table: archive "fUi Lueckenliste vs Qt/Flutter/..." rows 157-168.

## Done in this step (levers)
| Lever | What | Where |
|---|---|---|
| 1. Glow + ground | coloured glow shadow, 135 deg gradient primary button, 3 radial lights + 64 pt grid with ellipse mask, hover lift offset | `kit.glow_rect`, `kitfx.{ground_draw,button_glow_draw,button_flat_draw,card_lift}` |
| 2. Nav + cards look | soft active nav row (button ground + short glow bar), nav spacing set by `kit.nav_look`, hair-line card borders, raised account/select panels, round account tile | `kit.nav_draw`, `kitapp.*` |
| 3. Pictures + text | cover-fitted round-cornered picture from a JPEG/PNG file, real bold face (DejaVu/Segoe/Roboto Bold) instead of faux bold, `cat3`/`str_eq` | `kitfx.pic_*`, `plat/sysfont.sysfont_load_bold`, `screen.on_attach` |
| Fix | `painter.round_rect_radial` hung when the light's centre lay outside the box (negative f64 -> usize) | `lib/fui/painter.fi` |

Measured: flei SSIM vs the web original 0.823 -> **0.905** (target >= 0.90, tools: `staff_scratch/uivgl/compare.py`, 1100x700 dark).

## Priorities and line targets (non-empty, non-comment lines, app only)
| App | now | target | what gets it there |
|---|---|---|---|
| flei | 73 (original 1106) | <= 150 | already under; keep under while adding detail dialog |
| chat | not built | <= 120 | r481 popup select + auto-bind list, message list helper |
| settings | not built | <= 120 | r481 slider with drag, toggles, select popup |
| dash | 328 | <= 120 | tile/toolbar/chart-card helpers in kitapp (r483) |
| files | 100 | <= 100 | tree + table helpers exist; keep |
| form | 26 | <= 100 | done (kitform) |
| notes | not built | <= 100 | r482 `mdedit.fi` |

| Prio | Area | Item | Roadmap |
|---|---|---|---|
| P1 | Widgets | popup select, slider hit/drag, auto-bind list | r481 |
| P1 | Widgets | `mdedit.fi` markdown editor | r482 |
| P1 | Layout DSL | line targets per app, dash helpers | r483 |
| P1 | Theming | one-call "launcher" preset + design tokens (radius/shadow/glow) | r484 |
| P1 | Theming | Inter + weights 600/650 | r485 |
| P2 | Icons | stroke width 1.9, brand icons | r486 |
| P2 | Animation | timer in screen mode: hover fade, lift, fadeUp, caret blink | r487 |
| P2 | Hot reload | `--watch` + host preview | r488 |
| P2 | Templates | `fui new <template>` | r489 |
| P2 | Docs | kit docs for kitfx/kitapp/screen/kitdata/nav_look, CSS -> kit recipe | r490 |
| P2 | a11y | a11y tree + focus ring in screen mode | r491 |
| P3 | Images | image cache, real rounded clip | r492 |
| P3 | Glass | backdrop blur in screen mode | r493 |

## Honest limits
- Hover lift is a static offset (`kitfx.card_lift`) -- no animation until the screen has a timer (r487).
- Loader icons (hammer/anvil/quilt) are Lucide stand-ins; the F logo is the Lucide zap.
- Font is DejaVu (the original falls back to DejaVu on this server too); real Inter is open.
- Windows/phone measurements: "needs laptop" (laptop offline).

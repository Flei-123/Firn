# fUi web theme (`lib/fui/webtheme.fi`)

The look of the JARVIS mobile web app (`public/style.css`) as a fUi theme: colours, radii, shadows, spacing, **type steps**, dark / light / system.
A phone UI built with it looks like the web UI without copying numbers by hand.

## Where the numbers come from

`tools/fui/gen_webtheme.py` reads `:root` (theme "fleitec"), `html[data-theme="claude"]` and a few pinned phone literals
(body 14 px, brand 1.02rem / 800 / 2 px, pill 12, header 56, bubble 18 / 6, send button 38, icon button 36) and writes
`lib/fui/webtheme_gen.fi` (constants `FLEITEC_*`, `CLAUDE_*`, `CSS_*`). Also `--json OUT` (design-token export) and `--css-vars OUT`.

```
python3 tools/fui/gen_webtheme.py            # regenerate after style.css changed
python3 tools/fui/gen_webtheme.py --check    # exit 1 on drift (run.sh does this; SKIP when the CSS file is not on the machine)
```

`tools/fui/webtheme_main.fi` holds a second copy of the literal CSS values (so a regenerated file that silently changed a token still fails).

## API

| call | what |
|---|---|
| `webtheme.install(a, MODE_DARK / MODE_LIGHT / MODE_SYSTEM)` | once after the window exists: theme colours, shape (radii 16 / 12 / 12, font 14), 14 style-sheet rules |
| `set_mode(a, m)`, `set_flavor(a, FLAVOR_FLEITEC / FLAVOR_CLAUDE)`, `set_accent(a, 0xRRGGBB)` | live; rules refreshed in place (no new rules) |
| `set_system_dark(a, dark)`, `follow_syspref(a, &sp)` | what MODE_SYSTEM follows (default dark = the web app) |
| `palette(flavor, dark) -> Pal` | pure palette struct (bg, sidebar, panel, panel_up, line, field, hover, press, txt, dim, accent, warn, ok, sel, bubble0/1) |
| `text(a, "..", TY_*) -> node` | label with a type step; wraps at the container width, starts left (class based: follows theme and text scale) |
| `style_for(ty)`, `set_ty(a, n, ty)`, `K_TY(ty)` | the same as a Style / class |
| `K_CARD`, `K_PILL`, `K_BUBBLE`, `K_HEADER`, `K_SH_SM/MD/LG` | classes for `scene.node_add_class` |
| `shadow_dy/blur/alpha`, `set_shadow(st, SH_*, dark)`, `elev_level(SH_*)` | web shadows as box-shadow / fUi elevation level (1 / 3 / 4) |
| `S4 S6 S8 S10 S12 S16 S24`, `R_SM R_MD R_CARD R_PILL R_BUBBLE`, `H_HEADER H_ICON_BTN H_SEND H_TOUCH_MIN` | scales |

## Type steps

| step | px | web weight | line | tracking | colour |
|---|---|---|---|---|---|
| `TY_BRAND` | 16 | 800 | 1.20 | +2 | accent |
| `TY_TITLE` | 20 | 700 | 1.25 | 0 | text |
| `TY_SECTION` | 16 | 600 | 1.35 | 0 | text |
| `TY_BODY` | 14 | 400 | 1.50 | 0 | text |
| `TY_BODY_STRONG` | 14 | 600 | 1.50 | 0 | text |
| `TY_CAPTION` | 12 | 400 | 1.40 | 0 | text_muted |
| `TY_MONO` | 13 | 400 | 1.45 | 0 | text, font slot 1 |

The text scale (`app.set_text_scale`) moves all steps (`text_px(a, ty)` = design size x UI scale x text scale).
**Weights:** the painter has static faces only. 400 and 700/800 are real (700+ = faux bold without a bold face); 600 is handed over as 700, 500 would draw as 400.
Variable fonts / weight faces on Android are open (r141, r252).

## Light and contrast

The web has no light theme. Light = hand-set surfaces in the same hue family (fleitec: blue-grey, claude: warm paper); text, warn, ok, accent are darkened
step by step until every pair reaches 4.5 : 1 (`fit_text`). Dark = the CSS, with two honest exceptions: claude's `#ff5d5d` / dim on the raised panel is lifted
just enough for 4.5, and the web's own pairs *white on accent button* and *accent as text on claude dark* (3.6 : 1) are kept for 1 : 1 and tested at 3 : 1.
`contrast_failures(flavor, dark)` must be 0 (tests).

## Known limits

- `effect.rrect_shadow` paints only the lower half of the shadow (by design): with the web's `lg` (blur 48) a hard edge shows beside the card (see the golden).
- Per-corner radii (bubble tail 6) are not part of the style: `R_BUBBLE_TAIL` is a constant for `markdownview.mdv_set_bubble`.
- One window per process (module state).

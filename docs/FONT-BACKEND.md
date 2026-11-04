# fUi font backend

fUi measures and draws text through one switchable layer. The layout (wrapping, heights) always runs in fUi; only the numbers and the glyph bitmaps come from the backend.

## Modes

| Mode | Value | Source of advances, kerning, vertical metrics, glyphs |
|---|---|---|
| EMBEDDED | 0 | the embedded TTF (Inter) through `font/metrics.fi` and `fui/painter.fi` `glyph_into`. Default, and the only mode tests use. |
| PLATFORM | 1 | a registered `Provider` (browser canvas on wasm32-browser) |
| NATIVE | 2 | reserved registry slot for DirectWrite / CoreText / FreeType; same `Provider` interface, not built yet |

Without a registered provider, or when a provider reports an unknown glyph, fUi falls back to the embedded face. `mode_set` ignores PLATFORM/NATIVE while no provider is registered.

## Provider interface (`lib/font/fontprov.fi`)

```
struct Provider {
    advance:  fn(cp: u32, size: f64, weight: u32) -> f64
    pair:     fn(a: u32, b: u32, size: f64, weight: u32) -> f64
    vmetrics: fn(size: f64, weight: u32, out: *mut f64) -> bool
    glyph:    fn(cp: u32, size: f64, weight: u32, slant: u32, qx: u32, buf: u64, cap: usize, meta: *mut i32) -> i64
}
```

- `advance` / `pair`: CSS-pixel widths; a pair returns the kerning delta. Results are cached in `font/platmetrics.fi`.
- `vmetrics`: writes ascent and descent into `out[0..2]`.
- `glyph`: writes an 8-bit coverage bitmap into `buf` (capacity `cap`), `meta` = width, height, left, top; returns the byte count, 0 for an empty glyph, a negative value for unknown. `qx` is the horizontal quarter-pixel offset. Bitmaps are cached in `font/platglyph.fi` and composed through `raster.raster_add_coverage`, so the glyph cache, line cache and GPU atlas work unchanged.
- Emoji, symbols, private use and control characters are never asked of the provider; they use the fallback faces.

## Registering a backend

1. Build a `Provider` from four functions.
2. `fontprov.prov_set(provider)`.
3. For each weight, `metrics.fontset_platform(set, base_set, weight)` and `painter.painter_set_font` / `painter_set_weight_face`.
4. `fontprov.mode_set(MODE_PLATFORM)` (or `MODE_NATIVE`).

`lib/plat/webfont.fi` is the reference implementation (externs `firn_web_text_*`, exports `firn_web_font_backend(mode)` and `firn_web_font_stat(i)`; JS side `demos/webdemo/webfont.js`).

## Web switch

`?fontBackend=platform|embedded`, `data-font-backend` on the loader script tag, or `localStorage firn.fontBackend`. The query wins over storage. Default embedded. Switching needs a reload (line and glyph caches are keyed per font).

## Native later

DirectWrite, CoreText or FreeType backends implement the same four functions behind `MODE_NATIVE`. Applications need no change.

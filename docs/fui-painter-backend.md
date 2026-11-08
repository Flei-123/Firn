# fUi painter: canvas, raster and font backend

Additive API so a host with its own font stack (Certus) can use `fui.painter` instead of a fork.

## Canvas and raster
- `painter_set_canvas(z, cv)`: paint onto a caller-owned canvas (browser tab, offscreen layer). `painter_swap_canvas` also returns the old one.
- `painter_raster(z)`: the painter's raster, for glyph or coverage access.

## Immediate text API
- `text`, `text_width`, `text_fits_upto`, `text_baseline_mid`, `font_ascent`, `font_descent`, `TEXT_L/M/R` sit on top of the `run_*` API.

## Font backend (`lib/fui/fontback.fi`)
- `FontBackend` holds function values: `ascent`, `descent`, `advance`, `pair_adjust`, `present`, `has`, `pick`. Each gets `(fm, ctx, ...)`.
- Default = Firn `font.metrics` (`fbk_default`). `custom == 0` skips the indirect call, so the default path costs nothing.
- `painter_set_backend(z, b)` switches on a custom backend, `painter_reset_backend(z)` restores the default.
- `painter_set_backend_ctx(z, ctx)`: opaque pointer passed to every call, e.g. a per-call `FontReq` (family, weight, fallback chain).
- `pick(fm, ctx, cp)` returns the font handle that draws `cp`; returning `fm` keeps the Firn fallback chain.
- Limit: glyph outlines still come from `ttf` through the font set returned by `pick`. Replacing outlines and the raster/canvas internals is Certus gaps 7 and 8.

Tests: `tools/fui/painter_canvas_main.fi`, `painter_textapi_main.fi`, `painter_backend_main.fi`.

# examples/codehub -- a modern landing page in fUi, native and in the browser

A dark start page of a (made-up) code platform "CodeHub": header with logo,
links and sign-in, a two-line headline (white / red), paragraph, a red and a
dark button, a row of ticks, a terminal card with window dots and git output
in a monospace face, red light and an SVG mountain silhouette behind it, and
a glass bar with four figures. Name, logo and texts are this example's own.

**One page, two platforms.** `codehub.fi` paints the page into any fUi
painter; `main.fi` puts it into an X11 window (or a PNG), `web.fi` into a
`<canvas>` (WebAssembly). The browser's pixels equal the native PNG's --
`check_web.cjs` counts 0 differing pixels at 1996x1211.

```sh
bash examples/codehub/check.sh                        # build + prove everything
firnc --opt-level=release-fast -o codehub examples/codehub/main.fi
./codehub --size=1996x1211                            # X11 window (Esc closes)
./codehub --png=page.png --size=360x800 --full        # the whole phone page
./codehub --bench=60 --size=1996x1211                 # frame times
python3 -m http.server -d examples/codehub/site 8000  # browser: http://localhost:8000/
python3 examples/codehub/bundle.py codehub.html       # ONE file, opens from disk
```

| File | What it is |
|---|---|
| `codehub.fi` | the page (generated: `build.py` = `codehub.head.fi` + `texts.py` + `codehub.body.fi`) |
| `texts.py`, `codehub.theme`, `logo.svg`, `hills.svg` | the texts, the red/black theme (checked by `lib/fui/themefile.fi`), the logo, the mountains |
| `main.fi` | native: window (`lib/plat/fuiwin.fi`), PNG, stopwatch |
| `web.fi` | browser (`lib/plat/web.fi`, application mode) |
| `site/` | `index.html`, `codehub.wasm`, the fonts; `firn.js` is `demos/webdemo/firn.js` |
| `check.sh`, `check_web.cjs` | the proof (native + headless Chromium via Playwright) |
| `bundle.py` | a single self-contained HTML file |

## What fUi got for this page (29.09.2026)

- **Font slots** (`painter_set_font_slot`, style `font_id`, `render.font_enter`):
  a monospace and a bold face next to the theme's font.
- **Wrapping flexbox measured right** (`flex.flex_measure_cross_wrap`): a
  wrapping row of fixed width reports the height of all its lines.
- **The host's ear** (`fuiwirt.host_set_hook`): raw pointer and wheel for a
  page that paints its own hover or scrolls itself.
- **More faces in the browser** (`firn_web_font_extra`, `data-fonts`).

What the page does itself (and fUi could take over later): the baked
background band, the fluid headline size, counting lines for wrapped text.

## Measured (29.09.2026, one core, software rasteriser)

| | native | Chromium (WASM) |
|---|---|---|
| frame 1996x1211 | 5.6 ms | 5.3 ms |
| frame 360x800 | 3.7 ms | 3.1 ms |
| frame 360x800 at device ratio 3 | -- | 15.7 ms |
| first picture after navigation | -- | 0.26 s (phone) .. 0.74 s (desktop), local server |

`codehub.wasm` 428 KB (155 KB gzip); fonts 1.8 MB (not subset).

## The fonts

`DejaVuSansMono.ttf` and `DejaVuSans-Bold.ttf` are DejaVu 2.37, unchanged,
from Debian's `fonts-dejavu-core` (`/usr/share/fonts/truetype/dejavu/`);
`DejaVuSans.ttf` is the one of `demos/webdemo` (see its README). Licence:
Bitstream Vera, full text in `LICENSES/Bitstream-Vera.txt`.

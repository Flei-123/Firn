# demos/webdemo -- Firn in the browser

The described page of `tools/fui/gallery9_main.fi` -- toolbar, a list of 28
rows in a scroll area, two cards with switches -- compiled to WebAssembly and
painted by fUi into one `<canvas>`. Mouse (hover, press, drag the slider and
the scroll bar), wheel and keyboard (Tab / Shift+Tab, Enter / space, the
arrows, PageUp/PageDown, Home/End) work; nothing is painted while nothing
changes.

```sh
python3 -m http.server -d demos/webdemo 8000     # then http://localhost:8000/
```

| File | What it is |
|---|---|
| `index.html` | one canvas, one script |
| `firn.js` | the loader -- the only file that is not Firn, because a browser starts WebAssembly only from JavaScript. No program logic. |
| `gallery9.wasm` | `firnc --opt-level=release-safe --target=wasm32-browser -o demos/webdemo/gallery9.wasm tools/fui/gallery9_web.fi` |
| `DejaVuSans.ttf` | the font, see below |

The Firn source of the page is `tools/fui/gallery9_web.fi` (it has to stand
next to `gallery9_main.fi`, whose page functions it imports) and the platform
side is `lib/plat/web.fi`. `tools/wasm/webdemo.sh` rebuilds the module and
proves it: headless Chromium, four screenshots against the four PNGs that
`tools/fui/run.sh --images` paints natively for the same page -- 0 differing
pixels -- and then the page operated through the browser's own input events.

URL parameters: `?w=1240&h=720` fixes the size in CSS pixels,
`?theme=light|dark` overrides the system preference, `?wasm=` and `?font=`
load other files.

## The font

`DejaVuSans.ttf` is DejaVu Sans 2.37, unchanged, as Debian ships it in
`fonts-dejavu-core` 2.37-6 (`/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`),
759,720 octets, SHA-256
`abdc775b21b1bc470d50c97e790d276f2054b7504e56e5bd3e64f48d68582322`. It is the
file `tools/fui/gallery9_main.fi` reads when it paints the reference PNGs, and
a page has no files -- so the loader fetches this copy and hands the octets to
the module. Licence: Bitstream Vera, full text in `LICENSES/Bitstream-Vera.txt`.

## Device pixels

The canvas gets `devicePixelRatio` times the CSS size in pixels, and the
ratio reaches fUi as `theme.theme_set_scale` -- the one scale `render.len_of`
reads. Fonts and everything a widget measures went through it already;
since this round `scene.fi` sends the lengths of the tree (paddings, gaps,
fixed and style sizes, basis and bounds) through `render.len_of` too, and the
page's scroll area gets its numbers through the theme as well
(`tools/fui/gallery9_web.fi`). At a ratio of 2 the page is the same page with
twice the pixels: averaged back down 2x2 it differs from the 1x reference in
0.86 % of the pixels by more than 64 of 255 (mean 1.06) -- the anti-aliasing
of glyphs rasterised at twice the size. Before `scene.fi` scaled its lengths
it was 17.4 % (mean 25.1): the text doubled, the boxes did not.

Two things are not done: the page is described once, so a ratio that changes
while the page is open (the window moved to another screen) scales fonts and
the tree but not the scroll area's numbers; and at a fractional ratio
(1.25, 1.5) the hand-computed content height of gallery9's list
(`INHALT_H`) and the sum of its individually rounded rows can differ by a
few pixels at the end of the list.

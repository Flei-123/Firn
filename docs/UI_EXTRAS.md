# UI extras (the second launcher wave)

Seven things the first wave (fUi kit, Markdown view, i18n, `std.time`, regex) left
open, each with a test and, where another implementation exists, held against it.
Everything is in `lib/`, tested in `tests/2240-2242` (library) and `tools/fui/*_main.fi`
(painted pixels, synthetic pointer streams), run by `tools/fui/run.sh` section 18q;
the parts held against other programs (`tools/qr/run.sh`, `tools/uiextras/run.sh`) are
`test.sh` section 79.

| # | what | where | proof |
|---|---|---|---|
| 1 | animated GIF / WebP that plays | `lib/fui/uianim.fi`, `ImgInfo.anim` of the Markdown view | `tools/fui/anim_main.fi`: frames and delays of Pillow, 69 checks |
| 2 | text selection and copy | `lib/fui/richtext.fi` (labels, rich text), `fui.markdownview` | `richtext_main.fi` 37, `mdextras_main.fi` 38 checks |
| 3 | the kit in the accessibility tree | `lib/fui/kita11y.fi` | `kita11y_main.fi`: audit green, dump compared, `examples/fui/launcher_kit.fi` audits through it |
| 4 | touch gestures; right to left | `lib/fui/kittouch.fi`, `kit.kit_set_rtl` | `touch_main.fi` 52, `kitrtl_main.fi` 73 checks |
| 5 | rich text, syntax highlighting | `lib/fui/richtext.fi`, `lib/highlight`, `lib/fui/syntaxcolor.fi` | `tests/2242_highlight.fi`, `mdextras_main.fi` |
| 6 | QR encoder, decoder, widget | `lib/qr/qr.fi`, `lib/qr/qrdec.fi`, `lib/fui/qrview.fi` | `tools/qr/run.sh` (below), `tests/2240_qr.fi`, `qrview_main.fi` |
| 7 | localized time, sizes, lists | `lib/i18n/human.fi` | `tools/uiextras/run.sh` against ICU 72, `tests/2241_human.fi` |

## 1. Animated pictures (`uianim`)

```firn
var a: uianim.Animation = uianim.anim_new()
uianim.anim_from_bytes(p, n, &a)            // GIF, animated or still WebP, or any picture (one frame)
uianim.anim_tick(&a, now_ms)                // true = the frame changed, repaint
uianim.anim_draw(z, &a, x, y, w, h, uiimage.FIT_CONTAIN)
uianim.anim_next_ms(&a, now_ms)             // sleep this long; -1 = nothing changes any more
```

The first tick starts the show; delays and loop counts are the file's (GIF's NETSCAPE2.0, WebP's);
a delay under 20 ms in a GIF plays as 100 ms (every browser does), a WebP delay of 0 as 10 ms.
A program that was stopped jumps to the frame the clock says instead of playing the missed ones.
Pause/resume keep the time the frame had left. The Markdown view takes an animation through
`ImgInfo.anim`; `mdv_tick` advances all of them, `mdv_next_ms` says when to wake.
**Not done:** APNG (shows its first frame), memory is every frame as a full canvas (the decoders'
limits apply).

## 2. Selection and copy

`RichText` (`lib/fui/richtext.fi`) is a label with styled spans (bold, italic, underline, strike,
code, colour, link) that wraps, paints, and selects: drag, double click = word, third click =
paragraph, Ctrl+A, Ctrl+C to a clipboard hook (`fn(ptr, n) -> bool`, the one `editor.fi` takes;
`window_clip_write` of `fuiwin` fits). The Markdown view selects by the same gestures; the copy is
text as a person expects it: blocks separated by a blank line, tight list items and code lines
by a line break, table cells by tabs, words by blanks.
**Honest:** a new width/scale/text drops the selection (the layout items it names are gone);
a selection across a direction change is by logical order; no auto-scroll while dragging outside.

## 3. The kit in the accessibility tree

Each kit part describes itself into a `scene.Scene` + `a11y.A11y` from the state the frame paints
(`kax_button`, `kax_tabbar`, `kax_nav`, `kax_grid`, `kax_search`, `kax_progress`, `kax_toasts`,
`kax_modal`, `kax_avatar`): role, name ("Installed, 12", "Sodium, Rendering engine, Installed",
"Error: Download failed"), state (selected tab, pressed tile, disabled, focused), live status for
toasts. The audit of `lib/fui/audit.fi` runs over it; the launcher example no longer lists its
parts a second time by hand. **Honest:** the tree has no geometry; connecting it to AT-SPI / UIA so
that a real screen reader reads it is the platform step that is still open.

## 4. Touch and right to left

`kittouch.fi`: one recogniser fed with the pointer records of `lib/window/pointers.fi` -> TAP,
DOUBLE_TAP, LONG_PRESS, PAN_START/MOVE/END (with a fling velocity of the last 100 ms), PINCH
(scale, angle, midpoint). The numbers are `event.fi`'s (checked by the test). Consecutive moves
are coalesced so a frame with twenty records gives one PAN_MOVE. Mouse pointers give clicks only.
The grid takes `grid_touch_pan/_fling/_tap`, the Markdown view `mdv_touch_pan/_fling/_tap/_long`.
**Not done in this wave:** a run on a phone or the Android emulator (the streams are synthetic,
like `pointers_main.fi`); swipe-to-dismiss of toasts.

`kit.kit_set_rtl(true)` mirrors every part: tab and tile order, symbol/label/badge sides, progress
fill, the sidebar's edge and mark, close buttons, labels right-aligned; the arrow keys swap; the
hit functions answer for the mirrored picture. The flag is the program's environment (set when the
language changes). **Honest:** the grid's scrollbar stays on the right, pointing symbols are not
flipped, the Markdown view has no RTL block layout.

## 5. Rich text and syntax highlighting

`lib/highlight/highlight.fi`: `hl_tokenize(h, lang, p, n, &spans)` gives spans that tile the text
exactly; rules are data (`<kind><i|-> <regex>`), run through `lib/regex` for json, toml, ini, firn,
shell, markdown, yaml and a C-like family (js, java, c, rust, ...). `fui.syntaxcolor` gives every
kind a hue pulled to 4.5:1 against the code ground (light and dark); the Markdown view colours
fenced blocks by their info string. **A finding on the way:** `regex_find` set up its machine per
call (nine `mmap`s) -- 0.4 ms per token; `regex.Matcher` (the machine kept between finds, same
results: tests 1914 and 6,000 random patterns against Python `re` unchanged) made tokenizing
10x faster (2 KB of Firn in ~14 ms). Not a parser: no nesting, no interpolation; a string ends at
its line end.

## 6. QR codes

* `lib/qr/qr.fi`: versions 1-40, levels L/M/Q/H, numeric/alphanumeric/byte (one segment), all 8
  masks (best by the four penalty rules, or forced), level boost, RGBA/text output.
* `lib/qr/qrdec.fi`: from a module matrix or a picture: Otsu / local-mean binarisation, finder
  patterns, orientation by cross product, size by timing pattern, the four outer finder corners
  and the alignment pattern for a perspective fit, five votes per module; format BCH, unmask,
  de-interleave, Reed-Solomon (Berlekamp-Massey), numeric/alnum/byte/Kanji/ECI. Mirrored codes,
  Micro QR and several codes in one picture are not read.
* `lib/fui/qrview.fi`: the code as a widget (whole-pixel modules, quiet zone, black on white on both
  themes, `qrview_contrast_ok`, an accessible name).

**Measured (`tools/qr/check_qr.py`, `check_decode.py`):** encoder: 568 cases, the module matrix is
identical with Nayuki's reference encoder (`qrcodegen`) for every version/level/mask/auto-mask/boost,
identical with python-qrcode for forced version+mask (84 of 84), and ZXing-C++ read back all 568.
(segno is compared too but only reported: it writes a zero octet instead of the pad codeword 0xEC when
one pad codeword is needed in byte mode, and chooses its mask differently.) Decoder on pictures
that get worse (clean, rotated, perspective, blurred + noisy, damaged, cluttered): ours 282/285,
349/355, 340/345 against ZXing-C++ on the same pictures 284/285, 353/355, 340/345 (seeds 2-4), no
wrong payload ever; the rotated, clean, damaged and cluttered classes read 100 %.

## 7. Localized times, sizes, lists (`i18n.human`)

`format_relative("de", -300000)` -> "vor 5 Minuten"; `format_bytes("fr", 12345678, false)` ->
"12,3 Mo" (narrow no-break space); `format_datetime(lang, t, DATE_FULL, TIME_SHORT)`;
`format_percent`; `format_list(lang, "A|B|C", '|', or)`; `format_in_zone(lang, &tz, ms, ...)` (TZif).
All locale data is the generated block of the file (`tools/uiextras/gen_human.py`, ICU 72.1 / CLDR 42);
`check_human.py` compares thousands of random inputs per kind in all eight languages with ICU
(0 differences in 8,016 cases) and the zones with Python `zoneinfo`. Units for a time distance:
now (< 5 s), seconds, minutes, hours, days (< 7), weeks (< 30 days), months of 30 days (< 360),
years; counts are cut down, never rounded up.
**Findings:** ICU's legacy `NumberFormat` (what `check_i18n.py` uses) and its current
`NumberFormatter` differ for Spanish and Polish -- CLDR's "minimum grouping digits" is 2, so
"1234" is not grouped; `human` follows the current one, `i18n.format_number` keeps the old (an
existing contract, left as it is). ICU's calendar is Julian before 1582; `std.time` is proleptic
Gregorian (the check asks ICU for that). "or" lists come from Babel (CLDR 47): PyICU 2.10 cannot ask
ICU for them. **Not done:** zone names ("CEST"), "yesterday/tomorrow" words, other languages than the
eight.

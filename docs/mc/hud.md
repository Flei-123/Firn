# mc_hud -- 2D overlay: text, crosshair, hotbar, debug lines

`lib/mc/mc_hud.fi` (+ generated `lib/mc/mc_hud_font.fi`), test `tests/2762_mc_hud.fi`, cross-check `bash tools/mc/hud/run.sh`
(`RESULT mc-hud: ok`). Draws into the colour buffer of an `mc_raster.Raster` AFTER the world; the depth buffer is not touched.
Integer maths only -> bit-identical on every build stage and in `tools/mc/hud/ref.py`.

## Rules (the contract the python reference repeats)
* Colours `0xAABBGGRR`; the alpha of a drawing colour is a blend factor: 0 = nothing, 255 = copy,
  else per channel `(src*a + dst*(255-a)) / 255` (integer division); destination alpha stays 255. Everything is clipped to the buffer.
* GUI scale `s >= 1` multiplies all sizes; `hud_scale(w,h) = clamp(min(w/320, h/240), 1, 4)` (640x360 -> 1, 1280x720 -> 3).
* Font: own free 5x7 bitmap font, ASCII 32..126 (`tools/mc/hud/font5x7.txt`, one line per glyph, `#` = pixel; `gen_font.py`
  writes `mc_hud_font.fi`, run.sh checks it is current). Other codes draw `?`. A font pixel = `s x s` pixels, advance 6*s,
  line height 10*s in the debug block. Text width of n glyphs = `(6n-1)*s`.
* Shadow: same text moved by `(s, s)` and drawn first (colour with alpha 0 = no shadow).
* Crosshair: plus of arm `4s`, thickness `s`, centred on `(w/2, h/2)`, drawn by INVERTING the pixels (visible on any background).
* Hotbar (`hud_hotbar(r, s, slot)`, `slot = Player.slot` clamped 0..8): bottom centre, `181s x 22s` bar at `y = h - 23s`
  (black alpha 120, grey divider lines 10 x `s`, outer lines), slot numbers 1..9 (grey, selected white), white frame of `2s`
  around the selected cell (`24s x 24s`, may reach `s` above the bar). Integer division truncates towards zero (bar wider than frame).
* Debug lines (`HudDebug`, `hud_debug`, 5 lines at `(2s, 2s + 10s k)`, white text + shadow on a black alpha-120 strip):
  `FPS: 60` / `XYZ: 12.350 64.000 -3.500` / `Block: 12 63 -4 (id 1)` or `Block: none` / `Face: +y` (`inside`, `-`) /
  `Yaw: 45.0 Pitch: -10.0`. Numbers: `hud_push_fixed` = round half up on |v| in integers, `?` for NaN or |v| >= 1e12, no `-0.000`.
* `hud_frame(r, s, flags, slot, &debug)`: crosshair (`HUD_CROSSHAIR`), hotbar (`HUD_HOTBAR`), debug (`HUD_DEBUG`) in this order.

## API
| Function | Meaning |
|---|---|
| `hud_rect`, `hud_invert` | blended rectangle, inverted rectangle |
| `hud_glyph`, `hud_text`, `hud_text_bytes`, `hud_text_width` | text |
| `hud_crosshair`, `hud_hotbar`, `hud_debug`, `hud_frame` | the parts and the whole overlay |
| `HudDebug`, `hud_debug_new`, `hud_debug_line(d,k,&buf)` | debug data and line text |
| `hud_push_int`, `hud_push_fixed`, `hud_scale` | helpers |

## Checked (tools/mc/hud)
`dump.fi` executes 18 scenes (all glyphs at scale 1 / 2 / 3 with and without shadow and alpha, blend levels 0 / 1 / 127 / 128 /
254 / 255 + clipping at all edges, crosshair even / odd frames at scale 1..3, hotbar slots 0 / 4 / 8 / out of range / clipped,
debug lines with rounding, -0, 1e12, 1e11, NaN-free extremes, all face cases, whole frames at 640x360 and 1280x720) on a
non-trivial base pattern and logs every operation; `ref.py` replays the log with its own code (font read from the txt file,
python ints) and requires byte-identical framebuffers + equal SHA-256. 3 build stages byte-equal; 3 counter-checks (blend,
hotbar frame, one font row) detected. Not independently checked: the look (font shapes, colours are choices).
Measured: whole overlay 0.2 ms at 640x360, 1.3 ms at 1280x720.

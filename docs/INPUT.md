# lib/input -- reading the keyboard and mouse (round gpu-c, 12.10.2026)

Injecting input (uinput / SendInput) was already there: `input.input`. This round adds READING.

## What is there
- `window.raw_input(w, true)` (lib/window): `EV_RAWKEY` / `EV_RAWKEY_UP` for EVERY key (ev_a = Linux scancode, ev_x = keysym without modifiers, ev_y = time ms, ev_state = modifiers), `EV_FOCUS`, plus release/enter/leave. The old `EV_KEY` / `EV_CHAR` follow unchanged.
- `input.raw` (lib/input/raw.fi): per frame down / pressed / released for keys and mouse buttons, wheel, position, relative delta, auto-repeat filter (X11 release+press with equal time, Windows press without release), focus loss releases everything. Pure core `raw_feed` (tests/2792: scripted + 2000 random frames vs a model).
- `input.grab` (lib/input/grab.fi) over `window.cursor_mode(CURSOR_CAPTURED)`: invisible cursor, pointer kept in the window.
  - X11: pointer grab + XInput2 RawMotion (unaccelerated); fallback warp-to-centre with the warp-echo fix. No EV_MOTION while captured.
  - Windows: ClipCursor + RegisterRawInputDevices/WM_INPUT; fallback pointer position against the middle.
  - `window.cursor_shape(w, SHAPE_*)` (X11 cursor font glyphs, Windows IDC_*); `cursor_raw` / `cursor_is_raw`.
- Server quirk found: with a pointer grab Xorg/Xvfb delivers every raw event twice; identical neighbours are dropped (a real pair of identical events in the same ms with the same request sequence is lost; only 8 kHz mice could do that).

## Proven (tools/input/read_run.sh)
Xvfb + xdotool (XTest): 5x(10,-4)+3x(-7,20) sums to exactly (29,40) in raw AND warp mode; raw key a = scancode 30, keysym 97; the grab program builds for Windows.
NOT proven: invisible cursor (visual), real mouse/keyboard, Windows at runtime (Wine run of the probe stopped at window creation, not investigated), WM_INPUT, focus events under a real WM.

## NOT done (roadmap)
- Gamepad (evdev / XInput / SDL mapping / hot-plug / rumble): `/dev/uinput` does not exist on this build machine, so no test device; not started.
- Text input / IME: `EV_CHAR` (full Unicode, X11 keysym -> code point, Windows WM_CHAR) already exists; preedit struct, compose dead keys, text-cursor rect, XIM/TSF: not started.
- evdev raw keyboard/mouse (/dev/input/event*): not done.

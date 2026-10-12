# lib/input -- reading the keyboard and mouse (round gpu-c, 12.10.2026)

Injecting input (uinput / SendInput) was already there: `input.input`. This round adds READING.

## What is there
- `window.raw_input(w, true)` (lib/window): `EV_RAWKEY` / `EV_RAWKEY_UP` for EVERY key (ev_a = Linux scancode, ev_x = keysym without modifiers, ev_y = time ms, ev_state = modifiers), `EV_FOCUS`, plus release/enter/leave. The old `EV_KEY` / `EV_CHAR` follow unchanged.
- `input.raw` (lib/input/raw.fi): per frame down / pressed / released for keys and mouse buttons, wheel, position, relative delta, auto-repeat filter (X11 release+press with equal time, Windows press without release), focus loss releases everything. Pure core `raw_feed` (tests/2793: scripted + 2000 random frames vs a model).
- `input.grab` (lib/input/grab.fi) over `window.cursor_mode(CURSOR_CAPTURED)`: invisible cursor, pointer kept in the window.
  - X11: pointer grab + XInput2 RawMotion (unaccelerated); fallback warp-to-centre with the warp-echo fix. No EV_MOTION while captured.
  - Windows: ClipCursor + RegisterRawInputDevices/WM_INPUT; fallback pointer position against the middle.
  - `window.cursor_shape(w, SHAPE_*)` (X11 cursor font glyphs, Windows IDC_*); `cursor_raw` / `cursor_is_raw`.
- Server quirk found: with a pointer grab Xorg/Xvfb delivers every raw event twice; identical neighbours are dropped (a real pair of identical events in the same ms with the same request sequence is lost; only 8 kHz mice could do that).

## Proven (tools/input/read_run.sh)
Xvfb + xdotool (XTest): 5x(10,-4)+3x(-7,20) sums to exactly (29,40) in raw AND warp mode; raw key a = scancode 30, keysym 97; the grab program builds for Windows.
NOT proven: invisible cursor (visual), real mouse/keyboard, Windows at runtime (Wine run of the probe stopped at window creation, not investigated), WM_INPUT, focus events under a real WM.

## Gamepads (round gpu-e, r420) -- lib/input/pad*.fi
- Layers: `input.padcore` (pure: evdev `input_event` 24 B / joydev `js_event` 8 B / XInput -> the logical SDL gamepad: 16 buttons `PB_*`, 6 axes `PA_*`, pressed/released edges, event ring `PE_*`), `input.padset` (8 slots + SDL GameControllerDB text), `input.padsys` (Linux: evdev first, joydev for devices evdev did not give, hot-plug by inotify on /dev/input + settle rescan; `padsys.windows.fi`: XInput via std.dynlib), `input.pad` (front door: `pad_open/pad_poll/pad_frame/pad_close`). Syscalls only on Linux (static ELF stays static).
- Mapping: `pad_apply_sdl` takes one SDL GameControllerDB line (Linux lines; `b`/`a`/`h`, `+a`/`-a` half axes, `a~` inverted, SDL evdev index order); a device without a line gets the kernel's gamepad codes (BTN_SOUTH.., ABS_RX.., hat0). `pad.pad_db_load(set, path)` adds a gamecontrollerdb.txt (lookup: full GUID, then bus+vendor+product). No database is shipped.
- Normalisation from EVIOCGABS ranges, radial deadzone for sticks (0.15), threshold for triggers (0.05), `pad_set_deadzone`. Events: connect/disconnect are never dropped (ring of 64, oldest goes). SYN_DROPPED: events up to the next SYN_REPORT are discarded and the key/axis state is re-read (EVIOCGKEY/EVIOCGABS).
- Proven (tests 2794/2795, tools/input/pad_run.sh, gate 127): byte parsing, SDL parsing, deadzones, joydev maps, XInput scaling, 4000 random events vs a model, the real non-blocking read path on a PIPE with fake bytes (random chunks, 170+ events per read, unplug = EOF), DB lookup. The Windows build of the front door compiles.
- NOT proven: any real pad (no /dev/input here), real hot-plug, permissions, XInput at runtime (not even Wine), joydev ioctls (JSIOC*), rumble / force feedback (not done), DirectInput/HID-only pads on Windows, aarch64 syscall numbers (x86_64 numbers like uinput.fi; getdents64/inotify forms not checked on aarch64).

## evdev raw keyboard / mouse (r421) -- lib/input/rawdev.fi
`rawdev_open` scans /dev/input/event* (keyboards: KEY_A+SPACE+ENTER, mice: REL_X/Y+BTN_LEFT; pads are left to input.pad), `rawdev_pump(&rd, &raw)` feeds `raw.raw_feed_ev` = the SAME state machine as the window events (repeat value 2 dropped; position only counts up from 0). Read only, no EVIOCGRAB, no focus needed (so it needs group `input`; `rawdev_denied` counts refusals). Windows twin = honest stub (use window.raw_input). Proven with fake bytes on a pipe (tests/2797, 2000 random frames); real devices NOT.

## Text input / IME -- status
NOT done this round (context ran out): preedit struct, commit events, Compose/dead keys, cursor rect, XIM (needs a second Xlib connection with its own input window: the pure-protocol window cannot receive XIM ClientMessages; synthesised XKeyEvents into XFilterEvent), IMM/TSF stub, Android IME coordination (r70). Prepared: `tools/input/gen_compose.py` generates dead-key (Unicode NFC, 541 entries) and Compose-key sequence (1147 entries from the X11 Compose file) tables for lib/input/composedata.fi (not committed, regenerate with it).

## NOT done (roadmap)
- Text input / IME (see above); `EV_CHAR` (full Unicode, X11 keysym -> code point, Windows WM_CHAR) already exists.

# AUDIO -- `audio.engine`: Aulos design in Firn

`lib/audio/engine.fi` rebuilds the C++ engine AULOS (`/root/projects/aulos`, used by Daidalos `dai_audio.cpp`)
in Firn: events, voices, buses, mute/volume per bus, master limiter, 3D (pan, distance, Doppler), parameter curves,
low pass, voice stealing, `max_instances`. Output goes to any `audio.sink` (PulseAudio, waveOut, file) via `engine_pump`,
or offline via `engine_render`. Older, simpler layer stays: `audio.mixer` (integer, 32 voices, bit exact against Python).

## Parity table (Aulos -> Firn)
| Aulos | Firn (`import audio.engine`) |
|---|---|
| `aul_create` / `aul_destroy` | `engine_new(rate, voices)` / `engine_free` (device = `audio.sink` + `engine_pump`) |
| `aul_load_bank` (JSON) | `engine_add_sample/load_wav/load_mp3`, `engine_add_event`, `engine_event` (fields), `engine_event_add_sample`, `engine_event_add_param` -- **no JSON loader yet** |
| `aul_set_listener` | `engine_set_listener(pos, forward, up, velocity)` |
| `aul_play`, `_3d`, `_on_bus`, `_3d_on_bus` | `engine_play(e, event, bus)`, `engine_play_3d(e, event, bus, x, y, z)` (`bus` -1 = event's bus) |
| `aul_stop`, `stop_all`, `is_playing` | `engine_stop(h, fade)`, `engine_stop_all(fade)`, `engine_is_playing(h)` |
| `aul_set_position/volume/pitch/parameter` | `engine_set_position/volume/pitch/param` |
| `aul_set_bus_volume/get`, `set_bus_mute/get` | `engine_set_bus_volume/get_bus_volume`, `engine_set_bus_mute/get_bus_mute` |
| `aul_ensure_bus`, `aul_bus_effective_volume` | `engine_ensure_bus`, `engine_bus_effective_volume` |
| `aul_update` | `engine_update` |
| `aul_render` | `engine_render(e, out_f32_stereo, frames)` (blocks of 128, commands apply at call start) |
| `aul_get_stats` | `engine_stats` |
| miniaudio decode (WAV, MP3, ...) | `engine_load_wav` (PCM 8/16/24/32, float32), `engine_load_mp3` (`ton.mp3`, LAME gapless like dr_mp3) |

Same: handle layout `(slot+1) | gen<<16`, slot order, stealing by priority/age, xorshift32 random, 128 frame blocks,
per-block linear gain ramps, bus fold order (deepest first), limiter (0.97 / 1 ms / 250 ms), f32 math in Aulos' order.

## Differences (honest)
* E1 no JSON bank loader (events by calls). * E2 one mutex around the API and render (Aulos: lock free command ring).
* E3 sample rate conversion = linear interpolation (Aulos: miniaudio resampler with low pass); measured max diff 5e-3 on a 440 Hz-ish tone.
* E4 limits: 16 samples/event, 8 params, 16 curve points, 512 samples, 512 events, 32 buses.
* E5 sin/cos/exp via `std.math` (f64 -> f32): not measurable in the scenarios below.
* No OGG/Vorbis (plan in `docs/MC-AUDIO-INPUT.md`), no PCM streaming voice in `audio.engine` (use `audio.mixer` + `audio.ring` for streams), no ALSA/WASAPI backend (Pulse + waveOut exist).

## Test: same input, Aulos offline vs Firn
`bash tools/audio/aulos/run.sh` builds Aulos with g++ (`-ffp-contract=off`, Aulos repo is only read) in a temp dir, runs each script in
`tools/audio/aulos/scenarios/` with the real Aulos (`aulos_offline.cpp`) and with Firn (`gen_firn.py` turns the script into a Firn program), compares.
Result (11.10.2026): `basic` (buses, mute, fades, stereo, low pass, limiter overload) and `spatial` (pan, rolloff inverse/linear, Doppler, parameters,
`max_instances`, stealing) **100.00 % bit-equal**, max diff 0; `mp3` (ffmpeg MP3, 90000 frames incl. loop) peak/RMS equal, max diff 1.1e-5 (s16 vs float decode),
99.99 % bit-equal; `resample` 44.1 -> 48 kHz max diff 5.1e-3 (E3).

# AUDIO -- `audio.engine`: Aulos design in Firn


Device/stream registry (defaults, hot-plug, volume/mute, per-app mixer, microphone mute, levels): see `docs/AUDIO-HUB.md` (`audio.hub`).
`lib/audio/engine.fi` rebuilds the C++ engine AULOS (`/root/projects/aulos`, used by Daidalos `dai_audio.cpp`)
in Firn: events, voices, buses, mute/volume per bus, master limiter, 3D (pan, distance, Doppler), parameter curves,
low pass, voice stealing, `max_instances`. Output goes to any `audio.sink` (PulseAudio, waveOut, file) via `engine_pump`,
or offline via `engine_render`. Older, simpler layer stays: `audio.mixer` (integer, 32 voices, bit exact against Python).

## Parity table (Aulos -> Firn)
| Aulos | Firn (`import audio.engine`) |
|---|---|
| `aul_create` / `aul_destroy` | `engine_new(rate, voices)` / `engine_free` (device = `audio.sink` + `engine_pump`) |
| `aul_load_bank` (JSON) | `import audio.bank`: `bank_load_file/bank_load_text` (Aulos format, comments + trailing commas), `bank_provide` (samples from memory); by hand: `engine_add_sample/load_wav/load_mp3`, `engine_add_event`, `engine_event` (fields), `engine_event_add_sample`, `engine_event_add_param` |
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
* E1 DONE: `audio.bank` reads the Aulos JSON bank (`lib/audio/bank.fi`); Daidalos buses `BUS_MASTER/MUSIC/SFX/UI`, `bank_setup_buses`, `bank_play(e, ev, bus)`. WAV/MP3 only (OGG waits for MC-H), bank must not load while the audio thread runs.
* E2 DONE (mostly): default is still ONE mutex (`engine_set_lockfree(false)`). `engine_audio_start(e, sink, chunk)` starts an audio thread and switches to LOCK FREE mode: the only shared state is a single-producer/single-consumer command ring (control -> audio), a reclaim ring (audio -> control), stream done words and aligned 32/64 bit values (bus volume/mute, stats). Positions are published with `__atomic_swap` (= `lock cmpxchg` on x86-64, ldaxr/stlxr on aarch64, a full barrier) and read with `__atomic_add(p, 0)`. **Firn HAS compare-and-swap**: `__atomic_swap(p, expected, new)` (compiler/src/thread.rs); the claim "no CAS" in lib/jobs/jobs.fi J1 and docs/JOBS.md is outdated. A real MPMC lock free queue / work stealing is therefore possible now; SPSC did not need it. The listener is a command (no torn 12 float update). Rules: one control thread, one audio thread; buses/events/samples/banks only while the audio thread is stopped (`engine_audio_stop`). The program needs the 3 line dispatcher `fn __thread_work(kind, arg) { if kind == engine.AUDIO_KIND { return engine.audio_main(arg) } ... }` and `#[allow_escape]` on the function that owns the Engine/Sink (or heap them). Only x86-64 tested (aarch64 uses the same primitive, not run).
* E3 sample rate conversion = linear interpolation (Aulos: miniaudio resampler with low pass); measured max diff 5e-3 on a 440 Hz-ish tone.
* E4 limits: 16 samples/event, 8 params, 16 curve points, 512 samples, 512 events, 32 buses.
* E5 sin/cos/exp via `std.math` (f64 -> f32): not measurable in the scenarios below.
* PCM streaming voice DONE: `engine_play_stream(e, &ring, rate, bus, volume, pan)` reads an `audio.ring` (S16LE, mono/stereo, rate/16 .. 4 x engine rate, linear resampling) on the audio thread; `engine_stream_set/stop/playing`; 16 streams; a dry ring = silence + `stats.stream_underruns`; ends after `ring_close_write` and drain. `engine_stop_all` stops streams too.
* No OGG/Vorbis (MC-H), no ALSA/WASAPI backend (Pulse + waveOut exist; WASAPI not built), no 3D/Minecraft glue yet (`lib/mc/mc_audio.fi` open).

## Cost (measured 12.10.2026, x86-64, busy host, release build, `tools/audio/bank/run.sh --bench`)
`engine_render` per 128 frame block (budget 2.67 ms at 48 kHz): 0 voices 15 us, 8 voices 45 us, 32 voices 136 us, 64 voices (3D, looping, mono) 261 us = ~10 % of the budget. Latency over PulseAudio: NOT measured (see report).

## Test: `bash tools/audio/bank/run.sh`
13 checks: JSON bank (comments, block comments, trailing commas, replaced event, bus volume/parent, errors), stream voice value, audio thread + 400 control calls from the game thread without a lost command.

## Test: same input, Aulos offline vs Firn
`bash tools/audio/aulos/run.sh` builds Aulos with g++ (`-ffp-contract=off`, Aulos repo is only read) in a temp dir, runs each script in
`tools/audio/aulos/scenarios/` with the real Aulos (`aulos_offline.cpp`) and with Firn (`gen_firn.py` turns the script into a Firn program), compares.
Result (11.10.2026): `basic` (buses, mute, fades, stereo, low pass, limiter overload) and `spatial` (pan, rolloff inverse/linear, Doppler, parameters,
`max_instances`, stealing) **100.00 % bit-equal**, max diff 0; `mp3` (ffmpeg MP3, 90000 frames incl. loop) peak/RMS equal, max diff 1.1e-5 (s16 vs float decode),
99.99 % bit-equal; `resample` 44.1 -> 48 kHz max diff 5.1e-3 (E3).

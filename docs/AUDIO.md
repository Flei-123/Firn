# AUDIO -- `audio.engine`: Aulos design in Firn

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
* No OGG/Vorbis (MC-H), no ALSA backend (Pulse + waveOut + WASAPI exist; WASAPI off by default, see "WASAPI"), no 3D/Minecraft glue yet (`lib/mc/mc_audio.fi` open).

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

## Latency (PulseAudio, measured 12.10.2026) -- `bash tools/audio/latency/run.sh`

`audio.pulse` has `pulse_open_stream_ex(p, rate, ch, name, role, tlength_ms, prebuf_ms, minreq_ms, adjust_latency)`
and `pulse_latency(p)` / `pulse_latency_info` (GET_PLAYBACK_LATENCY, the native-protocol form of `pa_stream_get_latency`:
latency = (write index - read index) / rate + sink latency). `pulse_tlength/prebuf/minreq/maxlength` return what the server
really granted. The old `pulse_open_stream` is unchanged for callers (250 ms, `music`).

The test starts a private `pulseaudio` (null sink, temp HOME/XDG_RUNTIME_DIR; killed + removed afterwards), plays a
square wave for 3 s and samples the latency every ~2 ms (about 480 samples per row). Writer "full" = write every grant
(buffer stays at tlength); "lean" = top the queue up to N ms only (a game feeding an SFX stream). All 44.1 kHz stereo.

| setting (tlength ms / adjust) | granted tlength | sink latency | latency min / mean / max (ms) | dry samples |
|---|---|---|---|---|
| 250, no adjust (the old default) | 250 ms | 210 ms | 250 / 355 / 459 | 0 |
| 100, adjust | 70 ms | 30 ms | 70 / 85 / 100 | 0 |
| 50, adjust | 37.5 ms | 12.5 ms | 37 / 44 / 50 | 0 |
| 50, no adjust | 50 ms | 25 ms | 44 / 61 / 74 | 0 |
| 30, adjust | 22.5 ms | 7.5 ms | 15 / 23 / 29 | 0 |
| **20, adjust** | 15 ms | 5 ms | **14.5 / 17 / 20** | 0 |
| 20, adjust, prebuf 0 | 15 ms | 5 ms | 14 / 17 / 20 | 0 |
| lean writer keeps 15 ms, tlength 100 | 70 ms | 30 ms | 15 / 22 / 30 | 0 |
| lean writer keeps 10 ms, tlength 50 | 37.5 ms | 12.5 ms | 10 / 15 / 20 | 2 of 458 |

Findings
* **SFX (<= 50 ms) is reachable**: `tlength` 20-30 ms with `adjust_latency = true` gives 15-23 ms mean, max 29 ms, never dry in the full-writer rows.
* `adjust_latency` is what matters: without it the sink keeps its own large buffer (25 ms at tlength 50, 210 ms at 250) and the latency is 1.5-2x.
* With `adjust_latency` the server subtracts the sink buffer from tlength (asks 50 ms, grants 37.5 ms); read the granted value back instead of assuming.
* The lean writer has the lowest latency (10-15 ms) but is the only one that ran dry (2 of 458 samples at 10 ms on a busy host): keep >= 15 ms queued and a producer that wakes at least every 5 ms.
* The server default (tlength -1) is about 2 s on this daemon: never use it for audio that must react.

Recommendation
* Music / long streams: `pulse_open_stream` (250 ms): robust, a quarter second of start delay is irrelevant.
* SFX / game mixer (`audio.engine` sink): `pulse_open_stream_ex(.., "game", 30, 0, -1, true)`: tlength 30 ms, prebuf 0, adjust_latency, and feed it from a thread that wakes every <= 10 ms (engine block 256-512 frames = 6-12 ms).
* Do not go below 20 ms: the null-sink rows have no scheduling jitter of a real device; keep a margin of two blocks.

Honest
* Null sink, not a sound card: device-side jitter, USB/Bluetooth buffers and PipeWire's own quantum (`pipewire-pulse` honours tlength with its quantum, typically 20+ ms) are not measured. The numbers are the protocol buffer + the sink's announced latency.
* Start-up: on a null sink that has been idle for a while the first sample plays up to ~2 s late (the null sink's timer sleeps for its maximum latency when idle; our guess, not verified in the PulseAudio source). `first_ms` in the table is that delay; a real card is clock driven and does not show it. Keep the sink busy (or open the stream early and cork it) if the first sound matters.
* `pactl list sink-inputs` agrees: its Buffer Latency lies inside the range the protocol client sampled in every row (checked).

## WASAPI (E4, 12.10.2026) -- `bash tools/audio/wasapi/run.sh`

`lib/audio/wasapi.windows.fi` (`import audio.wasapi`, Windows target only) is a sink backend next to waveOut:
shared mode, 16 bit PCM at the caller's rate (`AUTOCONVERTPCM | SRC_DEFAULT_QUALITY`: the system mixer converts), polling
(2 ms sleeps while the engine buffer is full). Same operations as `audio.dev` with the `wasapi_` prefix, plus
`wasapi_list` (active render endpoints by friendly name), `wasapi_latency_us`, `wasapi_buffer_frames`,
`wasapi_error_step/hresult` (which step of the open sequence failed).

No import table, no compiler change: COM through `LoadLibraryA/GetProcAddress` (ole32: `CoInitializeEx`,
`CoCreateInstance`) and the Win64 call gate (`win64_call`, the same mechanism as `lib/gpu/wgl.windows.fi`,
docs/WINDOWS_GPU.md); a method call is `call*(vtable[slot], this, ..)`. Slots used (SDK headers):
IMMDeviceEnumerator 3 EnumAudioEndpoints / 4 GetDefaultAudioEndpoint, IMMDeviceCollection 3 GetCount / 4 Item, IMMDevice 3 Activate /
4 OpenPropertyStore, IPropertyStore 5 GetValue, IAudioClient 3 Initialize / 4 GetBufferSize / 5 GetStreamLatency / 6 GetCurrentPadding /
10 Start / 11 Stop / 12 Reset / 14 GetService, IAudioRenderClient 3 GetBuffer / 4 ReleaseBuffer.

`audio.dev` (dev.windows.fi) uses it when the environment has `FIRN_WIN_AUDIO=wasapi` and falls back to waveOut if it cannot be
opened; **default stays waveOut** until WASAPI has run on a real Windows machine. The change in `dev.windows.fi` is additive
(two fields in `Dev`, an `if use_wa` at the top of each function, `wants_wasapi()`).

Tested under Wine 8.0 (winealsa -> ALSA file plugin -> raw file, no sound card), **real Windows: UNTESTED**.

| step | Wine 8.0 result |
|---|---|
| CoCreateInstance(MMDeviceEnumerator), EnumAudioEndpoints | works; one endpoint "Out: default" |
| GetDefaultAudioEndpoint, Activate(IAudioClient) | works |
| Initialize(shared, 16 bit stereo 44.1 kHz, AUTOCONVERTPCM, 40 ms) | works; engine buffer 1764 frames (40 ms) |
| GetBuffer / ReleaseBuffer / Start / GetCurrentPadding | work; 2 s pattern in blocks of 1152 / 4410 / 77 frames arrives **bit for bit** (+ up to 30 ms of trailing zeros: Wine's last period) |
| GetStreamLatency + padding | about 50 ms (10 ms stream latency + 40 ms buffer filled at the first write) |
| through `audio.sink` with `FIRN_WIN_AUDIO=wasapi` | pattern bit for bit, CRC-32 of the sink = CRC of delivered octets; waveOut default unchanged |
| no device (ALSA without PCM) | `LIST 0`, open stops at step 3 (no default endpoint, HRESULT 0x80070490 = E_NOTFOUND); clean NOSINK, no crash |

Not covered: event mode (`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`), exclusive mode, choosing a device, device change
(IMMNotificationClient), float mix formats on real Windows (AUTOCONVERTPCM should hide them), latency numbers on a real driver.

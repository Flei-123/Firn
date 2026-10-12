# AUDIO-HUB -- `audio.hub`: device and stream registry of a sound server

`lib/audio/hub.fi`, test `tests/2790_audio_hub.fi` (63 checks, all four build levels). Written for the OrientOS audio
service, but nothing in it is OrientOS specific: the PulseAudio, waveOut/WASAPI and OrientOS HD-Audio back ends all
**report** into the same hub and every UI / service **asks** it, so the policy is identical everywhere.

The hub makes no sound and opens no device. It is state + policy + the per-block gain/level step.

## What it answers

| Question | Call |
|---|---|
| which output / input devices exist, in what order, what are they called | `hub_device_count/at/name/dir/kind/rate/channels` |
| which is the default of a direction | `hub_default`, `hub_set_default` |
| volume / mute of a device | `hub_set_volume/volume`, `hub_set_mute/muted` |
| which apps play / record, their streams and owners | `hub_stream_open/close/count/at/owner/pid/device` |
| per-app mixer (volume + mute that outlive the streams) | `hub_app_set_volume/volume/set_mute/muted` |
| per-app output (or input) device | `hub_stream_move(sid, dev)` (0 = follow the default) |
| what gain does a stream get | `hub_gain_permille` (device x app x stream, 0..1000) |
| run a block of samples through it | `hub_apply(sid, ptr, frames, channels)` (S16LE, in place, returns the peak) |
| level meter | `hub_peak_peek/take`, `hub_stream_peak_take` |
| systemwide microphone mute | `hub_mic_mute`, `hub_mic_muted` |
| "microphone in use" indicator | `hub_mic_app_count`, `hub_mic_app_name(i)` |
| what changed | `hub_event_pop` (9 event kinds), `hub_gen` (one word to poll) |

## Policy

* **P1** The first device of a direction is its default. A device of **higher priority** takes the default when it appears
  (built-in 1 < HDMI 2 < jack / USB / Bluetooth 3; virtual `K_TEST` = 0 never does) -- a headset plugged in becomes the default, as on Windows.
* **P2** `hub_set_default` **pins** the choice (by name hash + direction). A pinned device stays the default against better ones; if it is
  unplugged the best remaining device takes over, and when a device of the same name returns it is the default again.
* **P3** Removing the default hands it to the best remaining device (priority, then oldest) or 0. Streams that asked for a removed device follow the default.
* **P4** Gain = device volume x app volume x stream volume (integer, permille). Any mute = 0. A muted microphone (systemwide) gives every capture stream
  silence and level 0, whatever the back end delivers -- the check is in `hub_apply`, not in the UI.
* **P5** Streams of one owner name share one app entry; a restarted player keeps its volume.

## Honest limits

* Not thread safe: one owner thread; only `hub_apply` and the peak calls may run on an audio thread, for their own stream (they write only level words).
* 16 devices, 32 streams, 16 apps, 64 queued events (oldest dropped). App entries are never evicted (16 different app names, then new apps get no entry and play at unity gain).
* Level = peak after gain, 0..32767, no RMS / decay (the UI decays).
* No resampling, no mixing: that stays in `audio.mixer` / `audio.engine`. The hub only decides *gain, mute, routing, defaults*.
* The back ends are separate: PulseAudio and waveOut are real in `dev.fi` / `dev.windows.fi`; they do not report hot-plug into the hub yet
  (a `pa_context_subscribe` listener is the next step). The OrientOS HD-Audio back end lives in the OrientOS tree (`kernel/hda.fi`, syscalls 1850-1854).

## Test

`bash test.sh` picks up `tests/2790_audio_hub.fi`. Counter-proofs (done by hand, 12.10.2026): removing the mic-mute line fails 2 checks, removing the
priority rule 5, removing the pin 8.

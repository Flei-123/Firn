# Formant synthesis (`audio.formant*`)

A singing "robot voice" built from one source (sawtooth/pulse train) and three parallel resonance filters (formants F1–F3),
plus fricatives, plosive bursts and a syllable sequencer. Inspired by the video *Formant Synthesis Makes Buzzes Sing*.
Everything is plain `f64` arithmetic with `+ - * /` (no libm, no FMA) and an integer random generator: **the samples
are identical on every platform** and can be reproduced bit-exactly with Python.

## Signal flow

```
F0 curve (Hz per sample, vibrato) -> glottis source (saw | pulse) -> 3 band-pass filters in parallel (F1 F2 F3, center + bandwidth per sample)
                                                                   -> sum (formant levels 1.0 / 0.9 / 0.7) -> * volume
Noise (xorshift, seed) -> 1 band-pass -> envelope  (s sh f z v)           \
Burst 12 ms (b d g p t k): band noise * (1-u)^2  (+ sine thump for b d g)  > the sequencer mixes everything onto one timeline
```

The band-pass filters are `synth_fx.fx_biquad` (RBJ, 0 dB at the center, `Q = F / bandwidth`), the oscillators are `synth_osc`
(PolyBLEP saw), the noise is `synth_osc.noise_fill`: none of it was rewritten.

## Modules

| Module | File | Contents |
|---|---|---|
| `audio.formant_table` | `lib/audio/formant_table.fi` | Vowel table, consonant IDs, noise bands |
| `audio.formant` | `lib/audio/formant.fi` | F0/vibrato, glottis, formant tracks, bank, `sing_vowel`, `sing_morph` |
| `audio.formant_cons` | `lib/audio/formant_cons.fi` | Fricatives, plosive burst, nasal formants |
| `audio.formant_seq` | `lib/audio/formant_seq.fi` | Sequencer (`Seq`), `seq_render`, `seq_wav` |
| `audio.formant_songs` | `lib/audio/formant_songs.fi` | `daisy_bell` (public domain) |

## Formant table

**Source:** Peterson & Barney (1952), *Control methods used in a study of the vowels*, J. Acoust. Soc. Am. 24(2),
Table V, means of the male speakers. Bandwidths: default values of the Klatt synthesizer (1980):
B1 = 60, B2 = 90, B3 = 150 Hz (F1 for `ee`/`oo`: 70 Hz).

| Name | Id | Example word | F1 | F2 | F3 |
|---|---|---|---|---|---|
| `ah` | 0 | hod | 730 | 1090 | 2440 |
| `ee` | 1 | heed | 270 | 2290 | 3010 |
| `oo` | 2 | who'd | 300 | 870 | 2240 |
| `ae` | 3 | had | 660 | 1720 | 2410 |
| `oh` | 4 | hawed | 570 | 840 | 2410 |
| `eh` | 5 | head | 530 | 1840 | 2480 |
| `ih` | 6 | hid | 390 | 1990 | 2550 |
| `uh` | 7 | hud | 640 | 1190 | 2390 |
| `uu` | 8 | hood | 440 | 1020 | 2240 |
| `er` | 9 | heard | 490 | 1350 | 1690 |

Consonants: fricatives `s` (band 7 kHz), `sh` (3.2 kHz), `f` (5.5 kHz, wide and quiet), voiced `z`, `v`;
plosives `b d g` (voiced) and `p t k` (unvoiced) by place: labial 700 Hz, alveolar 4.5 kHz, velar 1.8 kHz;
nasals `m`, `n` (F1 250, F2 1100 and 1500 Hz respectively, wide).

## API

**`formant_table`**: `vowel_f(v, k)`, `vowel_bw(v, k)`, `vowel_gain(k)`, `vowel_name(v)`, `vowel_by_name("ee")` (-1 = unknown),
`cons_by_name("sh")`, `cons_name`, `cons_is_plosive/fricative/nasal`, `cons_voiced`, `cons_center/q/gain/ms`; constants `VOWEL_*`, `CONS_*`.

**`formant`**
- `midi_to_hz(m)`, `smoothstep(u)`
- `f0_const(out, n, hz)`, `vibrato_apply(f0, n, rate, vib_rate, cents, fade_ms, depth_arr)`
- `glottis_fill(out, n, rate, SRC_SAW | SRC_PULSE, f0)`
- Tracks: `track_new(n)` (6 Arrays: F1–F3, BW1–BW3), `track_set_const(...)`, `track_set_morph(tr, n, from, to, va, vb, t0, t1, shift)` (soft transition, smoothstep)
- `formant_bank(src, out, n, rate, tr)`
- `sing_vowel(out, n, rate, f0, vib_rate, vib_cents, vib_fade_ms, vowel, shift, level)`, `sing_morph(..., va, vb, t0_ms, t1_ms, shift, level, kind)`

**`formant_cons`**: `fric_fill(out, n, rate, cons, seed)`, `burst_len(rate)` (= round(rate · 12 ms)), `burst_fill(out, n, rate, cons, seed)` (returns the length), `nasal_f`, `nasal_bw`.

**`formant_seq`**: `seq_new(rate)`, `seq_note(&s, dur_ms, hz, vowel, vowel2, cons_before, cons_after, level, legato)`,
`seq_note_midi(...)`, `seq_rest(&s, dur_ms)`, `seq_render(&s, &n) -> u64` (n `f64`, free with `synth_osc.sig_free`),
`seq_length`, `seq_to_s16`, `seq_wav(sig, n, rate, &buf)` (complete WAV file, mono S16). Fields of `Seq`:
`shift`, `vib_rate`, `vib_cents`, `vib_fade_ms`, `source`, `seed`, `peak` (normalization, 0 = off).

## Examples

```
import audio.formant
import audio.formant_table
import audio.synth_osc

// Sing a vowel: 1 s "ah" at 220 Hz with vibrato
let out: u64 = synth_osc.sig_new(44100)
formant.sing_vowel(out, 44100, 44100.0, 220.0, 5.5, 35.0, 200.0, formant_table.VOWEL_AH, 1.0, 0.5)

// Transition ah -> ee between 200 and 600 ms
formant.sing_morph(out, 44100, 44100.0, 220.0, 0.0, 0.0, 0.0, formant_table.VOWEL_AH, formant_table.VOWEL_EE, 200.0, 600.0, 1.0, 0.5, formant.SRC_SAW)

// Fricative s / sh / f, 100 ms
formant_cons.fric_fill(out, 4410, 44100.0, formant_table.CONS_SH, 1)

// Plosive k (12 ms = 529 samples at 44.1 kHz)
let len: usize = formant_cons.burst_fill(out, 4410, 44100.0, formant_table.CONS_K, 1)
```

Sequencer + WAV (demo `demos/formant/daisy_bell.fi`):

```
var s: formant_seq.Seq = formant_seq.seq_new(44100)
formant_seq.seq_note(&s, 600.0, 440.0, VOWEL_EH, VOWEL_EE, CONS_D, CONS_NONE, 0.9, true)   // "Dai"
formant_seq.seq_rest(&s, 200.0)
var n: usize = 0
let sig: u64 = formant_seq.seq_render(&s, &n)
var w: rt.Buf = rt.buf_new()
formant_seq.seq_wav(sig, n, 44100, &w)            // Header + Samples
rt.write_file("out.wav\0".p as u64, rt.buf_ptr(&w), rt.buf_len(&w))
```

Daisy Bell:

```
firnc demos/formant/daisy_bell.fi -o daisy_bell
./daisy_bell                       # writes daisy_bell.wav (mono, S16, 44100 Hz, about 51 s)
./daisy_bell out.wav --rate 22050 --lines 3 --beat 700 --pulse --shift 1.17
./daisy_bell --live                # via audio.sink (device; FIRN_AUDIO=file:x.wav works too)
```

Options: `--rate`, `--lines 1..6`, `--beat MS` (quarter note, default 800), `--pulse` (pulse train instead of saw), `--shift X`
(formant factor: 1.17 woman, 1.3 child), `--live`. The WAV file (4.5 MB) does not belong in the repo.

**Sequencer timing:** event j occupies `[b_j, b_j+1)` with `b_j = round(sum of the preceding durations · rate / 1000)`; the
total length is the rounded sum of all durations. A consonant does not lengthen the line: it takes time from the start
(before, at most 40 %) or from the end (after, at most 35 %) of its event. Plosive: closure, 12 ms burst, aspiration
afterwards for p t k. Pitch glides for 8 ms (legato 35 ms), vowels glide for 45 ms (smoothstep), vibrato fades in on every
note, non-legato notes end with an 18 ms gap.

## Tests and measurements

| Test | Checks |
|---|---|
| `4962_formant_ref_python` | Vowels (saw/pulse, vibrato, morph), fricatives, bursts: CRC-32 of the S16 samples **bit-identical** to the Python reimplementation (`tools/formant/gen_ref.py`); independently `scipy.signal.lfilter`: deviation < 1e-4 |
| `4963_formant_spectrum_seq` | Saw harmonics at k·F0 (power ~1/k², nothing in between); F1/F2 peaks of ah ee oo ae oh within ±10 %; determinism (seed); sequencer length = sum of the durations; no NaN, \|x\| ≤ 1; WAV header and CRC fixed; burst = 529 samples at 44.1 kHz (265 / 576 / 96 at 22.05 / 48 / 8 kHz), silence afterwards |

- Reference: `python3 -I tools/formant/gen_ref.py --patch tests/4962_formant_ref_python.fi` (generates the `REF-BEGIN`..`REF-END` block); `--check` verifies it, `--report` shows the deviation from scipy.
- Measured: deviation of the Firn biquad (table sine in the coefficients) from scipy with exact coefficients at most 1.2e-5 absolute (8.4e-5 of the peak value).
- Both tests behave the same in `--no-opt`, dev-fast, release-safe and release-fast; 4963 additionally under Wine (x86_64-windows) and qemu-aarch64. The Daisy Bell WAV has the same CRC-32 (3303254580) in dev-fast, release-safe and release-fast.
- Runtime (server, load varies): complete Daisy Bell (2.24 million samples at 44.1 kHz, 51 s of audio) 0.96–1.4 s wall time incl. WAV (about 2 million samples/s and more), 228 MB peak RAM (see limit 3).

## Limits

1. **No real speech:** an averaged male vowel set, three formants, no nasal anti-resonance filter, no F4/F5; `l`, `r`, `w`, `y`, `h`, `th` do not exist (omitted in the song or replaced by glides).
2. **No coarticulation:** vowel = table value, consonant = its own sound, only the 45 ms glide in between.
3. **Memory:** the sequencer renders the whole line in one piece (about 13 `f64` arrays of the line length): 30 s at 44.1 kHz need about 140 MB. Cut long pieces into parts.
4. The pulse train sits on whole samples (jitter < 1 sample); filter coefficients are recomputed every 8 samples (`synth_fx`).
5. **Daisy Bell melody transcribed from memory:** the beginning ("Dai-sy, Dai-sy") and the pitch range are correct, the later lines are an approximation with a matching text rhythm. Compare with the 1892 score (IMSLP) before serious use.

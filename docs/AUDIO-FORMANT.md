# Formant-Synthese (`audio.formant*`)

Eine singende "Roboter-Stimme" aus einer Quelle (Sägezahn/Impulszug) und drei parallelen Resonanzfiltern (Formanten F1–F3),
dazu Zischlaute, Plosiv-Bursts und ein Silben-Sequenzer. Vorbild: das Video *Formant Synthesis Makes Buzzes Sing*.
Alles ist reine `f64`-Rechnung mit `+ - * /` (kein libm, kein FMA) und ein ganzzahliger Zufallsgenerator: **die Samples
sind auf jeder Plattform gleich** und mit Python bitgenau nachrechenbar.

## Signalfluss

```
F0-Kurve (Hz je Sample, Vibrato) -> Glottis-Quelle (Säge | Impuls) -> 3 Bandpässe parallel (F1 F2 F3, Mitte + Bandbreite je Sample)
                                                                   -> Summe (Formantpegel 1.0 / 0.9 / 0.7) -> * Lautstärke
Rauschen (xorshift, Seed) -> 1 Bandpass -> Hüllkurve  (s sh f z v)           \
Burst 12 ms (b d g p t k): Band-Rauschen * (1-u)^2  (+ Sinus-Thump bei b d g)  > Sequenzer mischt alles auf eine Zeitachse
```

Die Bandpässe sind `synth_fx.fx_biquad` (RBJ, 0 dB in der Mitte, `Q = F / Bandbreite`), die Oszillatoren `synth_osc`
(PolyBLEP-Säge), das Rauschen `synth_osc.noise_fill`: nichts davon wurde neu geschrieben.

## Module

| Modul | Datei | Inhalt |
|---|---|---|
| `audio.formant_table` | `lib/audio/formant_table.fi` | Vokaltabelle, Konsonanten-IDs, Rauschbänder |
| `audio.formant` | `lib/audio/formant.fi` | F0/Vibrato, Glottis, Formant-Spuren, Bank, `sing_vowel`, `sing_morph` |
| `audio.formant_cons` | `lib/audio/formant_cons.fi` | Zischlaute, Plosiv-Burst, Nasal-Formanten |
| `audio.formant_seq` | `lib/audio/formant_seq.fi` | Sequenzer (`Seq`), `seq_render`, `seq_wav` |
| `audio.formant_songs` | `lib/audio/formant_songs.fi` | `daisy_bell` (gemeinfrei) |

## Formant-Tabelle

**Quelle:** Peterson & Barney (1952), *Control methods used in a study of the vowels*, J. Acoust. Soc. Am. 24(2),
Tabelle V, Mittelwerte der männlichen Sprecher. Bandbreiten: Standardwerte des Klatt-Synthesizers (1980):
B1 = 60, B2 = 90, B3 = 150 Hz (F1 bei `ee`/`oo` 70 Hz).

| Name | Id | Beispielwort | F1 | F2 | F3 |
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

Konsonanten: Zischlaute `s` (Band 7 kHz), `sh` (3,2 kHz), `f` (5,5 kHz, breit und leise), stimmhaft `z`, `v`;
Plosive `b d g` (stimmhaft) und `p t k` (stimmlos) nach Ort: labial 700 Hz, alveolar 4,5 kHz, velar 1,8 kHz;
Nasale `m`, `n` (F1 250, F2 1100 bzw. 1500 Hz, breit).

## API

**`formant_table`**: `vowel_f(v, k)`, `vowel_bw(v, k)`, `vowel_gain(k)`, `vowel_name(v)`, `vowel_by_name("ee")` (-1 = unbekannt),
`cons_by_name("sh")`, `cons_name`, `cons_is_plosive/fricative/nasal`, `cons_voiced`, `cons_center/q/gain/ms`; Konstanten `VOWEL_*`, `CONS_*`.

**`formant`**
- `midi_to_hz(m)`, `smoothstep(u)`
- `f0_const(out, n, hz)`, `vibrato_apply(f0, n, rate, vib_rate, cents, fade_ms, depth_arr)`
- `glottis_fill(out, n, rate, SRC_SAW | SRC_PULSE, f0)`
- Spuren: `track_new(n)` (6 Arrays: F1–F3, BW1–BW3), `track_set_const(...)`, `track_set_morph(tr, n, from, to, va, vb, t0, t1, shift)` (weicher Übergang, smoothstep)
- `formant_bank(src, out, n, rate, tr)`
- `sing_vowel(out, n, rate, f0, vib_rate, vib_cents, vib_fade_ms, vowel, shift, level)`, `sing_morph(..., va, vb, t0_ms, t1_ms, shift, level, kind)`

**`formant_cons`**: `fric_fill(out, n, rate, cons, seed)`, `burst_len(rate)` (= round(rate · 12 ms)), `burst_fill(out, n, rate, cons, seed)` (gibt die Länge zurück), `nasal_f`, `nasal_bw`.

**`formant_seq`**: `seq_new(rate)`, `seq_note(&s, dur_ms, hz, vowel, vowel2, cons_before, cons_after, level, legato)`,
`seq_note_midi(...)`, `seq_rest(&s, dur_ms)`, `seq_render(&s, &n) -> u64` (n `f64`, mit `synth_osc.sig_free` freigeben),
`seq_length`, `seq_to_s16`, `seq_wav(sig, n, rate, &buf)` (vollständige WAV-Datei, mono S16). Felder von `Seq`:
`shift`, `vib_rate`, `vib_cents`, `vib_fade_ms`, `source`, `seed`, `peak` (Normierung, 0 = aus).

## Beispiele

```
import audio.formant
import audio.formant_table
import audio.synth_osc

// Vokal singen: 1 s "ah" bei 220 Hz mit Vibrato
let out: u64 = synth_osc.sig_new(44100)
formant.sing_vowel(out, 44100, 44100.0, 220.0, 5.5, 35.0, 200.0, formant_table.VOWEL_AH, 1.0, 0.5)

// Übergang ah -> ee zwischen 200 und 600 ms
formant.sing_morph(out, 44100, 44100.0, 220.0, 0.0, 0.0, 0.0, formant_table.VOWEL_AH, formant_table.VOWEL_EE, 200.0, 600.0, 1.0, 0.5, formant.SRC_SAW)

// Zischlaut s / sh / f, 100 ms
formant_cons.fric_fill(out, 4410, 44100.0, formant_table.CONS_SH, 1)

// Plosiv k (12 ms = 529 Samples bei 44,1 kHz)
let len: usize = formant_cons.burst_fill(out, 4410, 44100.0, formant_table.CONS_K, 1)
```

Sequenzer + WAV (Demo `demos/formant/daisy_bell.fi`):

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
./daisy_bell                       # schreibt daisy_bell.wav (mono, S16, 44100 Hz, ca. 51 s)
./daisy_bell out.wav --rate 22050 --lines 3 --beat 700 --pulse --shift 1.17
./daisy_bell --live                # über audio.sink (Gerät; FIRN_AUDIO=file:x.wav geht auch)
```

Optionen: `--rate`, `--lines 1..6`, `--beat MS` (Viertelnote, Standard 800), `--pulse` (Impulszug statt Säge), `--shift X`
(Formantfaktor: 1,17 Frau, 1,3 Kind), `--live`. Die WAV-Datei (4,5 MB) gehört nicht ins Repo.

**Zeitverhalten des Sequenzers:** Ereignis j belegt `[b_j, b_j+1)` mit `b_j = round(Summe der Dauern davor · rate / 1000)`; die
Gesamtlänge ist die gerundete Summe aller Dauern. Ein Konsonant verlängert die Zeile nicht: er nimmt sich Zeit vom Anfang
(davor, höchstens 40 %) oder vom Ende (danach, höchstens 35 %) seines Ereignisses. Plosiv: Verschluss, 12-ms-Burst, bei
p t k danach Aspiration. Tonhöhe gleitet 8 ms (legato 35 ms), Vokale gleiten 45 ms (smoothstep), Vibrato blendet bei jedem
Ton ein, Nicht-legato-Töne enden mit 18 ms Lücke.

## Tests und Messwerte

| Test | Prüft |
|---|---|
| `4962_formant_ref_python` | Vokale (Säge/Impuls, Vibrato, Morph), Zischlaute, Bursts: CRC-32 der S16-Samples **bitgleich** zum Python-Nachbau (`tools/formant/gen_ref.py`); unabhängig `scipy.signal.lfilter`: Abweichung < 1e-4 |
| `4963_formant_spectrum_seq` | Säge-Harmonische bei k·F0 (Leistung ~1/k², nichts dazwischen); F1/F2-Spitzen von ah ee oo ae oh innerhalb ±10 %; Determinismus (Seed); Sequenzerlänge = Summe der Dauern; kein NaN, \|x\| ≤ 1; WAV-Header und CRC fest; Burst = 529 Samples bei 44,1 kHz (265 / 576 / 96 bei 22,05 / 48 / 8 kHz), danach Stille |

- Referenz: `python3 -I tools/formant/gen_ref.py --patch tests/4962_formant_ref_python.fi` (erzeugt den Block `REF-BEGIN`..`REF-END`); `--check` prüft ihn, `--report` zeigt die Abweichung zu scipy.
- Gemessen: Abweichung Firn-Biquad (Tabellen-Sinus in den Koeffizienten) gegen scipy mit exakten Koeffizienten höchstens 1,2e-5 absolut (8,4e-5 vom Spitzenwert).
- Beide Tests laufen gleich in `--no-opt`, dev-fast, release-safe und release-fast; 4963 zusätzlich unter Wine (x86_64-windows) und qemu-aarch64. Das Daisy-Bell-WAV hat in dev-fast, release-safe, release-fast dieselbe CRC-32 (3303254580).
- Laufzeit (Server, Last schwankt): Daisy Bell komplett (2,24 Mio Samples bei 44,1 kHz, 51 s Audio) 0,96–1,4 s Wandzeit inkl. WAV (ca. 2 Mio Samples/s und mehr), 228 MB Spitzen-RAM (siehe Grenze 3).

## Grenzen

1. **Kein echtes Sprechen:** ein gemittelter männlicher Vokalsatz, drei Formanten, kein Nasal-Anti-Resonanz-Filter, kein F4/F5; `l`, `r`, `w`, `y`, `h`, `th` gibt es nicht (im Lied weggelassen oder durch Gleiten ersetzt).
2. **Keine Koartikulation:** Vokal = Tabellenwert, Konsonant = eigener Klang, dazwischen nur das 45-ms-Gleiten.
3. **Speicher:** der Sequenzer rechnet die ganze Zeile am Stück (ca. 13 `f64`-Arrays der Zeilenlänge): 30 s bei 44,1 kHz brauchen etwa 140 MB. Lange Stücke in Teile schneiden.
4. Impulszug liegt auf ganzen Samples (Jitter < 1 Sample); Filterkoeffizienten werden alle 8 Samples neu berechnet (`synth_fx`).
5. **Melodie von Daisy Bell aus dem Gedächtnis notiert:** der Anfang ("Dai-sy, Dai-sy") und der Tonumfang stimmen, die späteren Zeilen sind eine Näherung mit passendem Text-Rhythmus. Vor ernsthafter Nutzung mit der Partitur von 1892 (IMSLP) vergleichen.

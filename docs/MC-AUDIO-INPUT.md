# Ton, Eingabe, Scope -- Stand fuer den Minecraft-Client (Runde mcD)

## Ton (lib/audio) -- echt vs. Stub
| Teil | Stand | Beleg |
|---|---|---|
| Linux PulseAudio (nativ, eigenes Protokoll, auch PipeWire-pulse) | **echt, getestet** | `tools/desktop/audio_check.py`: echter pulseaudio, null-sink, `parec` Bit-Vergleich, pactl |
| Windows `waveOut` (winmm) | **echt, unter Wine getestet** (ALSA-File-Plugin, Bit-genau); auf echtem Windows ungetestet | `tools/desktop/audio_check_win.py` |
| WASAPI, ALSA direkt, CoreAudio, AAudio | **nicht vorhanden** (macOS/Osum: Stubs `dev_macos.fi`, `dev_osum.fi`) | `docs/DESKTOP.md` |
| MP3 | echt (`lib/ton/mp3.fi`, gegen ffmpeg) | `docs/TON1.md` |
| **OGG/Vorbis** | **fehlt** (nur MP3) | siehe Plan |
| Mixer, Ring, Sinus | **neu in dieser Runde** | s. u. |

### Neu
* `audio/synth.fi`: Sinus per CORDIC, nur Ganzzahlen -> plattformgleich. **Bit-genau gegen eine unabhaengige Python-Implementierung** (13 000 Oktette) und
  max. Fehler 1.0 Schritt gegen `math.sin` (`tools/audio/check.py`).
* `audio/ring.fi`: SPSC-Ring fuer PCM-Streaming (Pos. per `__atomic_add`, Wrap-around, Close/Finished).
* `audio/mixer.fi`: 32 Stimmen, Speicher-Klaenge (One-Shot/Loop) und Ring-Streams, Lautstaerke 0-400 %, Pan/Balance, Master, Klemmung, Sperre (Futex-Mutex),
  `mixer_pump(&m, &sink, n)` fuettert einen Stereo-`Sink` nach Platz. Mathe exakt dokumentiert; Szenario (4 Stimmen, Clip, Stream, Aenderungen zwischen Bloecken)
  **bit-genau gegen Python**. Kein Resampling (M1).
* Test: `bash tools/audio/run.sh` -> 9 Checks, alle OK (CRC sine 1667360149, mono 20619752, mix 432101923).

### Latenz (ehrlich)
Auf dem Server laeuft kein Pulse-Daemon (`pactl info`: Verbindung abgelehnt) und die Zeit reichte nicht, einen zu starten und `parec`-Zeitstempel auszuwerten.
**Nicht gemessen.** Rechnerisch: `audio.pulse` fordert ~1/4 s Puffer an (A2 in `player.fi`), das ist die Obergrenze der Latenz; fuer Spiel-SFX
`tlength` im Pulse-Client verkleinern (offen, siehe Naechster Schritt). Mix-Kosten: Mixer rechnet 1 Block (<= 4096 Frames) unter einem Lock, 32 Stimmen
ohne SIMD; nicht gemessen.

### OGG/Vorbis -- Plan und Aufwand (nicht begonnen)
1. Ogg-Container (Seiten, CRC, Segmenttabelle, Pakete): ~250 Zeilen, 0.5 Tag, Test gegen `ffmpeg`/`oggz`.
2. Vorbis-Header (Identification, Comment, Setup mit Codebooks, Floor-1, Residue 0/1/2, Mappings, Modes): ~900 Zeilen, 2-3 Tage.
3. Audio-Paket-Decode (Floor1-Kurve, Residue-Decode, Kanalkopplung, IMDCT n=64..8192 mit Fenstern, Overlap-Add): ~700 Zeilen, 2 Tage; Gegenprobe: WAV
   von `ffmpeg -f s16le` bzw. `oggdec` (liegt hier nicht; `ffmpeg` ist da) -- Bit-Gleichheit nicht zu erwarten (float), Ziel SNR > 90 dB.
Gesamt: ca. 1700 Zeilen / 5-6 Arbeitstage. IMDCT kann `lib/ton/mp3.fi` (MDCT) nur teilweise wiederverwenden (andere Groessen).

## Eingabe (lib/input) -- Befund
* `lib/input` ist **Eingabe-Injektion**, nicht Lesen: `/dev/uinput` (Linux) bzw. `SendInput` (Windows) bewegen Maus/Tasten DIESES Rechners. Rohe Tastatur-/Maus-Ereignisse
  **lesen** (evdev `/dev/input/event*`, XInput2 raw, RawInput) gibt es dort **nicht**.
* Fensterereignisse (Tasten, Maus-Position, Touch ueber XInput2 2.2) liefert `lib/window/x11.fi`/`backend.fi`; **relative Maus / Pointer-Grab / Warp
  / Cursor verstecken** (X11: `GrabPointer` + `WarpPointer` oder XI2 raw motion, Windows: `ClipCursor`/`SetCursorPos`/RawInput) gibt es **nicht**.
  Das haengt am X11-Verbindungsobjekt im Fenstermodul (MC-B/Fenster-Teil), nicht in `lib/input`: dort ergaenzen, nicht in einem zweiten Modul.
* Gamepad: nichts vorhanden (Linux `/dev/input/js*`/evdev `EV_ABS`, Windows XInput `XInputGetState` aus `xinput1_4.dll` per `dynlib`).
* **Diese Runde nicht gebaut** (Zeit/Kontext). Plan: `lib/input/evdev.fi` (Ereignisse lesen: `struct input_event` 24 Oktette, `EVIOCGRAB`, nicht-blockierend,
  reine Parser-Funktionen mit Test per `uinput`-Schleife -- `/dev/uinput` ist auf dem Server nicht garantiert), `lib/input/xi2.fi` fuer Raw-Motion im Fenstermodul,
  `lib/input/xinput.windows.fi` fuer Gamepad. Xvfb + xdotool sind installiert; XTEST kann Bewegungen fuer XI2-Tests erzeugen.

## B22 Destruktoren -- Befund
Die Luecke ist **veraltet**: seit Runde OWN gibt es `fn drop(inout self)` (Reihenfolge, `return`/`break`/`try`) und `defer`/`errdefer` (SPEC 3.3, `tests/2005_drop_order.fi`).
Offen bleibt: `drop` fuer `gc class`/`Rc[T]` (SPEC "Not yet"). Neu: `std.scope` (`lib/std/scope.fi`, Test `tests/2765_scope.fi`): `Arena` (Bump + Reset), `Scope`
(umgekehrte Freigabe dynamisch vieler Handles, `scope_cancel`), Tracker (`track_live(kind)` als Leck-Check). Regel: ein Besitzer pro Ressource; ein Struct mit `drop`
fuer eine, `Scope` fuer viele, `Arena` fuer viele Bloecke einer Lebensdauer.

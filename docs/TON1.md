# Runde TON 1 -- der MP3-Dekoder (`lib/ton/mp3.fi`)

Stand 18.09.2026, Zweig `ton`. Alles hier ist **gelaufen**, nicht geschaetzt;
die Befehle stehen darunter und lassen sich nachfahren.

## Warum diese Runde

Firn hat Netz, TLS und HTTP, aber keinen Ton: `lib/media/audio.fi` in Certus
hat eine Rueckwand fuer Win32 (`waveOut`) und sonst Stille. Fuer alles, was
mit Klang zu tun hat -- Radio, Datei, Strom -- fehlt zuerst der **Dekoder**.
Ohne ihn ist jede Tonschicht taub, weil praktisch alles, was ueber das Netz
kommt, MPEG-1/2 Layer III ist.

Diese Runde baut genau den einen Baustein: Oktette rein, PCM raus.
Kein Systemaufruf, kein Haufen, keine Geraeteabhaengigkeit.

## Was gebaut wurde

| Datei | Zeilen | Inhalt |
|---|---|---|
| `lib/ton/mp3.fi` | ~1500 | der Dekoder: Rahmensuche, Seiteninfo, Skalenfaktoren, Huffman, Stereo, IMDCT, Synthesefilterbank |
| `lib/ton/mp3_tab.fi` | 415 | die Tabellen, **erzeugt** von `tools/mp3_tabellen.py` |
| `lib/ton/mp3_main.fi` | 90 | Messtreiber: `.mp3` -> `.pcm` + eine Kennzahlzeile |
| `lib/ton/mp3_pruef_main.fi` | 150 | Selbsttest gegen festgehaltene Pruefsummen |
| `tools/mp3_tabellen.py` | 190 | Tabellenerzeuger aus der Vorlage |
| `tools/mp3_vergleich.py` | 60 | misst zwei PCM-Dateien gegeneinander (max, RMS, SNR) |

Herkunft: der Aufbau folgt **minimp3** (lieff, CC0-1.0). Die Tabellen kommen
aus der Vorlage durch den Erzeuger, der Ablauf ist neu geschrieben -- Firn
hat weder globale Variablen noch Zeigerarithmetik im C-Stil.

## Das Ergebnis: bitgenau

Verglichen wird gegen dieselbe Vorlage, in C uebersetzt (`gcc -O2
-DMINIMP3_NO_SIMD -DMINIMP3_ONLY_MP3`). "Bitgenau" heisst: jedes einzelne
16-Bit-Wort identisch.

| Probe | Format | Rahmen | Ergebnis |
|---|---|---|---|
| Sinus 440/3000 Hz | MPEG-1, 44,1 kHz, Stereo, 128 kbit/s | 117 | **bitgenau** |
| rosa Rauschen | MPEG-1, 44,1 kHz, Mono, 64 kbit/s | 79 | **bitgenau** |
| weisses Rauschen | MPEG-1, 32 kHz, Stereo, 192 kbit/s | 58 | **bitgenau** |
| Sinus | MPEG-2, 24 kHz, Mono, 48 kbit/s | 87 | **bitgenau** |
| VBR-Material | MPEG-1, 44,1 kHz, Stereo, VBR q2 | 194 | **bitgenau** |
| weisses Rauschen | MPEG-1, 44,1 kHz, Stereo, 320 kbit/s | 117 | **bitgenau** |
| Sinus | MPEG-1, 48 kHz, Joint Stereo, 96 kbit/s | 169 | **bitgenau** |
| Impulse (kurze Bloecke) | MPEG-1, 44,1 kHz, Mono, 128 kbit/s | 156 | **bitgenau** |
| Sinus | **MPEG-2.5**, 8 kHz, Mono, 32 kbit/s | 45 | **bitgenau** |
| **echter Radiostrom** (MangoRadio, 31 s) | MPEG-1, 44,1 kHz, Stereo, 128 kbit/s | 1210 | **bitgenau** |

Gegen **ffmpeg** (voellig andere Umsetzung) auf demselben Radiostrom:
SNR 66,4 dB, groesste Abweichung 310 von 32768, 0,47 % der Werte ungleich.
Das ist der normale Abstand zwischen zwei erlaubten MP3-Dekodern -- die
Norm gibt einen Fehlerspielraum vor, keinen exakten Bitstand.

### Was auf dem Weg dorthin falsch war

Drei Fehler, alle drei durch Messen gefunden, nicht durch Lesen:

1. **`MAX_SCFI` falsch ausgerechnet** (232 statt 44). Ergebnis: `1 << 58`
   in einem `i32`, die Verstaerkung wurde 0, der Dekoder lieferte
   **vollstaendige Stille** bei formal richtigen Rahmenzahlen. Lehre: eine
   Ausgabe, die genau die richtige LAENGE hat, ist noch kein Beweis.
2. **`s4 += s8 - s2`** war als `s4 + s8 - s2` geschrieben. In
   Fliesskomma ist das nicht dasselbe: ein ULP Unterschied, der erst dann
   sichtbar wird, wenn ein Wert dicht an einer Quantisierungsstufe liegt.
   Wirkung: 0,02 % der Abtastwerte um genau 1 daneben.
3. **`b[j] += vz*w1 + vy*w0`** in der Synthesefilterbank, ebenfalls als
   `b + vz*w1 + vy*w0` geschrieben -- dieselbe Falle, gleiche Wirkung.

Nach (2) und (3) war der Abstand **null**. Der Zwischenstand davor
(SNR 110 dB, maximal 1 LSB) waere hoerbar nicht zu unterscheiden gewesen --
umso mehr ein Grund, ihn nicht als "richtig" durchgehen zu lassen.

## Robustheit

* 200 kB **Zufallsoktette**: 0 Rahmen, kein Absturz, wie die Vorlage.
* 5 kB Muell **vor** einem gueltigen Strom: bitgenau, Rahmensuche findet ein.
* **abgeschnittene** Datei (20 kB von 49 kB): 47 Rahmen, bitgenau.
* **ID3v2**-Kopf davor: bitgenau (der Merkblock wird ueberlaufen).
* **150 zufaellig verfaelschte** Stroeme (1-40 gekippte Oktette je Lauf):
  **0 Abstuerze**, 147 davon bitgenau. Die drei Abweichungen entstehen dort,
  wo der Huffman-Leser bei kaputten Daten ueber das Rahmenende hinausliest --
  das tut die Vorlage auch, nur liegt hinter dem Puffer bei ihr ein anderes
  Feld. Kein Zugriff ausserhalb des eigenen Kratzraums.

## Tempo -- der ehrliche Teil

60 s Audio (MPEG-1, 44,1 kHz, Stereo, 192 kbit/s), AMD EPYC, dieselbe Maschine:

| | Zeit | Echtzeitfaktor |
|---|---|---|
| Firn (dieser Dekoder) | 3,3 s | ~18x |
| C-Vorlage, skalar, `-O2` | 0,34 s | ~176x |

**Rund zehnmal langsamer als C.** Das reicht fuer Radio mit grossem Abstand
(ein Strom braucht 1x Echtzeit), ist aber kein guter Wert, und der Grund ist
bekannt: jeder Speicherzugriff laeuft ueber die Hilfsfunktionen `lf`/`sf`
mit `adr4`, also ueber einen echten Aufruf samt Verzweigung fuer den
negativen Index. Das ist die Stelle fuer Runde TON 2 -- erst messen, dann
inlinen.

## Bekannte Grenzen

* **Nur Layer III.** Layer I/II werden erkannt und uebersprungen.
* **Kein SIMD.**
* **Ausgabe i16**, verschachtelt.
* Kein Gapless-Zuschnitt (LAME/Xing-Kopf wird nicht ausgewertet): am Anfang
  bleiben die Vorlaufwerte des Kodierers stehen, wie bei der Vorlage.

## Selbst nachfahren

```sh
FIRNC=/root/firn/compiler/target/release/firnc
FIRNLIB=$PWD/lib $FIRNC -o /tmp/mp3 lib/ton/mp3_main.fi
FIRNLIB=$PWD/lib $FIRNC -o /tmp/mp3pruef lib/ton/mp3_pruef_main.fi

# Selbsttest (ohne fremde Werkzeuge)
/tmp/mp3pruef testdata/ton/stereo44.mp3 testdata/ton/mono24.mp3 \
              testdata/ton/mono8.mp3 testdata/ton/kurzbloecke.mp3

# Eine Datei dekodieren und anhoeren
/tmp/mp3 irgendwas.mp3 /tmp/x.pcm
ffplay -f s16le -ar 44100 -ch_layout stereo /tmp/x.pcm
```

Die Tabellen neu erzeugen (nur noetig, wenn die Vorlage sich aendert):

```sh
curl -L -o /tmp/minimp3.h https://raw.githubusercontent.com/lieff/minimp3/master/minimp3.h
python3 tools/mp3_tabellen.py /tmp/minimp3.h > lib/ton/mp3_tab.fi
```

## Was als Naechstes kommt

1. **TON 2 -- Tempo**: `lf`/`sf` inlinen, Messung gegen diese Zahlen.
2. **TON 3 -- Ausgabe**: Rueckwand fuer Linux (ALSA) und Android (AAudio),
   damit aus PCM wirklich Ton wird.
3. **TON 4 -- Strom**: HTTP/Icecast-Anbindung, Ringpuffer, Nachschub-Weg
   ohne Aussetzer.
4. Danach erst die Mischerschicht im Sinn von Aulos (Stimmen, Busse, Kurven).

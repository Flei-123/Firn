# Runde TEMPO 5 — vier Spuren bis in die Umkehrwandlung

Stand 21.09.2026, Zweig `xmm-ra`. Fortsetzung von TEMPO 4: dort wurde `v128`
im Weg mit Registerzuteilung ueberhaupt erst benutzbar, hier wird es
angewandt — und der Befehlssatz um das ergaenzt, was dafuer noch fehlte.

## Was dazugekommen ist (Erzeuger)

| Name | x86 | aarch64 |
|---|---|---|
| `__v128_trunc_f32_i32` | `cvttps2dq` | `fcvtzs .4s` |
| `__v128_cvt_i32_f32` | `cvtdq2ps` | `scvtf .4s` |
| `__v128_cmplt_f32` | `cmpltps` | `fcmgt` (Operanden getauscht) |
| `__v128_cmple_f32` | `cmpleps` | `fcmge` (getauscht) |
| `__v128_cmpnlt_f32` | `cmpnltps` | `fcmgt` + `mvn` |
| `__v128_cmpgt_i32` | `pcmpgtd` | `cmgt .4s` |

Dazu duerfen jetzt auch `and`, `andnot`, `or`, `xor`, `add32` und `sub32`
durch den Weg mit Registerzuteilung (vorher schickte schon eines von ihnen
die ganze Funktion auf den Grundweg). `--cpu=avx` kennt alle neuen Befehle.

`cmpnlt` ist **nicht** die Verneinung von `cmplt`: bei NaN sind beide
Vergleiche ungeordnet, und `cmpnltps` sagt dann WAHR. Genau das wird
gebraucht, um die einzelne Fassung der Abtastwandlung nachzubilden —
`tests/1616_simd_cvt.fi` haelt es fest, damit die aarch64-Fassung
(`fcmgt` + `mvn`) nicht davon abweicht.

## Was damit im Dekoder umgestellt wurde

### `dct_ii` — vier Baender auf einmal (45 -> 19 Mio)

Die Umkehrwandlung rechnet je Band EINE Spalte von `grbuf`: der Zugriff ist
`grbuf[k + 18*z]`, und fuer vier benachbarte Baender liegen die vier Werte
NEBENEINANDER. Damit ist jede Zeile der Wandlung eine Rechnung auf vier
Spuren. Was an Baendern uebrig bleibt (`n` ist nicht immer durch vier
teilbar), rechnet weiter die alte Fassung — sie steht unveraendert daneben.

Die Beiwerttabelle `sec` hat vier Werte Polster bekommen: ein Beiwert wird
als 16-Oktett-Ladung geholt und mit `__v128_shuffle32` auf alle vier Spuren
gezogen.

### `l3_imdct36` — die Schlussschleife (44 -> 27 Mio)

Die neun Schritte am Ende lesen sieben Reihen, die alle mit `i` aufwaerts
laufen und nebeneinander liegen; nur die zweite Ausgabe geht rueckwaerts
(`17 - i`), und die dreht `__v128_shuffle32(.., 0x1B)`. Acht der neun
Schritte laufen jetzt als zwei Vierergruppen, der neunte einzeln.

### `scale_pcm` — die Abtastwandlung (26 -> 15 Mio)

Aus vier Gleitzahlen werden vier ganze Zahlen: `+0,5`, abschneiden,
bei negativem Ergebnis eins abziehen, an beiden Enden klemmen.

**Hier lag der einzige Fehler dieser Runde, und er ist lehrreich.** Die
einzelne Fassung schreibt

```firn
var s: i32 = (abtast + 0.5f) as i32
if s < 0 { s = s - 1 }
```

Das ist NICHT `floor(x + 0,5)`. Es zieht bei **jedem** negativen Ergebnis
eins ab — auch wenn das Abschneiden gar nichts verworfen hat, der Wert also
schon ganz war. Die erste Fassung hier rechnete `floor` (abschneiden,
zurueckwandeln, vergleichen) und war fuer die meisten Werte gleich, fuer
genau ganze negative Zwischenergebnisse aber um eins daneben. Der
Selbsttest hat es sofort gemeldet; ein Vergleichsprogramm ueber 200 000
Werte hat gezeigt, welche. Jetzt steht dort die Maske `i < 0`
(`__v128_cmpgt_i32(0, i)`) — und das ist obendrein kuerzer.

Die beiden Anschlaege sind ausdruecklich nachgebildet, nicht der Saettigung
ueberlassen: `cmpnlt(v, 32766.5)` ist wahr fuer `v >= 32766,5` **und** fuer
NaN (die einzelne Fassung landet bei NaN ueber den Ueberlauf der Wandlung
ebenfalls bei 32767), `cmple(v, -32767.5)` ist der untere Anschlag.

### `synth` — Zeiger statt Index (klein, aber richtig)

Die drei Adressen der inneren Schleife laufen in festen Schritten (256
Oktette abwaerts, 256 aufwaerts, 8 weiter). Einmal ausgerechnet und dann
weitergeschaltet.

## Die Zahlen

MP3-Dekoder, 60 s Ton, `release-fast`, kleinste von neun Laeufen, Ausgabe
nach `/dev/null`:

| | Befehle (8 s Ton) | Zeit |
|---|---|---|
| nach TEMPO 4 | 258,6 Mio | 0,21 s |
| + `dct_ii` auf vier Baendern | 236,1 Mio | |
| + `l3_imdct36` | 222,1 Mio | |
| + `scale_pcm` | 211,2 Mio | |
| + Zeiger in `synth` | **209,6 Mio** | **0,19 s** |
| dasselbe mit `--cpu=avx` | **194,1 Mio** | **0,18 s** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio | 0,06 s |

Nach **jedem** Schritt: PCM bitgleich (`cmp` ueber 10,6 MB) und Selbsttest
PASS 4/4, in `baseline` wie in `avx`.

Seit dem Beginn der Tempo-Runden: **644 -> 210 Mio Befehle, 0,88 -> 0,19 s.**

## Was jetzt noch drin steckt

Die Messung sagt es genau (Anteile am Ganzen):

* `synth` 59 Mio (28 %) — jetzt fast nur noch Adressrechnung und die acht
  Einzelzugriffe am Schleifenkopf.
* `l3_huffman` 30 Mio (14 %) — Bitleserei, nichts zu vektorisieren.
* `l3_imdct36` 27 Mio (13 %) — was bleibt, sind die beiden Aufrufe von
  `l3_dct3_9` (12 Mio) mit ihren Abhaengigkeiten.
* `mp3_decode_frame` 17 Mio, `scale_pcm4` 15 Mio, `dct_ii_4` 14 Mio.

Der naechste grosse Schritt waere nicht mehr SIMD, sondern die
**Ganzzahlseite**: Zeiger weiterschalten statt Adressen neu rechnen (im
Erzeuger, nicht von Hand), und Lebensdauern an Aufrufen zerschneiden.

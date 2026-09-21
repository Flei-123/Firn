# Runde TEMPO 4 — SIMD wird benutzbar: `v128` im Weg mit Registerzuteilung

Stand 21.09.2026, Zweig `xmm-ra`. Alles gemessen: Befehle mit
`valgrind --tool=callgrind` (8 s Ton), Zeiten als kleinste von neun Laeufen mit
Ausgabe nach `/dev/null`.

## Der Befund, der diese Runde ausgeloest hat

Nach TEMPO 3 stand der MP3-Dekoder bei 304 Mio Befehlen, `minimp3` in C bei
75 Mio. Ein Drittel des Unterschieds hat eine einzige Ursache: **`gcc -O2`
giesst die Schleifen selbst in `packed`-Befehle** — vier `f32` je Anweisung.
Ohne Vektorisierung braucht dasselbe C 0,10 s statt 0,07 s (gemessen mit
`-fno-tree-vectorize`).

Firn hatte `v128` schon als Sprachmittel (Runde 82: 42 Intrinsics fuer
AES, SHA, CRC, Byteschieben) — **aber keinen einzigen Fliesskommabefehl**, und
vor allem: *jede* Funktion mit einem `v128` fiel auf den Grundweg des
Erzeugers zurueck, also ohne Registerzuteilung. Wer Vektoren benutzte,
verlor die Zuteilung fuer die ganze Funktion — Adressrechnung inbegriffen.
Damit war SIMD in Firn praktisch unbenutzbar, sobald drumherum noch etwas
gerechnet wurde.

## Was gebaut wurde

### 1. Drei gepackte Fliesskommabefehle (beide Maschinen)

`__v128_addf32`, `__v128_subf32`, `__v128_mulf32` — x86: `addps`, `subps`,
`mulps`; aarch64: `fadd`, `fsub`, `fmul` mit `4s`. Sie rechnen **je Spur
genau das, was der einzelne Befehl rechnet**; eine ausgerollte Schleife, die
auf vier Spuren umgestellt wird, bleibt deshalb bitgleich. Geprueft wird das
in `tests/1615_simd_f32.fi` Spur fuer Spur gegen die einzelne Rechnung, in
allen vier Baustufen.

### 2. `v128` im Weg mit Registerzuteilung

`unsupported_basic` laesst jetzt eine **enge Auswahl** herein: `__v128_load`,
`__v128_store`, `__v128_zero`, die drei Rechnungen und `__v128_shuffle32`.
Dazu kommen `Op::Load`/`Op::Store`/`Op::Copy` mit `v128`. Alles andere
(Krypto, Byteschieben, einzelne Spuren ein- und auslesen) geht weiterhin
ueber den Grundweg mit dem Zwischenspeicher aus `simd.rs` — dort ist es
richtig aufgehoben, denn dieser Weg kann es.

Dafuer noetig:

* **Plaetze fuer `v128`**: sechzehn Oktette, sechzehnfach ausgerichtet
  (`layout`). `rbp` steht nach dem Vorspann auf einem Vielfachen von
  sechzehn, also genuegt ein Abstand, der eines ist. Damit darf `movaps`
  zwischen Register und Platz gehen; nur die Zugriffe ueber einen Zeiger aus
  dem Programm bleiben `movdqu`.
* **Register**: `v128` gehoert in dieselbe Klasse wie `f32`/`f64` (`xmm`), und
  der zweite Durchgang des Linear Scan aus Runde XMM 3 verteilt sie mit. Die
  Bedingung von dort gilt weiter: wer einen Aufruf ueberlebt, behaelt seinen
  Platz (alle sechzehn `xmm` sind caller-saved).
* **`mem2reg` befoerdert Vektorvariablen.** Bisher ausgeschlossen, weil die
  Kopie, die `phi.rs` aus einem `phi` macht, auf dem Grundweg als
  GANZZAHLkopie ausgegeben wurde — acht von sechzehn Oktetten. Beide Wege
  koennen sie jetzt (`simd::emit_copy_v128`).
* **`--cpu=avx`** kennt die neuen Befehle: `vaddps`/`vsubps`/`vmulps`
  dreistellig, `vpshufd`, `vmovdqu`.

### 3. Drei Stellen im Dekoder auf vier Spuren umgestellt

| Funktion | warum sie passt |
|---|---|
| `synth` (die heisseste Funktion des Dekoders) | die vier Summen rechnen dasselbe mit NEBENEINANDERLIEGENDEN Werten und denselben zwei Fenstergewichten; `__v128_shuffle32` zieht jedes Gewicht auf alle vier Spuren |
| `l3_midside_stereo` | `a+b` und `a-b` ueber zwei Reihen, Rest der Laenge einzeln |
| `l3_antialias` | `u` aufsteigend, `d` absteigend — beide liegen je vier Werte nebeneinander, `__v128_shuffle32(.., 0x1B)` dreht die Reihenfolge hin und zurueck |

Die Fenstertabelle `win` hat dafuer **vier Werte Polster** bekommen: das
Gewichtspaar wird als 16-Oktett-Ladung geholt (zwei Werte gebraucht, vier
gelesen), und damit bleibt auch das letzte Paar innerhalb der Tabelle.

## Die Zahlen

MP3-Dekoder, 60 s Ton, `release-fast`:

| | Befehle (8 s Ton) | Zeit (60 s Ton) |
|---|---|---|
| nach TEMPO 3 | 304,4 Mio | 0,24 s |
| + `synth` auf vier Spuren | 272,3 Mio | |
| + `l3_midside_stereo` | 267,3 Mio | |
| + `l3_antialias` | **258,6 Mio** | **0,21 s** |
| dasselbe mit `--cpu=avx` | **235,4 Mio** | **0,21 s** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio | 0,07 s |
| dasselbe C, `-O2 -fno-tree-vectorize` | — | 0,10 s |

**Abstand zu C: 3,0x** gegen `gcc -O2`, **2,1x** gegen dasselbe C ohne
Auto-Vektorisierung (vor dieser Runde: 3,4x und 2,4x).

Richtigkeit: nach **jedem** der drei Schritte ist die PCM-Ausgabe bitgleich
(`cmp` ueber 10,6 MB) und der Selbsttest gibt PASS 4/4 — in `baseline` wie in
`avx`. Das ist kein Zufall, sondern die Eigenschaft, auf der die ganze Runde
steht: `addps` ist viermal `addss`, Spur fuer Spur, mit derselben Rundung.

## Was als Naechstes moeglich waere

Gemessen, in der Reihenfolge des Gewichts im Dekoder:

1. **`dct_ii` (45 Mio) und `l3_imdct36` (44 Mio)** — dieselbe Umstellung wie
   `synth`, aber mehr Arbeit: die Schleifen sind von Hand ausgerollt und
   muessen erst wieder in Spuren sortiert werden.
2. **`scale_pcm` (28 Mio)** — braucht drei weitere Befehle
   (`cvttps2dq`, `cvtdq2ps`, `packssdw`) und eine genaue Nachbildung der
   Randfaelle (`floor(x+0,5)` und das Abschneiden an ±32767), sonst ist die
   Ausgabe nicht mehr bitgleich.
3. **Die Ganzzahlseite.** In `synth` stehen jetzt 204 `mov`, 31 `add`,
   27 `lea` gegen 26 `movaps` und die gepackten Rechnungen — die
   Adressrechnung ist die Haelfte der Arbeit. Das ist der naechste grosse
   Brocken und hat mit SIMD nichts zu tun (Zeiger weiterschalten statt
   Adressen neu rechnen, Lebensdauern an Aufrufen zerschneiden).

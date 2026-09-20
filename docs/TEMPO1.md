# Runde TEMPO 1 — Fliesskomma zu Ende gebracht, und ein Fehler aus XMM 3

Stand 20.09.2026, Zweig `xmm-ra`. Alles hier ist **gelaufen und gemessen**;
die Befehle stehen dabei.

## Ausgangslage

Runde XMM 3 hat dem Registerzuteiler Fliesskomma beigebracht. Der MP3-Dekoder
lief danach 60 s Ton in **0,91 s**, dieselbe Vorlage in C (`minimp3`,
`gcc -O2 -DMINIMP3_NO_SIMD`) in **0,11 s** — Faktor 8,3. Eine Messung der
wirklich ausgefuehrten Befehle (`valgrind --tool=callgrind`) sagte, woran es
liegt: **644 Mio** gegen **75 Mio** Befehle. Es war also nicht die
Reihenfolge und nicht der Zwischenspeicher, sondern die schiere Menge
erzeugter Arbeit.

`perf` geht in diesem Behaelter nicht (`perf_event_paranoid` liegt fest),
callgrind schon. Die Zuordnung Adresse -> Funktion kommt aus `nm`.

## Was gefunden wurde, in der Reihenfolge der Wirkung

### 1. Ein `-x` schickte die halben Dekoder auf den Grundweg (Uebersetzer)

`FIRN_RA_STATS=1` sagt, welche Funktion keine Registerzuteilung bekommt.
Antwort: **die sieben heissesten** — `l3_imdct36`, `l3_huffman`,
`synth_pair`, `scale_pcm` und drei weitere, alle mit demselben Grund
*„einstellige Rechnung mit Gleitzahl"*. XMM 3 hatte `Op::Un` ausgeschlossen,
also **jede** Funktion, in der irgendwo ein `-x` auf eine Gleitzahl steht.

Die Vorzeichenumkehr ist ein Bit: `xorps` gegen eine Maske, die ueber `rax`
in `xmm1` kommt (SSE hat keine Form mit Konstante). Damit faellt der
Ausschluss weg; `!` auf eine Gleitzahl bleibt draussen, weil es dafuer
ueberhaupt keine Bedeutung gibt.

**644 Mio -> 467 Mio Befehle, 0,91 s -> 0,64 s.**

### 2. Die Adressrechnung des Dekoders hatte einen Sprung (Bibliothek)

`lib/ton/mp3.fi` rechnet mit vorzeichenbehafteten Indizes, weil die
Synthesefilterbank wirklich negative braucht (`zlin[4*(i-16)+2]`). Das stand
als `if i < 0 { p - (-i)*4 } else { p + i*4 }` — vor **jedem einzelnen**
Feldzugriff ein Vergleich, ein Sprung und ein phi.

Noetig ist der Zweig nicht: Adressen rechnen modulo 2^64, und `i as u64` ist
bei gleicher Breite eine Umdeutung des Bitmusters (in `dev`, `release-safe`
und `release-fast` geprueft). `p +% ((i as u64) *% 4)` ist fuer negative `i`
dasselbe — ohne Sprung. Erst damit kann der Uebersetzer den Zugriff
ueberhaupt in den Adressteil des Befehls falten.

**467 Mio -> 370 Mio, 0,64 s -> 0,37 s.**

### 3. `+%` war fuer jede Optimierung unsichtbar (Uebersetzer)

Und hier zeigte die Messung den eigentlichen Witz: `Op::BinWrapSat { kind:
Wrap }` und `Op::Bin` bedeuten im FIR **dasselbe** — beide behalten die
unteren Bits, beide pruefen nichts (gepruefte Rechnung heisst
`Op::CheckedBin`). Der Unterschied war rein syntaktisch. Nur fragt **jede**
Optimierung nach `Op::Bin`: gemeinsame Teilausdruecke, Schleifeninvarianten,
die algebraischen Kuerzungen und vor allem das Falten der Adresse in den
Befehl (`regalloc::foldable_addresses`). Ein `+%` lief an allen vorbei.

`canon_wrap` schreibt `Wrap` einmal vor allen Paessen in `Op::Bin` um.
`Sat` bleibt unberuehrt — das Abschneiden ist wirklich etwas anderes.

**370 Mio -> 338 Mio, 0,37 s -> 0,34 s.**

### 4. Das Vorzeichen aendert kein Bit (Uebersetzer)

`copy_propagate` liess eine Umwandlung nur verschwinden, wenn beide Seiten
gleich breit **und gleich vorzeichenbehaftet** waren. Damit blieb aus
`(i as u64)` ein echtes `mov` stehen — im Dekoder vor jedem Zugriff. Bei
gleicher Breite ist das Muster identisch; ob es als negativ gelesen wird,
entscheidet allein die Anweisung, die es benutzt (jede traegt ihren eigenen
Typ). Bool und Gleitzahlen bleiben getrennt, die **gepruefte** Umwandlung
ist eine andere Anweisung.

### 5. Drei Kleinigkeiten im Fliesskommaweg (Uebersetzer)

* **Vertauschen statt kopieren:** liegt bei `a + b` / `a * b` der zweite
  Operand schon im Zielregister, wird getauscht — das spart die Rettung nach
  `xmm1` und die Kopie des ersten.
* **Fliesskomma-Parameter** duerfen in ihrem Register bleiben (der Vorspann
  konnte das laengst; ausgeschlossen waren sie noch aus der Zeit, als dieser
  Weg kein Fliesskomma ausgab).
* **`-1.5f` wird gefaltet:** das Vorzeichen einer Konstante ist ein Bit, kein
  Rechenschritt. Vorher stand in `scale_pcm` — der innersten Schleife —
  Konstante laden, Maske laden, `xorps`.

**338 Mio -> 324 Mio.**

## Und ein falsch erzeugtes Programm aus Runde XMM 3

`tests/1452_f32_abi.fi` gab **6 statt 0**, in `release-fast` wie in
`release-safe`. Der Fehler steckte in XMM 3 selbst (mit dem Stand von
`14f9ee3c` nachgestellt), nicht in dieser Runde — gefunden hat ihn erst der
volle Testlauf hier.

Ursache: `fp_taugt` entschied „darf dieser Wert in ein xmm?" nach der **Art**
der Anweisung (`Bin`, `Cmp`, `Cast`, `Copy`, `Store`: immer gut). Das ist zu
grosszuegig. Die Aufrufkonvention kopiert einen Verbund **achtbyteweise**,
und dabei steht ein `store.u64` mit einem Wert, dessen Typ `f64` ist — ein
`struct { f32, f32 }` reist als ein Achtbyte in `xmm0`. Diese Anweisung geht
ueber den Ganzzahlweg, holt ihren Operanden mit `mov`, und wenn der Wert
inzwischen in einem `xmm` lebte, schrieb sie Unsinn in den Rahmen. Im
Erzeugten stand woertlich `mov qword ptr [rbp-360], rbp`.

Jetzt zaehlt nicht die Art der Anweisung, sondern ob sie **wirklich** im
Fliesskommaweg steht — also genau die Bedingung, unter der die Ausgabe ihren
Fliesskommazweig nimmt (`inst.ty.is_float()`, beim Vergleich der Typ des
Vergleichs, bei der Umwandlung eine der beiden Seiten).

## Die Messung

MP3-Dekoder, 60 s Ton (1,4 MB, 192 kbit/s, Stereo), `release-fast`, kleinste
von neun Laeufen, dieselbe Maschine:

| | Befehle (callgrind, 8 s Ton) | Zeit (60 s Ton) |
|---|---|---|
| XMM 3 (Ausgangslage) | 644 Mio | 0,91 s |
| nach dieser Runde | **324 Mio** | **0,31 s** |
| `minimp3` in C, `gcc -O2` | 75 Mio | 0,11 s |

**Abstand zu C: von 8,3x auf 2,8x.**

Richtigkeit: die PCM-Ausgabe ist **bitgleich** zu vorher (`cmp` auf die
ganze Datei) und bitgleich zu C (`mp3_pruef_main` -> PASS 4/4 ueber vier
Stroeme: MPEG-1 Stereo, MPEG-2 Mono, MPEG-2.5 8 kHz, kurze Bloecke).

Ganzzahl-Programme aendern sich **nicht**: `bench/instr.sh` zaehlt fuer
`bubblesort` und `statemachine` denselben Befehl (1 von 1,47 Mrd
Unterschied, das ist die Startaufsetzung). Diese Runde wirkt auf
Fliesskomma, auf `+%` und auf Adressrechnung mit vorzeichenbehafteten
Indizes — nicht auf jede Schleife.

Testreihe: `bash test.sh` ohne neuen Fehler; `1452_f32_abi` ist von FAIL auf
PASS gegangen. Offen bleiben drei Punkte, die nichts mit dem Erzeuger zu tun
haben (die englische Namensprobe schlaegt auf `lib/fui/*` an, der
Selbstlauf-Fixpunkt wollte `tools/gen_gctext.sh` — nachgezogen —, und
`testdata/test262/subset.sha256` fehlt in diesem Arbeitsbaum).

## Was als Naechstes messbar etwas bringen wuerde

1. **Fliesskomma-Zellen in `xmm`.** Eine Schleifenvariable
   (`Op::Alloca`, als Zelle befoerdert) bekommt heute nur ein
   Ganzzahlregister und reist bei jedem Zugriff per `movd` hin und her — oder
   bleibt ganz auf dem Platz. In `synth`, der heissesten Funktion des
   Dekoders (112 Mio von 324 Mio Befehlen), sind das acht Summen: pro
   Durchlauf sechzehn Wege in den Rahmen und zurueck.
2. **Lebensdauern an Aufrufen zerschneiden.** Wer einen Aufruf ueberlebt,
   bekommt heute gar kein Register (alle sechzehn `xmm` sind
   caller-saved). In `synth` stehen deshalb drei Basiszeiger auf ihren
   Plaetzen und werden vor jedem Zugriff neu geholt.
3. **Drei Operanden (AVX).** `vmulss d, a, b` spart die Kopie, die `mulss`
   erzwingt — in `synth` stehen 50 solche `movaps`. Braucht einen Schalter
   fuer die Ziel-CPU; die Grundausstattung bleibt SSE2.

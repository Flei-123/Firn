# Runde TEMPO 2 — das Uebergaberegister, Zellen in `xmm`, und `--cpu=avx`

Stand 21.09.2026, Zweig `xmm-ra`. Alles gemessen, nicht geschaetzt. Zeiten:
kleinste von neun Laeufen, Ausgabe nach `/dev/null` (das Schreiben von
10,6 MB PCM verdeckt sonst genau den Unterschied, um den es geht), ruhige
Maschine, AMD EPYC 7571. Befehlszahlen: `valgrind --tool=callgrind` auf 8 s
Ton, Zuordnung per `nm`.

## Wo Runde TEMPO 1 aufgehoert hat

Der MP3-Dekoder stand bei 324 Mio Befehlen. Die Messung zeigte zwei Dinge
klar:

* `synth` allein waren **112 Mio** davon, und im Erzeugten stand pro
  Rechenschritt ein Paar wie dieses:

      movss  [rbp-3912], xmm0     <- hinschreiben
      movss  xmm0, [rbp-3912]     <- und gleich wieder holen

* 51 `movaps` in derselben Funktion: SSE hat nur die zweistellige Form,
  `mulss d, s` rechnet also immer INS Ziel, und wenn der erste Operand
  woanders liegt, muss er vorher kopiert werden.

## 1. Das Uebergaberegister (wirkt, −4,7 %)

Zwoelf `xmm` reichen in einer dicht gerechneten Schleife nicht; wer keines
bekommt, liegt im Rahmen. Aber: hat so ein Wert **genau einen** Leser, und
steht der kurz darauf im selben Block, dann braucht er den Rahmen ueberhaupt
nicht — er kann im Register stehen bleiben.

`fp_handover` (in `regalloc.rs`) sammelt diese Werte je Grundblock und
verteilt sie gierig auf die Register, die der Zuteiler nie ausgibt: `xmm2`,
`xmm3` und alles aus dem Vorrat `xmm4`–`xmm15`, was die Funktion gar nicht
braucht. Bedingungen, alle noetig:

* der Wert ist eine Gleitzahl, hat **keinen** Platz im Register bekommen,
  ist keine Zelle, kein Alias, keine unmittelbare Konstante, nicht `secret`,
* er wird **genau einmal** gelesen,
* Erzeuger UND Leser stehen im Fliesskommaweg der Ausgabe (dieselbe
  Bedingung, die den Fehler aus Runde XMM 3 abgestellt hat),
* der Leser steht im selben Block, hoechstens sechzehn Anweisungen weiter,
  und dazwischen liegt **kein Aufruf** — jedes `xmm` ist caller-saved.

Ueberschneidungen gibt es wirklich (`t1` wird erzeugt, dann `t2`, erst
danach werden beide gelesen); deshalb steht dahinter eine kleine
Intervallverteilung und kein einzelnes Register.

**324 Mio -> 308 Mio Befehle, 0,29 s -> 0,25 s.**

## 2. Fliesskomma-Zellen in `xmm` (ehrlich: hier ohne Wirkung)

Eine Zelle ist eine `alloca`, die eine Variable haelt. Bis jetzt konnte sie
nur ein GANZZAHLregister bekommen — eine `f32`-Summe reiste dann bei jedem
Zugriff per `movd` zwischen `r13` und der Recheneinheit hin und her. Jetzt
darf eine Zelle, die eine Gleitzahl haelt und nur im Fliesskommaweg angefasst
wird, ein `xmm` bekommen; Laden und Schreiben sind dann eine Kopie oder gar
nichts.

**Gemessen im Dekoder: null Wirkung** — und der Grund ist ehrlich zu nennen:
`mem2reg` befoerdert diese Variablen laengst zu gewoehnlichen Werten mit
`phi`, es gibt in `lib/ton/mp3.fi` bei `release-fast` **keine** einzige
Fliesskomma-Zelle (`FIRN_FPCELL_DEBUG=1` sagt es je Funktion). Uebrig bleibt
die Wirkung dort, wo `mem2reg` nicht laeuft — `--opt-level=dev` — und in
Funktionen, deren `alloca` aus anderen Gruenden stehen bleibt. Der Code
bleibt drin, weil er nichts kostet und die Klassenverwechslung, die es
vorher gab (`movd` durch ein Ganzzahlregister), ohnehin niemand will.

## 3. `--cpu=avx`: die Dreioperandenform (−11 % Befehle, −8 % Zeit)

`vmulss d, a, b` nennt sein Ziel selbst. Damit entfaellt die Kopie, die SSE
erzwingt. Neu ist der Schalter `--cpu=<baseline|avx>` (Voreinstellung
`baseline`, also SSE2 wie bisher; `FIRN_CPU=avx` wirkt genauso, damit die
volle Testreihe in beiden Stufen laufen kann).

Zwei Teile:

1. **Die Umschrift.** `addss d, s` und `vaddss d, d, s` bedeuten dasselbe;
   `vexify` in `codegen_x86.rs` schreibt jede SSE-Anweisung des Weges in ihre
   VEX-Form um. Das bringt selbst **kein** Tempo — es verhindert, dass in
   einer Funktion beide Formen gemischt stehen. Auf Intel kostet jeder
   Wechsel zwischen altem SSE und VEX zweistellige Taktzahlen und wuerde den
   Gewinn auffressen.
2. **Die echte Dreioperandenform** in der Rechnung selbst (`emit_bin`,
   Vorzeichenumkehr): erste Quelle ein Register, zweite darf Speicher sein,
   Ziel frei — die Kopie verschwindet.

Nur der Weg mit Registerzuteilung schreibt VEX. Der Grundweg bleibt bei SSE,
weil dort `simd.rs` `v128`-Werte in den Registern haelt und eine Umschrift von
`movss` auf `vmovaps` deren obere Haelfte ausloeschen wuerde.

Eine Feinheit ist dokumentiert: `movss xmm1, xmm2` laesst die oberen 96 Bit
stehen, `vmovaps xmm1, xmm2` nicht. Auf diesem Weg ist das gleichgueltig, weil
er nur Skalare haelt.

## Die Zahlen

MP3-Dekoder, 60 s Ton (192 kbit/s, Stereo), `release-fast`:

| | Befehle (8 s Ton) | Zeit (60 s Ton) |
|---|---|---|
| Runde XMM 3 (Stand 20.09., mittags) | 644 Mio | 0,88 s |
| Runde TEMPO 1 | 324 Mio | 0,29 s |
| **TEMPO 2, `--cpu=baseline`** | **308 Mio** | **0,25 s** |
| **TEMPO 2, `--cpu=avx`** | **275 Mio** | **0,23 s** |
| `minimp3` in C, `gcc -O2` | 75 Mio | 0,07 s |
| dasselbe C, `-O2 -fno-tree-vectorize` | — | 0,10 s |
| dasselbe C, `-O0` | — | 0,39 s |

**Abstand zu C:** 3,3x gegen `gcc -O2` (das seine Schleifen selbst
vektorisiert), **2,3x** gegen dasselbe C ohne Vektorisierung. Gegen `-O0` ist
Firn schneller.

Ein reiner `f32`-Rechenkern (vier Summen, acht Fensterpaare, `bench/kern.c`
gegen dieselbe Schleife in Firn):

| | Zeit |
|---|---|
| Firn `--cpu=baseline` | 0,09 s |
| Firn `--cpu=avx` | 0,08 s |
| C `gcc -O2` | 0,03 s |
| C `gcc -O2 -mavx2` | 0,02 s |

Richtigkeit: die PCM-Ausgabe ist in **allen** Stufen bitgleich
(`cmp` ueber 10,6 MB, in `baseline` wie in `avx`), der Selbsttest des
Dekoders gibt PASS 4/4. `bash test.sh` laeuft ohne neuen Fehler, in
`baseline` und mit `FIRN_CPU=avx`.

## Was jetzt noch zwischen Firn und C steht

Gemessen, nicht geraten:

1. **Vektorisierung.** 30 % des Vorsprungs von `gcc -O2` kommen daher, dass
   es die Schleifen selbst in `packed`-Befehle giesst (vier `f32` je
   Anweisung). Firn hat `v128` als Sprachmittel, aber keinen Pass, der es
   von sich aus benutzt.
2. **Lebensdauern an Aufrufen zerschneiden.** Wer einen Aufruf ueberlebt,
   bekommt heute gar kein Register. In `synth` stehen deshalb drei
   Basiszeiger im Rahmen und werden vor jedem Zugriff neu geholt.
3. **Anordnung der Befehle.** Firn gibt die Anweisungen in FIR-Reihenfolge
   aus. Ein Laden dicht vor seinen Verbraucher zu ziehen, senkt den Druck auf
   die Register und damit die Zahl der Werte, die ueberhaupt in den Rahmen
   muessen.

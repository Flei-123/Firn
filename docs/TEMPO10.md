# Runde TEMPO 10 — drei Antworten auf dieselbe Frage: wem gehoert das Register?

Stand 23.09.2026, Zweig `xmm-ra`. Ausgangspunkt war die Zaehlung nach
TEMPO 9 — nicht je Funktion, sondern **je Muster ueber das ganze Programm**
(`/tmp`-Werkzeug, Vorlage in TEMPO 9):

| Muster | Befehle | Anteil |
|---|---|---|
| **`movaps xmm,xmm`** (SSE-Zweioperandenform) | **16,3 Mio** | **11,1 %** |
| **aus dem Rahmen holen** | **10,3 Mio** | **7,0 %** |
| `mov rA,rB` + `add $K` (statt `lea`) | 2,1 Mio | 1,4 % |
| `mov $K,r` + `imul` (statt `lea`/`shl`) | 0,8 Mio | 0,6 % |

Die beiden ersten sind dieselbe Frage in zwei Kleidern: **wer bekommt ein
Register, und wie lange behaelt er es.** Diese Runde gibt drei Antworten.

---

## 1. Die Zweioperandenform ist auch eine Kopie (16,3 → 8,7 Mio)

SSE hat keine Dreioperandenform: `mulps d, s` heisst `d = d * s`. Der
Erzeuger kopiert deshalb erst den ersten Operanden ins Ziel und rechnet dann:

```text
movaps %xmm13,%xmm7
mulps  %xmm12,%xmm7
```

Stirbt `%xmm13` bei dieser Multiplikation, ist die Kopie fuer nichts — `a`
und `d` duerfen dasselbe Register haben. Das ist **genau die Frage, die das
Verschmelzen aus TEMPO 8 schon beantwortet**, nur fuer eine andere
Anweisungsart. Die Kandidatensuche nimmt jetzt zusaetzlich

* `Op::Bin(+,-,*,/)` mit Gleitzahltyp,
* `Op::Un(Neg)` mit Gleitzahltyp,
* jeden `Op::Simd`, der in seinem ersten Operanden rechnet
  (`addps`, `subps`, `mulps`, die drei Vergleiche, `pcmpgtd`, `pand`,
  `pandn`, `por`, `pxor`, `paddd`, `psubd`, `punpckldq/hi`).

**Die Bedingung musste dafuer geschaerft werden**, und das war der lehrreiche
Teil. Fuer eine echte Kopie hiess sie "die Quelle hat genau einen Leser".
Fuer die Zweioperandenform ist das zu streng: in der heissen Schleife der
Synthesefilterbank wird derselbe Vektor ZWEIMAL multipliziert, und beim
zweiten Mal stirbt er — genau dort darf das Ziel sein Register erben. Die
richtige Frage ist nicht, wie oft der Wert gelesen wird, sondern ob er **nach
dieser Anweisung noch lebt**. Mit "genau ein Leser": 146,3 → 141,6 Mio. Mit
"stirbt hier": 146,3 → **138,8 Mio**.

Dieselbe Lockerung gilt seitdem auch fuer echte Kopien: die Chaitin-Frage
("stoeren sie sich?") ist die vollstaendige Bedingung, "genau ein Leser" war
nur Vorsicht. `FIRN_COAL_ENG=1` stellt die vorsichtige Fassung wieder her
(gemessen 1,0 Mio schlechter).

Dazu kommt, dass jetzt auch **zwei Rahmenplaetze** verschmolzen werden, aber
**nur fuer Ganzzahlen**: bei Gleitzahlen legt `fp_handover` einen Wert ohne
Register in `xmm2` statt in den Rahmen, und die Kopie haette dann geglaubt,
Quelle und Ziel seien derselbe Platz. An genau dieser Falle ist TEMPO 8 schon
einmal gestorben (`tests/1182_layout_float_probe.fi`).

---

## 2. Dichte statt Summe (137,7 → 136,7 Mio, und der Tueroeffner)

Wenn kein Register frei ist, verdraengt der lineare Scan das aktive Intervall
mit dem **kleinsten Gewicht** (Verwendungen mal Schleifentiefe). Das
bevorzugt lange Intervalle: ein Wert mit fuenfzig ueber die ganze Funktion
verstreuten Verwendungen schlaegt einen mit dreien in der innersten Schleife
— obwohl der erste sein Register die ganze Zeit belegt und der zweite es nur
kurz braeuchte.

Verglichen wird jetzt **Gewicht je Laenge**. Eine Zeile Aenderung,
`FIRN_RA_SUMME=1` stellt die alte Antwort wieder her.

---

## 3. Lebensdauern zerschneiden — und warum der erste Anlauf falsch war

Der lineare Scan kennt je Wert EIN Intervall und EINEN Platz. Ein Zeiger, der
am Anfang gesetzt und am Ende noch einmal gebraucht wird, belegt sein
Register ueber die ganze Funktion — oder keines, und dann wird er in der
heissen Schleife dazwischen bei JEDER Verwendung aus dem Rahmen geholt.

Die Lehrbuchantwort ist *live range splitting*. Im Zuteiler selbst waere das
ein Umbau jeder Ausgabestelle (`loc(v)` muesste von der POSITION abhaengen).
Dasselbe Ergebnis bekommt man ohne diesen Umbau, indem man das Stueck zu
einem eigenen WERT macht: eine Kopie in den Vorkopf der Schleife, und der
Rumpf liest die Kopie.

**Erster Anlauf: ein Pass im Optimierer.** Er schnitt jeden Wert, der in
einer Schleife mehrfach gelesen und dort nicht geschrieben wird. Gemessen:
137,7 → **140,4 Mio, also zwei Prozent SCHLECHTER**. Der Grund ist im
Nachhinein offensichtlich: wo der neue Wert auch nur einen Rahmenplatz
bekommt, zahlt man die Kopie im Vorkopf und gewinnt nichts — der Rumpf liest
dann eben den anderen Platz. Erhoehen der Schwelle half nicht (min=8: immer
noch 138,0).

**Zweiter Anlauf: schneiden, nachdem man weiss, wo es klemmt.**
`emit_func_ra` teilt jetzt einmal zu, fragt `split::nach_zuteilung`, welche
Werte wirklich **im Rahmen gelandet sind UND in einer Schleife mehrfach
gelesen werden**, schneidet nur diese, und teilt noch einmal zu. Bekommt beim
zweiten Zuteilen kein einziger der neuen Werte ein Register, wird das
Ergebnis verworfen — dann kostet der Schnitt nur Uebersetzungszeit und kein
einziges Bit im Programm.

**Und dann passierte genau das: null Register, jedes Mal.** Zwei Ursachen,
nacheinander gefunden:

1. **Das Verschmelzen machte den Schnitt sofort wieder zu.** `%v2 = copy %v`
   ist fuer TEMPO 8 ein perfekter Kandidat. Dafuer gibt es jetzt
   `Func::no_coalesce` — eine Liste von Werten, die nicht verschmolzen werden
   duerfen, gefuellt allein vom Zuteiler fuer seine eigene Zweitfassung.
2. **Auch danach: null.** Und das war kein Fehler, sondern die Antwort des
   Zuteilers. Mit der SUMME als Massstab verliert ein kurzes Intervall mit
   drei Verwendungen gegen ein langes mit fuenfzig — immer. Erst mit der
   Dichte aus Punkt 2 gewinnt der Schnitt seine Register.

Die beiden Aenderungen haengen also zusammen: **Dichte ohne Schnitt bringt
1,0 Mio, Schnitt ohne Dichte bringt nichts, beide zusammen 2,3 Mio.**

Schwelle: drei Leser in der Schleife (`FIRN_SPLIT_MIN`), abschaltbar mit
`FIRN_NO_SPLIT=1`.

---

## Die Zahlen

MP3-Dekoder, 8 s Ton, `release-fast`, `valgrind --tool=callgrind`:

| | Befehle |
|---|---|
| nach TEMPO 9 | 146,3 Mio |
| + Zweioperandenform | 138,8 Mio |
| + aggressives Verschmelzen, Plaetze | 137,7 Mio |
| **+ Dichte + Schnitt (TEMPO 10)** | **135,4 Mio** |
| dasselbe mit `--cpu=avx` | **127,1 Mio** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio |

Und die Wanduhr, 60 s Ton, kleinste von elf Laeufen, Ausgabe nach
`/dev/null`:

| | Zeit |
|---|---|
| Firn nach TEMPO 8 | 0,15 s |
| **Firn jetzt** | **0,13 s** |
| Firn mit `--cpu=avx` | 0,12 s |
| C, `gcc -O2` | 0,06 s |
| dasselbe C ohne Auto-Vektorisierung | 0,09 s |
| dasselbe C mit `-O0` | 0,36 s |

Also **2,2x hinter `gcc -O2`** (mit AVX 2,0x) und **1,4x** hinter demselben
C ohne Auto-Vektorisierung. Zu Beginn der Tempo-Runden waren es 8,3x.

Die Bank (`bench/firn/`) zeigt keine Verschlechterung: zehn von elf
Programmen Befehl fuer Befehl gleich, `jsonscan` −1,8 %. Diese Programme
haben kaum Registerdruck — dort gibt es nichts zu verteilen.

Die Uebersetzung von `bin/firnc1.fi` dauert 4,9 statt 4,5 Sekunden (+7 %);
das ist die zweite Zuteilung fuer Funktionen mit Schleifen und Ueberlauf.

## Geprueft

Alle 318 Testprogramme in vier Baustufen, `self_compare` 339 von 339 mit
gleichem Verhalten (0 abweichend, 0 fehlerhaft), und der **Fixpunkt**: Firn
uebersetzt sich selbst, Stufe 2 und Stufe 3 zeichengleich (793 453 Zeilen
Assembler). Die PCM-Ausgabe des Dekoders ist nach jedem einzelnen Schritt
bitgleich, in beiden CPU-Stufen. Die drei roten Punkte des Laufs
(`tools/js/run.sh` an einer fehlenden `testdata/test262/subset.sha256`,
`tools/fmt/run.sh` an ungeformten Dateien in `lib/fui/`,
`tools/english/check.sh` an Bezeichnern in `lib/fui/`) sind aelter als diese
Runde; die Bezeichner IM UEBERSETZER sind seit dieser Runde alle englisch
(25 -> 18 gemeldete, keiner mehr unter `compiler/src/`).

## Was jetzt noch dasteht

| Muster | Befehle |
|---|---|
| aus dem Rahmen holen | 10,0 Mio |
| `movaps xmm,xmm` (die Quelle lebt wirklich weiter) | 8,7 Mio |
| `mov rA,rB` + `add $K` | 2,0 Mio |
| `mov $K,r` + `imul` | 0,8 Mio |

Die letzten beiden sind der naechste, einfache Schritt: `lea` darf auch mit
32-Bit-Ziel benutzt werden (`lea %edx,0x1(%r10)` rechnet die Adresse in 64
Bit und schneidet auf 32 — genau die Arithmetik modulo 2^32, die ein 32-Bit
`add` macht), und bei einer vertauschbaren Rechnung gehoert die Konstante
nach rechts, damit die vorhandene Faltung sie sieht.

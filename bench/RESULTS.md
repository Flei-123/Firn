# Benchmark-Ergebnisse (real gemessen)

Erzeugt von `bench/run.sh` (`bench/bench.py`), 5 Laeufe je Programm, **Median**.
Jeder Benchmark existiert zweimal — `bench/firn/<name>.fi` und `bench/rust/<name>.rs` — und beide geben ihr Ergebnis aus; die Ausgaben muessen uebereinstimmen, sonst bricht die Messung ab.
Die Rust-Seite benutzt `std::hint::black_box` und dieselben ungeprueften Zeigerzugriffe wie die Firn-Seite, damit dieselbe Arbeit gemessen wird.

* CPU: AMD EPYC 7571 32-Core Processor
* System: Linux 7.0.14-5-pve x86_64
* rustc 1.99.0-nightly (c98d0cb27 2026-08-12)
* Firn: eigener Codegenerator, keine externen Crates

| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Faktor Firn/Rust | Gewinn durch Optimierer | Ergebnis |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.056 s | 0.143 s | 0.032 s | **1.75x** | 2.54x | 4356618 |
| sieve | 0.119 s | 1.444 s | 0.032 s | **3.68x** | 12.10x | 697026 |
| matmul | 0.111 s | 2.168 s | 0.025 s | **4.48x** | 19.55x | 8291727 |
| bytecount | 0.507 s | 5.605 s | 0.193 s | **2.63x** | 11.05x | 1604208 |
| bubblesort | 0.103 s | 1.344 s | 0.039 s | **2.63x** | 13.10x | 12021846167 |
| statemachine | 0.253 s | 1.286 s | 0.085 s | **3.00x** | 5.08x | 6710880 |

Median ueber alle Benchmarks: **2.82x** langsamer als Rust `-O` (Spanne 1.75x – 4.48x).
Der Optimierer (mem2reg, CSE, Inlining, Registerzuteilung) bringt im Median **11.57x** gegenueber `--no-opt`.

---

## A/B-Messung der Optimierer-Runde (14.08.2026)

Die Faktoren oben schwanken mit der **Rust**-Zeit: auf dieser Maschine liefert
dieselbe Binary zwischen zwei Läufen bis zu **40 % Unterschied**. Eine
Codegen-Änderung von 5 % lässt sich damit nicht bewerten — beim ersten Versuch
erschien dieselbe Verbesserung einmal als −18 % und einmal als +6 %.

Deshalb wird der Fortschritt am Compiler seit dieser Runde über die
**ausgeführten Instruktionen** gemessen (`bench/instr.sh`, `valgrind
--tool=callgrind`). Die Zahl ist auf die Instruktion genau reproduzierbar.

**Stand `e517942` (vor der Runde) gegen `26861e3`+ (LICM, `lea`, Inline-Grenze):**

| Programm | Instruktionen vorher | nachher | Änderung |
|---|---:|---:|---:|
| matmul | 1.668.312.681 | 1.376.734.921 | **−17,48 %** |
| bubblesort | 811.682.925 | 667.321.089 | **−17,79 %** |
| bytecount | 2.579.216.109 | 2.148.310.351 | **−16,71 %** |
| sieve | 825.458.961 | 708.292.727 | **−14,19 %** |
| statemachine | 1.847.172.267 | 1.721.343.055 | **−6,81 %** |
| fib | 338.351.740 | 338.353.992 | ±0,00 % |

`fib` ist reine Rekursion ohne Schleifen und ohne Feldzugriffe — dort gibt es
für LICM und `lea` nichts zu holen. Das Ergebnis ist kein Fehler, sondern die
Probe aufs Exempel: die Durchgänge greifen genau dort, wo sie sollen.

**Ehrliche Grenze der Metrik:** Instruktionen sind nicht Laufzeit. Ein `lea`
und ein `div` zählen beide als eine. Für die Frage „erzeugt der Compiler
weniger Arbeit?" ist sie richtig, für die Frage „wie schnell ist es?" nicht.

## Warum der Tokenizer davon nicht schneller wird

Gemessen am Korpus `realweb` (4.931.819 Bytes), beide Seiten mit callgrind:

| | Instruktionen | je Byte |
|---|---:|---:|
| Firn-Tokenizer | 4.033.688.605 | **818** |
| html5ever | 540.567.170 | **110** |

Das Verhältnis **7,46×** deckt sich fast genau mit dem gemessenen Zeitfaktor
(**7,04×**). Damit ist belegt, woran der Abstand NICHT liegt: nicht an der
Qualität des erzeugten Codes. Firn **führt siebeneinhalb Mal so viel Arbeit
aus**. Ein perfekter Codegenerator würde daran nichts ändern.

Die Ursachen liegen im Tokenizer und im Messaufbau, nicht im Compiler:

1. **Firn dekodiert die Eingabe zuerst vollständig nach UTF-32** (`mem.CpBuf`,
   4 Byte je Zeichen) und tokenisiert dann diesen Puffer. html5ever arbeitet
   direkt auf den Bytes. Das ist ein kompletter zusätzlicher Durchlauf über die
   Eingabe plus der vierfache Speicherverkehr.
2. **Kein Bulk-Pfad für Textläufe.** html5ever sucht das nächste `<`, `&` oder
   `\0` und gibt alles dazwischen als einen Block aus. Firn geht jedes Zeichen
   einzeln durch die volle Zustandsmaschine — genau deshalb ist der Abstand auf
   `realweb` (lange Texte) mit 7,0× viel größer als auf `html5lib` (fast nur
   Grenzfälle) mit 2,8×.
3. **Der Firn-Lauf schreibt zusätzlich das html5lib-JSON**, html5ever zählt nur
   Token. Diese Arbeit steckt vollständig in den 818 Instruktionen je Byte.

**Folgerung für die Roadmap:** Das Abnahmeziel „≤ 2× Referenz" ist mit
Compilerarbeit allein nicht erreichbar. Der nächste Schritt gehört dem
Tokenizer (Byte-Pfad statt Codepoint-Puffer, Blockverarbeitung für Textläufe)
und einem fairen Messaufbau (gleiche Ausgabe auf beiden Seiten).

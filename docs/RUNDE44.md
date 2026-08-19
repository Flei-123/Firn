# Runde 44 — die Aufbauphase des Sammlers entschärft

Auftrag: **keine Pause über 4 ms mehr, über den ganzen Lauf einschließlich
Aufbau**, angestrebt ≤ 2 ms, bei höchstens 10 % Durchsatzverlust.

Ergebnis vorweg: die längste Unterbrechung fiel von **11,82 ms auf 0,45 ms**
(5-s-Lauf, ruhige Maschine) bzw. **0,62 ms reine Rechenzeit** im
10-Minuten-Dauerlauf; der Durchsatz verlor **2 %** bei kleiner und **0 %** bei
großer lebender Menge. Der Grund für die alte Schwelle `INKR_AB = 8 MiB` war
richtig gemessen, aber falsch zugeordnet — die eigentliche Ursache saß im
gestückelten Fegen und ist jetzt behoben.

## 1. Zuerst messen: was in der Aufbauphase wirklich passiert

`tools/gc_mess/pause_gross.fi` konnte die Frage nicht beantworten: es ruft
`gc_hist_reset()` **nach** dem Aufbau, das Histogramm zeigt also nur den
Dauerbetrieb. Neu:

* **`tools/gc_mess/aufbau.fi`** — nullt nichts. Phase 0 ist die Bilanz nach
  dem Aufbau, Phase 1 die Bilanz über den GANZEN Lauf; der Dauerbetrieb ist
  die Differenz. Jeder Sammellauf während des Aufbaus wird einzeln gemeldet
  (mit Heapgröße und Knotenzahl an dieser Stelle).
* **`tools/gc_mess/durchsatz.fi`** — feste ARBEIT, gemessene ZEIT (die
  Pausenläufe messen umgekehrt und taugen für Durchsatz nicht).
* **`tools/gc_mess/ab.fi`** — A/B im selben Prozess, siehe §4.

Dazu vier Ergänzungen in der Laufzeit (`lib/gc/gc.fi`):

* **Histogrammtyp 7 = die ganze Unterbrechung an einer Allokationsstelle.**
  Die Typen 0–5 messen einzelne Abschnitte (Start, Mark, Sweep, Ende) und
  untertreiben, sobald in EINEM Aufruf mehrere Abschnitte hintereinander
  laufen. Ein Bild sieht nicht die Scheibe, sondern die Summe. `gc_stop_max()`
  gibt das Maximum dieses Typs.
* **Zweite Uhr `gc_set_cpu_uhr(1)`** — dieselbe Unterbrechung ein zweites Mal,
  gemessen mit `CLOCK_THREAD_CPUTIME_ID` (`gc_chist(fach)`,
  `gc_cpu_stop_max()`). Weicht die Wanduhr nach oben ab, war es die Maschine;
  stimmen beide überein, war es der Sammler. Siehe §8 — ohne diese Uhr wäre
  diese Runde in denselben Fehlbefund gelaufen wie Runde 40.
* **`gc_diag(0..6)`** — neue Chunks in Ruhe / im Markieren / im Fegen,
  Mark-Scheiben, Sweep-Scheiben, graue Allokationen, Allokationen gesamt.
* **`gc_set_inkr_ab(n)` / `gc_inkr_ab()`** — die Umschaltschwelle zur
  Laufzeit stellbar. Das ist die Grundlage der A/B-Messung in §4.

### Ausgangsmessung (Stand `main` f48e51c, `INKR_AB` = 8 MiB)

`aufbau.fi`, 120 000 lebende Textknoten, Aufbau bis Heap 9,5 MiB:

| Sammellauf | Pause | Heap dabei | Knoten dabei | Art |
|---|---|---|---|---|
| 1 | **0,88 ms** | 1,5 MiB | 13 106 | VOLL (Stop-the-World) |
| 2 | **2,90 ms** | 2,6 MiB | 26 214 | VOLL |
| 3 | **11,81 ms** | 4,7 MiB | 52 430 | VOLL |
| 4 | 0,14 ms | 8,9 MiB | 105 278 | inkrementell |

Vier Wiederholungen: 10,80 / 10,88 / 11,15 / 11,84 ms für Lauf 3 — der Wert
ist stabil, kein Ausreißer. Die Aufbauphase dauert 65–77 ms; über ein Drittel
davon ist ein einziger Stop-the-World-Lauf.

Über den ganzen Lauf (Aufbau + 5 s Dauerbetrieb), Histogramm Typ 7 (ganze
Unterbrechung, Fach k = [2^(k−1) µs, 2^k µs)):

```
Fach  2   3   4   5   6      7    8   9  10  12  14
Zahl  1   4   6  10  54  44390  345  76   2   1   1
                                            ^   ^
                               2,05-4,10 ms |   | 8,19-16,38 ms
```

Zwei Unterbrechungen über 2 ms, eine davon über 8 ms. `gc_stop_max` =
**11,82 ms**. Das ist genau der Befund, der zu beseitigen war.

## 2. Die Begründung von `INKR_AB` nachgeprüft — und widerlegt

`docs/RUNDE38.md` begründet die Schwelle so: rein inkrementell kostete
−13 bis −26 % Durchsatz bei kleinem Heap, „reiner Verlust"; der Hybrid kostet
nur −8,7 % für den „Phasen-Check je Allokation".

**Erster Befund, am Quelltext:** der Phasen-Check läuft ohnehin bei JEDER
Allokation, unabhängig von der Schwelle. `__gc_alloc_raw` prüft
`S_PHASE == 0 && S_SEIT >= S_GRENZE` und danach `S_PHASE != 0` — beides
bedingungslos. Die Schwelle spart ihn nicht. Die −8,7 % (Stufe 2 → Hybrid)
sind also bereits bezahlt und kein Argument für die Schwelle. Zu erklären
blieben die zusätzlichen −8,1 % (Hybrid → rein inkrementell, 7 110 000 →
6 533 000 Zyklen in Runde 38).

**Nachmessung** (`durchsatz.fi`, 2000 lebende Knoten, 200 000 Runden,
Heap 1,5 MiB — das Regime, das die Schwelle schützen soll):

| Variante | Arbeit | Heap Ende | Sammelzeit | RSS |
|---|---|---|---|---|
| `INKR_AB` = 8 MiB (atomar) | 121 ms | 1,57 MiB | 26,0 ms | 1612 KiB |
| `INKR_AB` = 0 (inkrementell) | 163 ms (**−26 %**) | 3,15 MiB | 61,0 ms | 3148 KiB |

Der Verlust ist also reproduzierbar. Aber der Heap **verdoppelt** sich dabei,
und die Sammelzeit steigt um mehr als das Doppelte. Die Zähler `gc_diag`
zeigen, woher:

| Diagnose | atomar | inkrementell |
|---|---|---|
| neue Chunks in Ruhe | 119 | 6 |
| neue Chunks im Markieren | 0 | 0 |
| neue Chunks **im Fegen** | 0 | **113** |
| Mark-Scheiben | 0 | 429 |
| Sweep-Scheiben | 0 | 614 |
| graue Allokationen | 0 | 949 von 1 402 001 (0,07 %) |

**Die Ursache ist nicht das inkrementelle Verfahren.** Die vermuteten
laufenden Kosten — graue Allokation und aktive Einfügebarriere — treffen
0,07 % aller Allokationen und sind messbar bedeutungslos. Die Kosten kamen
aus `__gc_sweep_init()`: es leerte **alle Freilisten auf einen Schlag**. Das
Fegen läuft aber in Scheiben, zwischen den Scheiben alloziert das Programm
weiter — also fiel jede Allokation bis zum Ende des Fegens auf eine leere
Liste und musste einen frischen Chunk mmapen. 113 von 119 neuen Chunks
entstanden so. Der Heap wächst, und weil `__gc_block_von` die Chunkliste
linear absucht, wird damit **jede Markierung teurer** — daher die doppelte
Sammelzeit.

Gegenprobe, die die Zuordnung festnagelt: mit `SCHEIBE_SWEEP = 1 000 000`
(das ganze Fegen in EINER Scheibe, sonst unverändert inkrementell) fiel die
Arbeit von 163 auf 141 ms, der Heap von 3,15 auf 2,36 MiB, die Sammelzeit von
61 auf 47 ms. Der Rest steckte im Übergang Markieren → Fegen: `sweep_init`
leert die Listen und der Schritt kehrt zurück, die auslösende Allokation
findet garantiert leere Listen vor.

## 3. Die Änderung

Drei Eingriffe in `lib/gc/gc.fi`, alle im Fegen:

1. **Freilisten klassenweise und verzögert leeren.** `__gc_sweep_init()`
   leert nichts mehr, sondern setzt nur Merker (`S_SWKL`, je Größenklasse
   einer). Die Freiliste einer Klasse wird geleert, wenn der **erste Chunk
   dieser Klasse** gefegt wird.
   Das ist sicher: danach enthält die Liste nur noch Blöcke bereits gefegter
   Chunks derselben Klasse. Ein Block eines noch nicht gefegten Chunks fällt
   beim Leeren aus der Liste und kommt erst wieder hinein, wenn sein Chunk
   gefegt ist — doppelt vergeben werden kann er nie. Auch die Rückgabe eines
   ganz toten Chunks bleibt gültig: der Rücksprung des Listenkopfes
   (`alt_kopf`) schneidet weiterhin genau dessen zusammenhängendes Segment
   ab, und aus dem alten Listeninhalt kann kein Zeiger mehr in diesen Chunk
   zeigen.
2. **Fegen mit Zeitbudget.** Das Fegen hängt jetzt am selben
   `ZEIT_BUDGET_NS` (100 µs) wie das Markieren seit Runde 41, statt an zwei
   Chunks je Scheibe. `SCHEIBE_SWEEP` sagt nur noch, nach wie vielen Chunks
   die Uhr befragt wird. Ein kleiner Heap wird damit in EINER Scheibe fertig —
   und nur dann sieht die auslösende Allokation wieder volle Freilisten.
3. **`INKR_AB` = 0**, also inkrementell ab dem ersten Zyklus; zur Laufzeit
   mit `gc_set_inkr_ab()` stellbar (Voreinstellung bleibt der Konstantenwert).

## 4. Durchsatz: A/B im selben Prozess

Der zu messende Unterschied liegt bei wenigen Prozent. Zwei getrennt gebaute
Programme unterscheiden sich schon durch Codelayout um mehr als das — in
dieser Runde gemessen: dieselbe Messung lief nach dem Einbau reiner
Diagnosezähler von 121 auf 104 ms, also 14 % **schneller**, obwohl mehr Code
lief. Wer so vergleicht, misst Zufall.

`tools/gc_mess/ab.fi` vergleicht deshalb im **selben Prozess**, mit
demselben Maschinencode und derselben lebenden Menge: `gc_set_inkr_ab()`
schaltet um, die Phasen laufen verschachtelt (A B A B A B), damit eine Drift
der Maschine beide gleich trifft.

**2000 lebende Knoten, 100 000 Runden je Phase, zwei Läufe à drei Paare:**

| | atomar (A) | inkrementell (B) |
|---|---|---|
| Arbeit, Einzelwerte | 52, 48, 48, 52, 56, 52 ms | 52, 53, 53, 54, 61, 53 ms |
| **Median** | **52 ms** | **53 ms** (−2 %) |
| Heap | 1,57 MiB | 1,57 MiB (gleich) |
| Sammelzeit je Phase | 10,7–12,3 ms | 13,7–15,9 ms |
| längste Unterbrechung | 296–371 µs | **120–148 µs** |

**120 000 lebende Knoten, 60 000 Runden je Phase:**

| | atomar (A) | inkrementell (B) |
|---|---|---|
| Arbeit | 529, 514, 530 ms | 523, 525, 526 ms |
| **Median** | **529 ms** | **525 ms** (+0,8 %) |

Der Heap ist hier von Anfang an über der alten Schwelle; beide Varianten
laufen also im Dauerbetrieb gleich, der Unterschied läge nur im Aufbau. Dass
sie gleich schnell sind, ist die Kontrolle, dass die Messung sauber ist.

Aus −26 % (vor der Änderung) wurden **−2 %**. Die Vorgabe von höchstens
−10 % ist eingehalten.

## 5. Ergebnis: Pausen über den ganzen Lauf

`aufbau.fi`, 120 000 lebende Knoten, Aufbau eingeschlossen, 5 s Dauerbetrieb,
ruhige Maschine:

| | vorher (`INKR_AB` 8 MiB) | nachher (`INKR_AB` 0) |
|---|---|---|
| volle Stop-the-World-Läufe im Aufbau | **3** | **0** |
| Pausen im Aufbau | 0,88 / 2,90 / 11,81 ms | 0,079 / 0,113 / 0,109 / 0,140 ms |
| Aufbaudauer | 76 ms | 74 ms |
| **längste Unterbrechung, ganzer Lauf** | **11,82 ms** | **0,45 ms** |
| Durchsatz Dauerbetrieb | 107,7 Zyklen/ms | 107,0 Zyklen/ms (−0,7 %) |

Histogramm Typ 7 (ganze Unterbrechung) über den ganzen Lauf:

```
vorher   Fach  2   3   4   5   6      7    8   9  10  12  14
         Zahl  1   4   6  10  54  44390  345  76   2   1   1
nachher  Fach  2   3   4   5   6      7    8   9
         Zahl  3   4   4  14  90  44216  314  74
```

Kein Fach über 9 mehr besetzt: **nichts über 512 µs**, über den ganzen Lauf
einschließlich Aufbau.

## 6. Nebenwirkung: der Speicherüberhang schrumpft

Runde 38 nannte als „ehrliche Nebenwirkung" des Hybrids im Phasen-Test
`rss_ende` = 14 912 KiB gegenüber 2112 KiB in Stufe 2 — Floating Garbage und
verzögerte Rückgabe. Mit dem verzögerten Leeren der Freilisten misst
`tools/gc_mess/run.sh` (3b, Phasen-Fragmentierung) jetzt **2368 KiB**. Der
Überhang war also größtenteils nicht Floating Garbage, sondern derselbe
Fehler: im Fegen frisch gemappte Chunks.

## 7. Abnahme

| Prüfung | Ergebnis |
|---|---|
| `bash ./test.sh` | **676/676** (673 Basis + neuer Test 771, jeder Test laeuft in drei Baustufen) |
| `bash tools/selbst_vergleich.sh` | **197** gleiches Verhalten, 0 abweichend, 0 fehlerhaft (196 Basis + Test 771) |
| `bash tools/fixpunkt.sh` | Stufe 2 == Stufe 3, zeichengleich (322 723 Zeilen Assembler) |
| `bash tools/gc_mess/run.sh` | Fragmentierung Drift +0,0 % (stabil), RSS Ende 2632 KiB, Phasen-Test Ende 2368 KiB, `volle_laeufe` 0 |
| Pausen-Histogramm ≥ 10 min, große lebende Menge, Aufbau eingeschlossen | §8 — Wanduhr max 2,14 ms, Rechenzeit max 0,62 ms, 0 volle Läufe |
| `tools/dom_soak` (in test.sh) | Verbrauch flach 1360 → 1360 KiB, Gegenprobe schlägt an |

Neuer Test `tests/771_gc_aufbau_ohne_stw.fi`: 200 000 lebende Knoten (Heap
über der alten Schwelle), geprüft wird deterministisch — nicht über die Uhr —
dass `gc_volle_laeufe() == 0`, dass überhaupt gesammelt wurde, dass der Heap
unter dem 2,5-fachen der lebenden Menge bleibt (gemessen 1,2-fach) und dass
die Kette vollständig und in der richtigen Reihenfolge ist. Gegenprobe: mit
`gc_set_inkr_ab(8388608)` — dem Verhalten bis Runde 43 — schlägt derselbe
Test fehl (Ausgang 3). Der Test kann also wirklich anschlagen.

## 8. Der Dauerlauf — und warum es zwei Uhren braucht

**Erster 10-Minuten-Lauf** (600 s, 120 000 lebende Knoten, Aufbau
eingeschlossen, 61,3 Mio. Zyklen, 7164 Sammelläufe, **0 volle Läufe**):
Wanduhr-Höchstwert **8,11 ms**, und im Histogramm Typ 7 standen **712
Unterbrechungen in [2,05; 4,10) ms und 81 in [4,10; 8,19) ms**. Nach den
Zahlen wäre das Ziel verfehlt gewesen.

Fast alle langen Werte waren **Markierscheiben** (Typ 1: Fach 12 = 682,
Fach 13 = 80). Eine Markierscheibe hat aber ein Zeitbudget von 100 µs, und
die Uhr wird alle 16 Objekte befragt. 16 Objekte sind rund 20 µs Arbeit
(16 × 8 Zeigerfelder × ~50 Chunks in `__gc_block_von`) — 4 ms Rechenarbeit
zwischen zwei Uhrabfragen sind schlicht nicht möglich. Parallel liefen auf
derselben Maschine zwei andere Runden; die 1-Minuten-Last lag im **Median bei
2,30, im Höchstwert bei 5,57**.

Genau dieser Fehlbefund hat Runde 40 eine Runde gekostet. Statt ihn zu
behaupten oder wegzureden, misst die Laufzeit ihn jetzt: `gc_set_cpu_uhr(1)`
bucht dieselbe Unterbrechung ein zweites Mal in **reiner Rechenzeit des
Fadens**.

**Zweiter 10-Minuten-Lauf, beide Uhren** (600,35 s, 65,5 Mio. Zyklen,
7662 Sammelläufe, **0 volle Läufe**, Heap 13,0 MiB, RSS 13 404 KiB,
0 Markstapel-Überläufe; Last Median 1,62, Höchstwert 2,60):

Aufbau: 4 Sammelläufe, **kein einziger voll**, Pausen 82 / 110 / 133 / 134 µs,
längste Unterbrechung im Aufbau 162 µs (Rechenzeit 161 µs).

| Fach (Obergrenze) | Wanduhr | Rechenzeit |
|---|---|---|
| 2 µs | 173 | 170 |
| 4 µs | 400 | 397 |
| 8 µs | 577 | 582 |
| 16 µs | 1 211 | 1 303 |
| 32 µs | 5 059 | 6 457 |
| 64 µs | — | — |
| 128 µs | 5 143 326 | 5 146 125 |
| 256 µs | 44 409 | 41 026 |
| 512 µs | 11 076 | 10 406 |
| 1,02 ms | 221 | **21** |
| 2,05 ms | 34 | **0** |
| 4,10 ms | 1 | **0** |
| darüber | **0** | **0** |

* **Rechenzeit: nichts über 1,02 ms.** Der Sammler selbst hält den Mutator
  also nie länger als eine Millisekunde auf — 5 206 487 Unterbrechungen, davon
  21 (0,0004 %) über 512 µs, keine über 1,02 ms.
  `gc_cpu_stop_max` = **0,62 ms**.
* **Wanduhr: nichts über 4,10 ms**, ein einziger Wert in [2,05; 4,10) ms.
  `gc_pause_ns_max` = **2,14 ms**. Die 35 Werte über 1 ms sind genau die
  Differenz zur Rechenzeit — Verdrängung durch die parallelen Runden.

Damit ist die harte Vorgabe (**keine Pause über 4 ms**) auch nach der
Wanduhr erfüllt und nach der Rechenzeit mit Faktor 6 Abstand; die
angestrebten ≤ 2 ms sind in Rechenzeit mit Faktor 3 Abstand erfüllt und in
Wanduhr bis auf einen einzigen, nachweislich maschinenverursachten Wert.

Zum Vergleich der Ausgangszustand im selben Format: dort lag der
Höchstwert bei **11,82 ms**, und er kam nicht von der Maschine, sondern von
drei vollen Stop-the-World-Läufen, die reproduzierbar in jedem Lauf an
derselben Stelle auftraten.

## 9. Offen

* **`__gc_block_von` sucht die Chunkliste linear ab.** Das ist der teuerste
  Pfad des Sammlers: im Lauf mit 120 000 lebenden Knoten stecken 1,65 s von
  1,73 s Arbeit im Sammler, und die Markierkosten wachsen mit der Zahl der
  Chunks. Ein sortiertes Chunk-Feld mit binärer Suche oder eine
  Seitentabelle würde Durchsatz UND Scheibenlänge deutlich senken. Nicht
  angefasst, weil diese Runde die Aufbauphase zum Gegenstand hatte und die
  Änderung den Allokator berührt.
* **Das Zeitbudget wird nur alle 16 Objekte (Mark) bzw. alle 2 Chunks
  (Sweep) geprüft.** In Rechenzeit reicht das heute (nichts über 1,02 ms),
  aber der Abstand zur 4-ms-Grenze ist damit von der Objektform abhängig.
  Wenn `__gc_block_von` schneller wird, sollte auch dieser Abstand neu
  vermessen werden.
* **Finalisierer und `Arc[T]`** — unverändert benannte Restarbeit aus
  Runde 38, in dieser Runde nicht angefasst.

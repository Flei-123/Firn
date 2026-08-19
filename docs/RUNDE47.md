# Runde 47 — Finalisierer, `Arc[T]`, schwache Verweise: die Restarbeit an der Speicherverwaltung

Branch `r47-arc`, Basis `a492d26`. Diese Runde schließt die drei Posten ab, die
seit Runde 4 unverändert als „offen" in `ABNAHME.md` und `docs/RUNDE44.md`
standen: **Finalisierer (`S4`)**, **`Arc[T]`** und **schwache Verweise, die
beim Einsammeln wirklich genullt werden (`S3`)**.

Ergebnis vorweg, alles selbst gemessen:

| | Basis (a492d26) | Runde 47 |
|---|---|---|
| längste Unterbrechung, **Rechenzeit**, Median aus 7 Läufen | 469 µs | **460 µs** |
| Durchsatz `aufbau.fi` (Zyklen in 5 s, Median) | 554 000 | **554 000** |
| Unterbrechungen über 1 ms in Rechenzeit (150-s-Lauf mit Finalisierern) | — | **0** von 253 698 |
| RSS über 140 s Dauerbetrieb mit 48 Mio. Finalisierern | — | **1372 KiB, driftfrei** |
| `test.sh` | 696/696 | **727/727** |

---

## 1. Finalisierer (`S4`)

### 1.1 Die Form, und warum sie nicht `fn finalize(inout self)` ist

`SPEC.md` §3.5.3 sagt `fn finalize(inout self)`. Stufe 0 hat weder Methoden
noch `inout` noch Funktionszeiger — und **indirekte Aufrufe gehören Runde 46**
(Interfaces/Vtables), nicht dieser. Die ehrliche Entsprechung ohne
Funktionszeiger sind zwei Teile:

```firn
gc_finalisierer_setzen(p, art)      // je OBJEKT eine Aufraeumart (1..16777215)
fn __gc_finalisiere(art: u64, p: *mut u8) { … }   // EIN Verteiler im Programm
```

Die Aufräumart steht **im Blockkopf**, nicht in einer Seitentabelle: das Wort
`[4..8)` trug bisher nur die Marke (0/1/2) und hat 30 Bits frei. Neu:

```
Bits 0..1   Marke        0 weiss, 1 grau, 2 schwarz
Bit  2      F_FIN        das Objekt hat einen Finalisierer
Bit  3      F_WART       es steht in der Warteschlange / läuft gerade
Bit  4      F_GETAN      sein Finalisierer ist gelaufen
Bits 8..31  Aufraeumart  24 Bit
```

Das kostet **kein Byte je Objekt** und macht die Erkennung beim Fegen zum Test
eines Bits in einem Wort, das dort ohnehin gelesen wird.

Deklariert die **Wurzeldatei** `fn __gc_finalisiere`, nimmt der Compiler sie;
sonst legt er eine leere Voreinstellung dazu. Das ist genau der Mechanismus,
mit dem schon `error AllocError` behandelt wird (Tokensuche in
`gc.rs::quelle_hat_allocerror`), und er ist in **beiden** Compilern gleich
gebaut (`gc.rs::quelle_hat_finalisierer` / `lib/firnc1/gc.fi::gc_quelle_scan`,
Bit 4). Nur die Wurzeldatei zählt: in einem Modul hieße die Funktion
`modul____gc_finalisiere` und die Laufzeit fände sie nicht mehr.

### 1.2 Wiederbelebung — die Entscheidung

Java lässt einen Finalisierer sein Objekt zurück in den lebenden Graphen
hängen. Der Sammler braucht dafür einen zweiten Zyklus, die Semantik ist
berüchtigt schwer, und `finalize()` gilt seit Java 9 als deprecated. **Firn
macht das nicht.** Die Entscheidung ist:

> Ein Finalisierer kann sein Objekt nicht am Leben halten. Der Block wird
> freigegeben, sobald der Finalisierer zurückkehrt — unabhängig davon, was er
> getan hat.

Das ist nicht nur eine Bitte an den Programmierer, sondern **erzwungen**:

1. **Vor** dem Aufruf werden alle `Gc[T]`- und `GcWeak[T]`-Felder des Objekts
   genullt. Ein Finalisierer sieht also nie einen Zeiger auf ein Nachbarobjekt,
   das im selben Lauf schon eingesammelt sein kann. Er sieht statt eines
   baumelnden Zeigers eine **0**, und die ist deterministisch.
2. `stark(w)` auf ein wartendes Objekt liefert 0 (Merker `F_WART`).
3. Eine **GC-Allokation** im Finalisierer bricht sichtbar ab (Rücksprungwert
   **71**), `gc_collect()` ebenso (**72**), und das Schreiben eines Gc-Zeigers
   in ein Heapfeld (**73**).

Die Meldungen, wörtlich:

```
firn-gc: allokation waehrend eines finalisierers (SPEC 3.5.3 S4)
firn-gc: gc_collect() waehrend eines finalisierers (SPEC 3.5.3 S4)
firn-gc: wiederbelebung im finalisierer (SPEC 3.5.3 S4)
```

**Punkt 3 löst zugleich die Reentranz-Frage.** Ein Finalisierer, der
alloziert, würde einen Sammellauf mitten in einem Sammellauf starten:
Warteschlange, Freilisten und Fegemerker sind in diesem Augenblick halb
fertig. Weil er gar nicht erst allozieren kann, ist der Fall nicht behandelt,
sondern **unmöglich**.

**Und es kostet den Normalfall nichts.** Die Sperre steckt in einer Prüfung,
die ohnehin am Anfang jeder Allokation steht: `S_INIT` war 0 („kein
`gc_init`") oder 1 („bereit") und ist jetzt 2, solange ein Finalisierer läuft.
Aus `== 0` wurde `!= 1` — dieselbe Instruktion. Der Test in der
Einfügebarriere sitzt **innerhalb** des Zweiges „es läuft gerade ein Zyklus",
den es schon gab.

### 1.3 Reihenfolge, Anzahl, Zeitpunkt

* **Zeitpunkt:** nachdem der Sammler das Objekt als unerreichbar erkannt hat.
  Der Zyklus hat dafür eine **eigene Phase 3**, die wie Markieren und Fegen in
  Scheiben läuft (Zeitbudget `ZEIT_BUDGET_NS`).
* **Reihenfolge zwischen zwei Objekten: keine Zusage.** Die Warteschlange folgt
  der Fegereihenfolge. Wer eine Reihenfolge braucht, nimmt keinen Finalisierer.
* **Höchstens einmal je Objekt** (`F_GETAN`).
* Ein Finalisierer selbst wird **nicht** unterbrochen. Seine Laufzeit gehört
  dem Programm, nicht dem Sammler, und wird getrennt ausgewiesen
  (`gc_fin_ns_max`, nur mit `gc_set_fin_uhr(1)`).

### 1.4 Die Warteschlange braucht keinen Speicher

Ein wartender Block ist tot; sein **Seriennummernwort** `[8..16)` wird nicht
mehr gebraucht. Dort steht der Verweis auf den nächsten Wartenden. Damit kann
die Warteschlange nicht überlaufen und keine Allokation auslösen — beides
wäre in einem Sammellauf fatal. Dass die Seriennummer dabei zerstört wird, ist
harmlos und sogar richtig: `stark()` prüft zusätzlich `F_WART`.

---

## 2. Schwache Verweise werden wirklich genullt (`S3`)

Bis Runde 46 war die Zusage „ein schwacher Verweis wird beim Einsammeln leer"
nur **scheinbar** erfüllt: `stark(w)` lieferte 0, weil die Seriennummer nicht
mehr passte — im Feld stand aber weiter das alte, verschleierte Bitmuster.

Die Typtabelle kennt die Offsets der `GcWeak[T]`-Felder seit Runde 4; sie waren
bis jetzt „nur für die Statistik". Seit Runde 47 nutzt das Fegen sie: für jedes
**lebende** Objekt werden die schwachen Felder durchgesehen, und ein Ziel, das
in diesem Lauf stirbt, wird auf den leeren Verweis gesetzt. Die Entscheidung
fällt über die **Marke** (weiß = tot), nicht über die Seriennummer — deshalb
ist sie unabhängig davon, ob der Zielblock schon gefegt wurde.

`tests/822_gc_weak_genullt.fi` sieht sich das **Rohwort** des Feldes an und
verlangt, dass es nach dem Sammellauf 0 ist.

**Was das nicht kann, offen benannt:** ein `GcWeak[T]` in einer *lokalen
Veränderlichen* wird nicht angefasst — der Sammler kennt den Stapel nur
konservativ und hat dort keine Feldkarte. Dort schützt weiterhin die
Seriennummer, und `stark()` liefert 0.

---

## 3. `Arc[T]` — und was „atomar" hier wirklich heißt

### 3.1 Das Primitiv

`Arc` unterscheidet sich von `Rc` durch genau eines: den atomaren Zähler.
Ohne atomare Instruktion wäre `Arc` nur `Rc` mit anderem Namen. Also gibt es
seit dieser Runde **ein** neues FIR-Primitiv, das kleinste, das reicht:

```firn
__atomar_addieren(p: *mut u64, delta: u64) -> u64    // liefert den ALTEN Wert
```

→ `lock xadd qword ptr [rcx], rax`, eine Instruktion. Erniedrigen ist die
Addition des Zweierkomplements; ein eigenes Primitiv dafür wäre Ballast.
Gebaut in **beiden** Compilern (`compiler/src/atomar.rs`, `Op::AtomicAdd`;
`lib/firnc1/{fir,sema,lower,codegen}.fi`, `O_ATOMADD`), FIR-Text oktettgleich.

### 3.2 Kein „fadensicher" ohne Beleg

Firn hat in Stufe 0 **keine Fäden** (`SPEC` §7). Ein Wettrennen lässt sich
also nicht herbeiführen, und die Behauptung „fadensicher" wäre ungedeckt.
Belegt wird deshalb das, was belegbar ist — `tools/atomar/run.sh`, als
Abschnitt 8b in `test.sh`:

* `__atomar_addieren` erzeugt `lock xadd` — in **drei Baustufen** und in
  **beiden Compilern**, im Assemblertext *und* im fertigen Binary;
* ein gewöhnliches `*p = *p + 7` erzeugt **kein** `lock` (Gegenprobe: ohne sie
  wäre der Nachweis wertlos, weil er alles bestehen ließe);
* der Rückgabewert ist der **alte** Wert, und der Zähler stimmt nach 100 000
  Erhöhungen und 100 000 Erniedrigungen exakt;
* die FIR beider Compiler ist **oktettgleich**.

**Und die Grenze wird genannt:** `arc_klonen`/`arc_freigeben` sind mit
`lock xadd` auch unter Nebenläufigkeit korrekt — freigegeben wird nur von dem
Aufruf, der beim Erniedrigen die 1 *sieht*, und den sieht genau einer.
`aufwerten_atomar` (schwach → stark) braucht dagegen einen **Vergleichs-Tausch**
(`compare_exchange`); mit reinem fetch-add lässt sich das Wettrennen „der
letzte starke Verweis fällt genau jetzt weg" nicht schließen. Runde 47 baut
bewusst nur fetch-add. Im heutigen Firn ist `aufwerten_atomar` korrekt; als
Fadenzusage gilt es **nicht**.

### 3.3 Das Zusammenspiel mit dem Tracing-GC

Die Arc-Halde ist ein eigenes mmap-Gebiet. Der Sammler kennt nur seine eigenen
Chunks. Daraus folgt eine Falle, die vorher nirgends stand: **ein `Gc[T]` im
Wert eines `Arc` ist für den Sammler unsichtbar** und sein Ziel wird
eingesammelt, obwohl es noch benutzt wird.

Deshalb gibt es jetzt **externe Wurzelbereiche**:

```firn
gc_wurzel_anmelden(arc_wert_adresse(a) as *mut u8, groesse)
gc_wurzel_abmelden(arc_wert_adresse(a) as *mut u8)
```

Ein angemeldeter Bereich wird bei jedem Zyklusstart konservativ mitgescannt,
genau wie der Stapel. `tests/833_arc_gc_wurzel.fi` misst **beide** Seiten in
einem Lauf: ohne Anmeldung stirbt das Ziel (der Fehler steht als Messung da,
nicht als Warnung), mit Anmeldung überlebt es 2000 Müllobjekte und mehrere
Sammelläufe, nach dem Abmelden stirbt es wieder.

**Kein doppeltes Freigeben** — und zwar aus einem strukturellen Grund, nicht
aus Sorgfalt: der Zähler entscheidet ausschließlich über den **Arc-Block** (er
geht in die Freiliste der Arc-Halde), der Sammler ausschließlich über
**GC-Objekte** (er kennt nur seine Chunks). Die beiden Speicherbereiche
überschneiden sich nicht.

**Zyklen lecken**, genau wie bei `Rc` — der atomare Zähler ändert daran
nichts. `tests/832_arc_zyklus_leck.fi` zeigt beides in einem Lauf: 1000
Zyklenpaare mit zwei starken Verweisen lecken vollständig (2000 lebende
Blöcke, 0 Freigaben), dieselben 1000 Paare mit einer schwachen Seite werden
restlos frei. Wäre das Leck weg, wäre die Dokumentation falsch — der Test
schlägt dann an.

**Preis, offen benannt:** die Startpause eines Zyklus wächst mit der
angemeldeten Gesamtgröße (konservativer Scan, 8 Byte je Wort).

---

## 4. Messung

### 4.1 Erst musste das Messmittel repariert werden

Der Auftrag verlangt Instruktionszahlen mit callgrind. Das ging nicht: **jedes
Programm mit `gc class` starb unter valgrind mit einem Speicherzugriffsfehler**
— auch mit dem Compiler der Basis, es ist kein neuer Fehler.

Ursache: `__gc_stapel_boden()` las Feld 28 aus `/proc/self/stat`. Das ist die
Startadresse des Stapels, wie der Kern sie beim Programmstart notiert hat —
unter valgrind läuft der Klient aber auf einem von valgrind bereitgestellten
Stapel. Der konservative Scan lief von seinem Stapelzeiger bis zu einer
Adresse, die gar nicht dazugehört.

Jetzt wird zuerst `/proc/self/maps` gelesen und die Abbildung gesucht, in der
der eigene Stapelzeiger wirklich liegt; ihr Ende ist der Boden. Feld 28 bleibt
Rückfall. Damit ist der Sammler zum ersten Mal mit callgrind messbar.

### 4.2 Instruktionen (callgrind, deterministisch)

Gleiche Quelle, gleicher Arbeitsablauf (60 000 Runden à 6 Objekte, Ring
geschlossen, 4000 lebende Objekte), einmal mit dem Compiler der Basis
(plus derselben Stapelboden-Reparatur, damit callgrind überhaupt läuft),
einmal mit Runde 47:

| Arbeitsablauf | Basis | Runde 47 | Δ |
|---|---|---|---|
| Klasse **ohne** `GcWeak`-Feld | 128 067 085 | 134 015 379 | **+4,6 %** |
| Klasse **mit** `GcWeak`-Feld, jede Runde gesetzt | 138 288 807 | 154 997 400 | **+12,1 %** |
| dito **+ Finalisierer** auf jedem 4. Ring | — | 162 856 205 | +5,1 % gegenüber Runde 47 ohne |

Das ist der **teuerste denkbare** Fall: jede Runde schreibt einen schwachen
Verweis, und *alle* 4000 lebenden Objekte haben ein schwaches Feld, das bei
jedem Fegen durchgesehen wird. Auf dem DOM-Arbeitsablauf (`aufbau.fi`, wo nur
`Observer` ein schwaches Feld hat) ist der Durchsatz **unverändert** (§4.4).

### 4.3 Was der Weg dahin gekostet hat — vier gemessene Rücknahmen

Der erste Wurf kostete **+21,1 %** statt +12,1 %. Die Fegeschleife läuft je
Sammellauf über **jeden** Block des Heaps; dort zählt jede Instruktion. Vier
Eingriffe, jeder einzeln mit callgrind gemessen:

| Eingriff | fest.fi |
|---|---|
| erster Wurf (`behalten`-Merker in der Fegeschleife) | 167 427 654 |
| `continue` statt Merker — freie Blöcke zahlen nichts mehr | 160 415 861 |
| Typmaske für schwache Felder (eine Verschiebung statt vier abhängiger Speicherzugriffe), kalter Allokationszweig in eine eigene Funktion, Finalisierer-Einreihung in eine eigene Funktion, Reentranztest aus `__gc_barrier` ausgelagert | **154 997 400** |

Zwei Einzelbefunde, die man nicht rät, sondern misst:

* **Zwei zusätzliche Blöcke in `__gc_alloc_raw` kosteten 3,3 Mio.
  Instruktionen** (2,4 %) — nicht durch die Prüfung selbst, sondern weil die
  Registerzuteilung in der heißesten Funktion des Sammlers kippte. Mit *einem*
  Block (kalter Zweig in einer eigenen Funktion) sind es 0,7 Mio.
* Die Gegenprobe, die Typkennung in der Fegeschleife **zweimal zu lesen statt
  einmal zu binden**, war um 0,3 Mio. Instruktionen **schlechter**. Der Rahmenplatz
  ist hier billiger als der zweite Speicherzugriff — also blieb die Bindung.

### 4.4 Pausen: `aufbau.fi`, 120 000 lebende Knoten, 5 s, je 7 Läufe

Auf dieser Maschine liefen dabei **zwei weitere Runden parallel**. Die Wanduhr
ist damit nicht auswertbar (das war der Fehlbefund der Runde 40); maßgeblich
ist die **Rechenzeit des Fadens**.

| Kennzahl (min/Median/max) | Runde 47 | Basis |
|---|---|---|
| **längste Unterbrechung, Rechenzeit** | 434 / **460** / 522 µs | 453 / **469** / 519 µs |
| längste Unterbrechung, Wanduhr | 473 / 962 / 1854 µs | 476 / 541 / 609 µs |
| Zyklen in 5 s | 545 000 / **554 000** / 562 000 | 546 000 / **554 000** / 554 000 |
| volle Stop-the-World-Läufe | 0 / 0 / 0 | 0 / 0 / 0 |
| RSS | 12 388 / 13 160 / 13 924 KiB | 12 888 / 13 144 / 13 148 KiB |

**Die Pausen sind nicht schlechter geworden** — in Rechenzeit sogar 2 %
besser, und in derselben Größenordnung wie die 0,45 ms aus Runde 44. Der
Durchsatz ist auf diesem Arbeitsablauf unverändert.

### 4.5 Sprengen Finalisierer die Pausen? Nein — A/B im selben Prozess

`tools/gc_mess/final.fi` misst zwei Phasen im **selben** Prozess mit
demselben Code (zwei Binaries hätten anderes Codelayout, und das überdeckt
Unterschiede im Prozentbereich). Beide Uhren an, je 30 s, 4000 lebende Objekte:

| | Phase A **ohne** Finalisierer | Phase B **mit** Finalisierer |
|---|---|---|
| Zyklen | 64 425 600 | 39 156 416 |
| Sammelläufe | 23 581 | 14 325 |
| gelaufene Finalisierer | 0 | **9 789 092** |
| genullte schwache Felder | 64 382 273 | 103 512 460 |
| **längste Unterbrechung, Rechenzeit** | 561 µs | **525 µs** |
| längste Unterbrechung, Wanduhr | 955 µs | 1780 µs |

Histogramm der ganzen Unterbrechung (Fach *k* = [2^(k−1) µs, 2^k µs)):

| Fach | A Wanduhr | A Rechenzeit | B Wanduhr | B Rechenzeit |
|---|---|---|---|---|
| ≤ 32 µs | 160 684 | 160 580 | 99 854 | 99 802 |
| 64 µs | 27 563 | 27 723 | 18 969 | 19 105 |
| 128 µs | 29 050 | 29 037 | 121 917 | 121 942 |
| 256 µs | 10 728 | 10 708 | 12 750 | 12 674 |
| 512 µs | 106 | 87 | 202 | 174 |
| 1,02 ms | 5 | **1** | 4 | **1** |
| 2,05 ms | 0 | **0** | 2 | **0** |
| darüber | 0 | **0** | 0 | **0** |

**In Rechenzeit gibt es in beiden Phasen keine einzige Unterbrechung über
1,02 ms** — 253 698 Unterbrechungen in Phase B, davon eine über 512 µs. Die
beiden Wanduhrwerte über 1 ms in Phase B haben in der Rechenzeit **keine
Entsprechung**: sie sind Verdrängung durch die parallelen Läufe, kein
Sammlerverhalten. Die Vorgabe „im Dauerbetrieb 98 % unter 1 ms" ist mit
**100 %** erfüllt.

Der längste **einzelne** Finalisierer (Zähler hochzählen) wurde mit 882 µs
gemessen — dieselbe Verdrängung; sie gehört dem Programm, nicht dem Sammler,
und wird deshalb getrennt ausgewiesen.

### 4.6 Dauerlauf: 150 s mit 48 Mio. Finalisierern, RSS driftfrei

Derselbe Aufbau, Phase B über **150 s**:

* 193 776 192 Zyklen, **70 888 Sammelläufe**, **48 444 045 Finalisierer**
  gelaufen (alle registrierten), Warteschlange am Ende leer
* 256 693 280 schwache Felder genullt
* **RSS ab der ersten Stichprobe (5 s) bis zur letzten (145 s) konstant
  1372 KiB**, Heap konstant 1 310 720 Bytes, lebende Objekte 4024–4029
* die 4000 Objekte der lebenden Kette waren am Ende **vollständig und in der
  richtigen Reihenfolge** vorhanden — der Sammler hat nichts Lebendes
  eingesammelt

Kein Drift über 2,3 Minuten, obwohl in jeder Sekunde rund 320 000 Objekte
finalisiert und freigegeben wurden.

---

## 5. Tests

| Datei | Was sie prüft |
|---|---|
| `tests/820_gc_finalisierer.fi` | Finalisierer läuft mit richtiger Aufräumart; Gc-Felder sind vorher genullt; **höchstens einmal**; ein erreichbares Objekt wird nicht finalisiert; Massenlauf (300 Objekte) — der Verteiler des Programms lief genau so oft, wie der Sammler zählt |
| `tests/821_gc_finalisierer_grenzen.fi` | jede Ablehnung: Nullzeiger, Stapelzeiger, Zeiger mitten ins Objekt, Art 0, Art > 16777215, doppelte Registrierung; `gc_finalisierer_loeschen` nimmt sie wirklich zurück |
| `tests/822_gc_weak_genullt.fi` | das **Rohwort** des schwachen Feldes ist nach dem Sammeln 0; ein lebendes Ziel wird nicht genullt; nichts zählt doppelt |
| `tests/823_gc_finalisierer_reentranz.fi` | Allokation im Finalisierer bricht mit **71** ab |
| `tests/824_gc_finalisierer_wiederbelebung.fi` | Selbst-Einhängen im Finalisierer bricht mit **73** ab |
| `tests/830_arc_grund.fi` | Zähler, letzter Verweis gibt frei, **kein doppeltes Freigeben**, Block wird wiederverwendet, 20 000 Runden ohne Rest |
| `tests/831_arc_weak.fi` | schwach hält nicht am Leben, Aufwerten nach dem Tod ist sichtbar leer, Freigabe genau einmal in **beiden** Reihenfolgen |
| `tests/832_arc_zyklus_leck.fi` | Zyklen lecken (2000 Blöcke, 0 Freigaben) — und mit einer schwachen Seite nicht |
| `tests/833_arc_gc_wurzel.fi` | GC-Zusammenspiel **ohne** und **mit** `gc_wurzel_anmelden`, Abmelden, beide Buchhaltungen |
| `tests/neg/arc_verworfen.fi` | `arc_neu` ist `#[must_consume]` |
| `tests/neg/atomar_typ.fi` | falscher Zeigertyp beim atomaren Primitiv — Fehler mit Zeile/Spalte, kein stilles Rechnen auf 32 Bit |
| `tests/neg/atomar_stellen.fi` | falsche Stellenzahl — die Meldung nennt die vereinbarte Form |
| `tools/atomar/run.sh` (test.sh 8b) | `lock xadd` in 3 Baustufen und beiden Compilern, Gegenprobe, FIR oktettgleich |

Jeder Positivtest läuft in **drei Baustufen** (release-fast, no-opt, dev-fast)
und zusätzlich unter **firnc1**.

### 5.1 Zwei bestehende Tests mussten von der Rahmenlage unabhängig werden

`__gc_scrub` säubert nur, was **unterhalb** seines eigenen Rahmens liegt. Die
obersten paar hundert Oktette unter dem Stapelzeiger des Programms — dort, wo
später die Rahmen von `gc_collect` und `__gc_scrub` selbst liegen — bleiben
stehen. Ein Helfer, der **flach** aufgerufen wird, legt seine Zeiger genau
dort ab, und der konservative Scan liest sie als Wurzeln.

Bis Runde 46 ging das gut. Als die Laufzeit in dieser Runde größer wurde,
verschoben sich die Rahmen, und dieselbe Lücke hielt in `tests/520` (dev-fast)
**1** und in `tests/535` (ohne Optimierer) **126** längst unerreichbare
Objekte fest. Das war vorher **Glück, kein Nachweis**.

Beide Tests prüfen unverändert dasselbe; neu ist nur, dass die
zeigerhaltenden Rahmen kilobyteweise tiefer liegen (rekursiv — wird also nie
eingebettet — und mit Polster). Dieselbe Technik benutzt das Projekt seit
Runde 4 in `dom_observer_lebt()`.

**Verworfen:** der erste Versuch war, `gc_collect` einen 3 KiB großen,
genullten Puffer im eigenen Rahmen zu geben. Das reparierte `tests/535`, kippte
aber `tests/520`, `820` und `822` — und einer davon mit einem
Speicherzugriffsfehler. Der Grund ist derselbe: die Lücke verschwindet nicht,
sie **wandert** an eine andere Tiefe. Ein Polster im Sammler kann das Problem
nicht lösen, nur verschieben; deshalb wurde es zurückgenommen.

---

## 6. Abnahme

| Prüfung | Basis | Runde 47 |
|---|---|---|
| `bash ./test.sh` | 696/696 | **727/727** |
| `bash tools/selbst_vergleich.sh` | 201 / 0 / 0 | **210 / 0 / 0** |
| `bash tools/fixpunkt.sh` | zeichengleich | **zeichengleich** |

---

## 7. Was offen bleibt

* **`compare_exchange`.** Ohne es ist `aufwerten_atomar` keine Fadenzusage
  (§3.2). Es gehört in die Runde, die Fäden bringt — zusammen mit
  Speicherordnungen (`acquire`/`release`/`relaxed`), die auf x86-64 bei
  `lock xadd` ohnehin gegeben sind, auf aarch64 aber nicht.
* **Finalisierer als Sprachform.** `fn finalize(inout self)` braucht Methoden
  *und* indirekte Aufrufe. Die indirekten Aufrufe baut Runde 46 für Vtables;
  danach ist der Verteiler eine reine Bequemlichkeitsfrage, keine
  Fähigkeitsfrage. Die Semantik dieser Runde bleibt dabei unverändert.
* **Statische Prüfung des Finalisierer-Vertrags.** `SPEC` §3.5.3 sagt „der
  Compiler prüft das"; Runde 47 erzwingt es zur **Laufzeit**. Die statische
  Variante wäre `#[no_gc]` auf `__gc_finalisiere` zu verlangen — die Prüfung
  dafür gibt es schon (`nogc.rs`, transitiv). Bewusst nicht gemacht, weil
  `#[no_gc]` auch das Schreiben *lokaler* Gc-Felder verbietet und damit mehr
  einschränkt als der Vertrag verlangt.
* **`__gc_block_von` ist linear.** Bei schwachen Feldern, deren Ziele über
  viele Chunks streuen, ist die Chunkliste der teuerste Posten des Nullens
  (gemessen: der größte Einzelanteil der +12,1 % in §4.2). Ein nach Adresse
  sortiertes Chunkfeld mit binärer Suche würde das erledigen — es ist ein
  Umbau der Chunkverwaltung und gehört nicht in diese Runde.
* **`GcVec`/`GcMap`, `virtual`, 24-Stunden-Lauf, Fragmentierung bei
  wechselnden Objektgrößen** — unverändert offen (`ABNAHME.md` Punkt 2).

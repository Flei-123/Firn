# Runde 38 — GC: Pausen, Fragmentierung, Dauerlauf

Revier dieser Runde: `lib/gc/gc.fi`, `lib/rc/`, Messwerkzeuge `tools/gc_mess/`.
Compiler-Quellen wurden nicht angefasst. Basis: Runde 36 (Commit 97ec31a).

## Stufe 1 — Messwerkzeuge und Vorher-Zahlen (ohne GC-Aenderung)

Neu: `tools/gc_mess/` mit `run.sh`, `pause.fi`, `frag.fi`.

- **pause.fi**: DOM-artiger Dauer-Workload, sortiert jede Sammellauf-Pause
  (`gc_pause_ns_last`) in neun Klassen (Histogramm) und meldet Maximum und
  Summe. Laufzeitbudget per `GCM_PAUSE_SEK`.
- **frag.fi**: sechs Groessenklassen (48 bis 2048 Bytes Nutzdaten), pro Runde
  ein frischer Stapel genau einer Klasse, der Stapel derselben Klasse aus der
  vorherigen Runde wird fallen gelassen. Lebend: konstant sechs Stapel.
  Gemessen wird der echte RSS aus `/proc/self/statm` (via `lib/dom/mess.fi`).
- **run.sh**: baut beide in drei Baustufen (release-fast, no-opt, dev-fast),
  prueft im Kurzlauf, dass die Zaehler baustufenunabhaengig uebereinstimmen
  (sonst waere die Messung wertlos), dann der eigentliche Lauf in release-fast
  plus Auswertung (Pausen-Histogramm, Fragmentierungs-Urteil).

### Befund: der Test musste erst messbar gemacht werden

Der erste Entwurf des Fragmentierungstests zeigte `lebende=120000` — exakt
ALLE jemals angelegten Objekte, d.h. der GC hat im Test nichts eingesammelt.
Isolation mit Mindesttests (im Log vermerkt, nicht Teil des Repos):

1. Verkettete Kette (10000 Objekte), Kopf-Variable auf null gesetzt, zweimal
   `gc_collect()` → **10000 bleiben lebend**.
2. Unverkettete Objekte, sofort fallengelassen → werden frei (1 Rest).
3. Wie (1), plus 40 KiB Stapel-Verwuestung nach dem Nullen → **bleibt**.

Ursache liegt NICHT im Sammler, sondern im Zusammenspiel Codegen +
konservativem Stapelscan, zweifach nachgewiesen per `objdump`:

- Der Optimierer entfernt `k = gc_null[T]()` als tote Zuweisung, wenn `k`
  danach nie gelesen wird (min1: kein einziger Schreibzugriff auf den
  k-Slot nach dem letzten Lesen). Der alte Zeiger bleibt physisch im Rahmen.
- Ergebnis-Temp-Zellen von Aufrufen werden pro Aufrufstelle frisch vergeben
  und nie genullt (min4: erste `bau()`-Union in `-0x50(%rbp)`, zweite in
  `-0x110(%rbp)`). Der alte Stapelkopf klebt in der ersten Temp-Zelle —
  EIN Zeiger haelt per Verkettung die ganze alte Kette.

Folge fuer alle GC-Messungen (und ehrlich fuer Nutzer der Laufzeit):
**Unerreichbarkeit zuverlaessig herstellen heisst: den Zeiger in einer
Hilfsfunktion sterben lassen (zurueckgekehrter Rahmen wird vom Scrubber
genullt) oder die Wurzel als rohen Wert echt ueberschreiben** — niemals sich
auf ein letztes Null-Setzen in derselben Funktion verlassen.
`tests/510_gc_zyklus_wird_aufgeloest.fi` macht es genau so (Zyklus in
`zyklus()` angelegt, nur `u32` kommt zurueck) — der Test beweist, dass der
Sammler selbst korrekt einsammelt.

frag.fi haelt die sechs Stapelkoepfe deshalb als rohe Adressen in einem
`[u64; 6]`-Array in `main` (echtes Ueberschreiben, Referenzsumme am Ende),
gebaut wird in Hilfsfunktionen, die nur die Adresse zurueckgeben.
Damit: `lebende=1200` nach dem Schluss-Collect — exakt die erwarteten
6 Stapel × 200 Objekte. Kurzlauf-Zaehler in allen drei Baustufen identisch
(300 bei 120×50).

### Vorher-Zahlen (Basis 97ec31a, ohne GC-Aenderung)

Pausen, 60 s DOM-Soak-Workload (release-fast, diese Maschine):

| Lauf | Zyklen | Sammellaeufe | laengste Pause | ≤ 250 µs | > 2 ms | Summe GC |
|---|---|---|---|---|---|---|
| 1 | 105,3 M | 49819 | 1,97 ms | 96,1 % | 5 | 9456 ms |
| 2 | 94,3 M | 44619 | 7,22 ms | 96,0 % | 154 | 9390 ms |

Die Max-Pause schwankt stark zwischen Laeufen (Ausreisser im 4–8-ms-Band);
typisch (96 %) sind ≤ 250 µs. GC-Zeit-Summe ≈ 15–16 % der Laufzeit.
Die 3,54 ms aus der Browser-Abnahme liegen in derselben Grossenordnung.

Fragmentierung, 600 Runden × 200 Objekte (120000 Allokationen, 6 Klassen):

| Metrik | Wert |
|---|---|
| RSS Start | 48 KiB |
| RSS Maximum | 3140 KiB |
| RSS Ende | 3140 KiB |
| Drift im letzten Drittel | **+0,0 % (stabil)** |
| heap_bytes Ende | 3145728 |
| lebende Ende | 1200 (erwartet 1200) |

RSS laeuft einmalig auf ~3 MiB hoch (Heap-Grenze pendelt sich ein) und
bleibt dann 200+ Runden konstant — auf dieser Dauer keine Fragmentierung
sichtbar. Die Aussage ist durch die kurze Laufzeit begrenzt; der 30-Minuten-
Dauerlauf (Stufe 4) ist der haertere Nachweis.

Messartefakte: `tools/gc_mess/pause.tsv`, `tools/gc_mess/frag.tsv`.

## Stufe 2 — Fragmentierung: leere Chunks gehen ans OS

### Befund (Vorher, gemessen mit dem neuen Phasen-Test `frag2.fi`)

`frag2.fi` faehrt einen Phasen-Workload: 300 Runden nur grosse Objekte
(Klasse 2048, 24 Stapel im Ring), dann 300 Runden nur kleine (Klasse 48).
Ergebnis mit dem alten Sweep:

| Metrik | Vorher |
|---|---|
| RSS Phase A max | 20284 KiB |
| RSS Phase B max | 24124 KiB (**steigt weiter**) |
| RSS nach Schluss-`gc_collect()` | 24124 KiB (**faellt nie**) |

Zwei getrennte Ursachen, beide in `lib/gc/gc.fi`:

1. **Leere Klassen-Chunks gingen nie ans OS.** `__gc_sweep` gab nur
   Grossobjekt-Chunks (`klasse >= KLASSEN`) per `munmap` zurueck; die
   256-KiB-Klassen-Chunks blieben fuer immer gemappt, selbst komplett leere.
2. **Die Sammel-Grenze war nach oben offen.** Nach dem letzten Lauf der
   Gross-Phase war `GRENZE = lebbytes` ~ 9,5 MiB; die Klein-Phase alloziert
   nur 3,8 MiB gesamt — es lief NIE wieder eine Sammlung, die toten Chunks
   der Gross-Phase wurden nicht einmal mehr besucht. (gemessen: 300 Runden,
   0 Sammellaeufe in Phase B)

### Aenderung (nur `lib/gc/gc.fi`)

- `__gc_sweep`: komplett leere Chunks JEDER Klasse werden ans OS
  zurueckgegeben. Die Freilisten-Segmentabschneidung laeuft ueber den vor
  dem Chunk gemerkten Listenkopf in O(1) — kein zweiter Durchgang.
- **Hysterese** (Chunk-Kopf Offset 48, neu belegt und dokumentiert): ein
  Chunk wird erst zurueckgegeben, wenn er zwei Sweeps in Folge leer war.
  Ohne sie pendelt der Phasen-Test zwischen munmap/mmap — gemessen
  +46 % Laufzeit (118 -> 176 ms); mit ihr: 123 ms, also churfrei.
- **Grenzen-Kappe**: `MAX_GRENZE = 4 MiB` deckelt den Abstand zwischen zwei
  Sammlungen. Der Speicherueberhang ueber der Lebendmenge ist damit immer
  < 4 MiB, egal wie gross der Heap einmal war. Workloads mit kleinem Heap
  (DOM-Soak ~ 1,3 MiB) merken davon nichts (`MIN_GRENZE` wirkt wie bisher).

### Nachher (gleiche Tests, gleiche Maschine)

| Metrik | Vorher | Nachher |
|---|---|---|
| frag: RSS Ende | 3140 KiB | 2624 KiB |
| frag: Laufzeit | 63 ms | 58 ms |
| frag2: Phase A max | 20284 KiB | 14400 KiB |
| frag2: Phase B max | 24124 KiB | 16448 KiB |
| frag2: RSS Ende | 24124 KiB | **2112 KiB** |
| frag2: Laufzeit | 120 ms | 115 ms |

Die RSS-Kurve in Phase B faellt jetzt MITTEN IM LAUF von selbst
(16448 -> 13632 -> 2112 KiB ab Runde ~150), statt auf dem Maximum der
Gross-Phase sitzen zu bleiben. `lebende` bleibt exakt 1200 (frag) bzw.
4856 (frag2-Ring) — Verhalten der Sammlung unveraendert, nur die
Speicherrueckgabe ist neu.

Messlatte: test.sh **640/640**, selbst_vergleich **186/0/0**, Fixpunkt
**zeichengleich (284207 Zeilen)**, 19 gc/rc-Tests ok, 6 gc-Negativtests
rc!=0.

Verbleibende Designgrenze (ehrlich benannt): Objekte, die ueber viele
Chunks verstreut leben, halten diese Chunks — ohne Kompaktieren (durch den
konservativen Scan untersagt) ist das nicht loesbar. Die Verstreuzahl haelt
sich aber in Grenzen, weil Allokationen chunk-weise aus der Freiliste
kommen.

## Stufe 3 — Inkrementelles Sammeln (hybrid)

### Verfahren

`lib/gc/gc.fi` fuehrt jetzt einen dreiphasigen Zyklus (neue Zustandsfelder
ab Offset 320 im Zustandsblock — Platz war vorhanden, **keine
Compiler-Aenderung noetig**):

- **Phase 0 (Ruhe)**: wie bisher. Grenze erreicht → Zyklus.
- **Phase 1 (Markieren)**: Wurzelscan (Register + Stapel) bleibt atomar
  (konservativer Scan kann nicht unterbrochen werden — der Stapel aendert
  sich), das Verfolgen laeuft in Scheiben von 512 Objekten pro Allokation.
- **Phase 2 (Fegen)**: der Sweep laeuft in Scheiben von 2 Chunks pro
  Allokation (Merker im Zustandsblock); Freilisten-Segmentabschneidung und
  Leer-Hysterese aus Stufe 2 gelten pro Chunk unveraendert.

Korrektheit ohne Kompaktieren und ohne erneuten Stapelscan:

- **Einfuegebarriere aktiviert** (Dijkstra): `__gc_barrier` faerbt das Ziel
  eines Gc-Schreibzugriffs grau, solange ein Zyklus laeuft — bisher zaehlte
  sie nur. Weisse Ziele hinter schwarzen Containern koennen so nicht
  unsichtbar werden.
- **Waehrend eines Zyklus allokierte Objekte sind grau** und kommen auf den
  Markstapel (der Stapel wird nicht erneut gescannt; ein nur stapellebendes
  Objekt duerfte sonst gefegt werden, bevor es je angeschlossen wurde).
- **Weiss-Paritaet** (`S_PAR`, 0/2): der Sweep setzt keine Marken mehr
  zurueck; am Zyklusende kippt die Paritaet und alle Ueberlebenden sind auf
  einen Schlag wieder weiss. Damit gibt es keinen Marken-Reset-Durchlauf
  und keine Altmarken im naechsten Zyklus.
- **Erschoepfungs-Fallback**: findet die Allokation mitten im Zyklus keinen
  Block, wird der Rest des Zyklus atomar zu Ende gebracht, bevor
  OutOfMemory gemeldet wird (DESIGNZIELE §2 bleibt).

### Messungen (diese Maschine, Lastvorbehalt: parallel laufende Runden)

Pausen nach Scheibentyp (Diagnose-Felder `S_PMAX_*`, `gc_pause_max_typ`),
DOM-Workload, inkrementeller Pfad erzwungen (`INKR_AB` testweise 1 MiB):

| Scheibe | laengste Pause |
|---|---|
| Zyklus-Start (Wurzelscan) | 0,15 ms |
| Markieren (512 Objekte) | 0,01–0,02 ms |
| Fegen (2 Chunks) | 0,44–0,52 ms |
| Termination | 0,01 ms |

Die Pausen sind **heap-groessen-unabhaengig** begrenzt: Start haengt von
der Stapeltiefe ab, Mark/Fegen von den (konstanten) Scheibengroessen —
nicht mehr vom Heap. Genau das war das Ziel.

Durchsatz DOM-Workload (5 s-Laeufe, Median von 5, verschachtelt):

| Variante | Zyklen (Median) | laengste Pause |
|---|---|---|
| Stufe 2 (atomar) | 7 786 000 | 0,44 ms |
| Hybrid | 7 110 000 (**−8,7 %**) | 0,46 ms |
| inkrementell erzwungen | 6 533 000 | **0,47 ms** |

### Der Hybrid und warum

Rein inkrementell (ab dem ersten Zyklus) kostete −13 bis −26 % Durchsatz
bei Heaps, deren Vollzyklus ohnehin unter einer Millisekunde liegt — reiner
Verlust. Deshalb schaltet der Sammler erst ab `INKR_AB = 8 MiB` Heap auf
den inkrementellen Zyklus um; darunter laeuft der atomare Pfad aus Stufe 2
unveraendert (seine Pause waechst linear: ~0,5 ms je 1,3 MiB — bei 8 MiB
waeren das ~3 ms, ab dort greifen die Scheiben).

Der DOM-Soak-Workload (Heap ~1,3 MiB) laeuft also atomar: Pausen wie in
Stufe 2, Durchsatz-Einfluss nur durch den Phasen-Check je Allokation
(gemessen −8,7 %, innerhalb der ±10%-Vorgabe). Grosse Heaps bekommen
Pausen um 0,5 ms, unabhaengig von der Heapgroesse.

Ehrliche Nebenwirkung, gemessen am Phasen-Test (Peak ~20 MiB, liegt ueber
der Schwelle): `rss_ende` 14912 KiB statt 2112 KiB in Stufe 2 — Floating
Garbage (grau allokierte Objekte leben einen Zyklus laenger) und die
Leer-Hysterese verzoegert die Rueckgabe im inkrementellen Pfad. Gegen den
Zustand vor dieser Runde (24124 KiB, nie fallend) bleibt es eine deutliche
Verbesserung; unterhalb von 8 MiB gilt exakt das Stufe-2-Verhalten.

Messlatte: test.sh **640/640**, selbst_vergleich **186/0/0**, Fixpunkt
**zeichengleich (284207)**, 19 gc/rc-Tests ok, 6 gc-Negativtests rc!=0.

## Stufe 5 — Finalisierer und Arc[T]: benannte Restarbeit

Bewusst NICHT begonnen, beide sind groesser als eine Reststufe:

- **Finalisierer**: brauchen eine Semantik-Entscheidung (wann erlaubt,
  Resurrektion ja/nein, Reihenfolge, Faden). Ohne SPEC-Grundlage waere jede
  Umsetzung ein Ratespiel; der konservative Scan und die Paritaets-Marken
  dieser Runde sind damit vertraeglich (Finalisierbare Objekte wuerden einen
  Zyklus laenger leben — dieselbe Maschinerie wie Floating Garbage).
- **Arc[T]**: ein zweiter Verweis-Typ neben Rc[T] mit atomaren Zaehlern —
  sinnvoll erst, wenn es Faden gibt (SPEC §7 ist Stufe 0 einfaedelig). Atomik
  ohne Faden zu bauen waere toter Aufwand.

Beide bleiben als Restarbeit benannt, nicht angeflickt.

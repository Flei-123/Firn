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

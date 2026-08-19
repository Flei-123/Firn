# Runde 40 — Regalloc-Angriff auf realweb, GC-Dauerlauf ehrlich gemacht

Basis: Merge-Commit `415e8b4` (Runden 37+38+39). Revier: `compiler/src/regalloc.rs`
und `tools/gc_mess/`. Zwei getrennte Stränge, beide in dieser Datei.

## Strang A — Registerzuteilung

Ausgangslage nach Runde 37: html5lib 1,69×, realweb 4,34× gegenüber html5ever.
Dokumentierter Hebel: 7 391 reg→reg-`mov`s und 445 Store/Reload-Paare im
heißen Pfad von `dekodiere`/`tokenize`.

### A1 — Register-Deskriptor streicht Store→Reload (Commit `07dab2d`)

Peephole auf Wert-Slots im Emissionspfad: ein Reload direkt nach dem Store
desselben Slots liest stattdessen das Register weiter. Wirkung:
realweb 4,38× → 4,23×, html5lib 1,73× → 1,70×.

### A2 — Zellen-Alias für Loads (Commit `f6895ea`)

`d = load c`, wobei die Zelle `c` in einem Register liegt und alle
Verwendungen von `d` im selben Block vor dem nächsten Schreiben der Zelle
stehen: `d` bekommt keinen eigenen Ort, die Verwendungen lesen direkt das
Zellenregister (`Alloc::ort()`), der Load entfällt. Das streicht die drei
`mov`-Kopien je Schleifendurchlauf im heißesten Loop von `dekodiere`
(33,5 Mio. Iterationen im realweb-Lauf).

**Falle, die drei Tests umgeworfen hat** (211_generic_struct, 430_ct_select,
416_fehler_ausgabe): Bei 8/16/32-Bit-Loads holt der Load die relevanten Bits
per `movzx`/32-Bit-`mov` heraus; das Zellenregister enthält oben noch Reste.
Der Alias ist deshalb **nur für volle 64 Bit** zulässig.

**Messung — deterministisch statt Wanduhr.** Die Wanduhr-Messung
(`durchsatz.sh`) schwankt um ±30 % und zeigte den Gewinn nicht (mit Alias
10,47 MB/s, ohne 10,72 MB/s — reines Rauschen). Belastbar ist die
Instruktionszählung mit callgrind auf demselben realweb-Korpus:

| Stand | I refs (realweb) |
|---|---|
| ohne Alias (`FIRN_NO_ALIAS=1`) | 2 334 236 911 |
| mit Alias | 2 136 489 667 |
| | **−8,47 %** |

Lehre für kommende Runden: Optimierungen **immer** mit callgrind belegen,
`durchsatz.sh` nur zur Kontrolle der Größenordnung.

Verifikation A2: `test.sh` 649/649, `selbst_vergleich` 188/0/0,
Fixpunkt 289 096 Zeilen zeichengleich.

## Strang B — der GC-Dauerlauf maß den falschen Pfad

Der 30-Minuten-Dauerlauf aus Runde 38 (`tools/gc_mess/pause.fi`) lief sauber
durch, aber die ehrliche Auswertung zeigt:

- 30,0 min, 1 394 549 Sammelläufe, 2 948 276 000 Zyklen, 0 übersehene
  Mehrfach-Sammlungen
- RSS: Start 0,82 MiB, Maximum 1,33 MiB, Ende 0,83 MiB — **kein Drift**
  (erstes Zehntel 1,16 MiB, letztes Zehntel 1,16 MiB): kein Leck, keine
  Fragmentierung über die Zeit
- mittlere Pause 127 µs, längste Pause 3,853 ms
- Zeit in Pausen: 177,3 s von 1800 s = **9,85 %**

**Aber:** `heap_bytes` blieb die ganze Zeit bei 786 432 Bytes. Der
inkrementelle Zyklus schaltet erst ab `INKR_AB = 8 MiB` (`lib/gc/gc.fi:109`)
ein — er wurde in diesem Lauf also **kein einziges Mal benutzt**. Der Lauf
belegt die Stabilität des nicht-inkrementellen Pfades, über die in Runde 38
beworbenen „~0,5 ms, heap-unabhängig" sagt er nichts.

### B1 — neuer Lauf mit großer lebender Menge (`tools/gc_mess/pause_gross.fi`)

Hält absichtlich viel am Leben: eine Wurzel mit `KINDER` (Standard 120 000)
Textknoten als Geschwisterkette, gehalten an einer Rahmenzelle von `main`,
am Ende per `dom_kinder_zaehlen` nachweislich noch erreichbar. Parallel läuft
derselbe Müllstrom wie in `pause.fi`. Damit steht der Heap dauerhaft bei
13 MiB, jeder Sammellauf muss 120 000 lebende Objekte markieren — der Fall,
für den das inkrementelle Sammeln gebaut wurde. Eingehängt in `run.sh` als
Stufe 2b (`GCM_GROSS_SEK`, `GCM_KINDER`), mit eigener Auswertung.

Messung (6 s Budget, 120 000 lebende Knoten, Heap 13,0 MiB, 82 Sammelläufe):

| Klasse | Anzahl | kumuliert |
|---|---|---|
| ≤ 500 µs | 29 | 36,7 % |
| ≤ 1 ms | 50 | 100,0 % |

Also **alle Pausen der Messschleife unter 1 ms**, trotz 13 MiB Heap und
120 000 lebenden Objekten — das Ziel < 2 ms hält der inkrementelle Pfad
tatsächlich, und zwar heap-unabhängig. Scheiben-Maxima: Typ 0 159 µs,
Typ 1 719 µs, Typ 2 183 µs, Typ 3 6 µs.

**Offener Punkt für Runde 41:** `pause_max_ns` meldet 12,0 ms. Dieser Wert
stammt aus der **Aufbauphase**, in der der Heap erst wächst und noch
nicht-inkrementell gesammelt wird. Ein 12-ms-Aussetzer beim Heap-Wachstum
ist für ein 16-ms-Bildbudget zu viel: der Übergang in den inkrementellen
Modus muss früher greifen oder das Wachstum selbst inkrementell laufen.

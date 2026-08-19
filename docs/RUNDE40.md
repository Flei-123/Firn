# Runde 40 — Regalloc-Angriff auf realweb, GC-Dauerlauf ehrlich gemacht

Basis: Merge-Commit `415e8b4` (Runden 37+38+39). Revier: `compiler/src/regalloc.rs`
und `tools/gc_meas/`. Zwei getrennte Stränge, beide in dieser Datei.

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

Der 30-Minuten-Dauerlauf aus Runde 38 (`tools/gc_meas/pause.fi`) lief sauber
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

### B1 — neuer Lauf mit großer lebender Menge (`tools/gc_meas/pause_big.fi`)

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

## Strang C — der eigentliche Durchbruch lag nicht im Compiler

Nach A2 zeigte das callgrind-Profil des realweb-Laufs (2,14 Mrd. Ir):

| Anteil | Ir | Funktion |
|---|---|---|
| 26,1 % | 557 993 466 | `dekodiere` (UTF-8 → Codepunkte) |
| 23,9 % | 510 486 268 | `eingabe_pruefen` |
| 23,1 % | 492 598 522 | `tokenize` (Zustandsautomat) |
| 15,6 % | 332 932 201 | `main` (Sammeltopf: alles ohne eigenes Symbol) |

Damit war klar: der teuerste Teil ist nicht der Automat, sondern die
Vorverarbeitung je Zeichen. Drei Experimente, jeweils mit callgrind gemessen
und mit oktettgleicher Ausgabe gegengeprüft:

### C1 — `eingabe_pruefen`: ein Bereichstest statt zwölf Vergleichen

`0x20..0x7E` ist auf echten Seiten der Normalfall und **nie** ein Fehler des
Eingabestroms. Ein vorangestellter Bereichstest beendet die Funktion sofort.

**2 136 489 667 → 1 793 235 323 Ir (−16,1 %)**

### C2 — `dekodiere`: vorab reservieren, ASCII direkt schreiben

Ein Codepunkt kostet mindestens ein Byte, also reichen `len` Plätze immer:
`cp_reserve(out, len)` einmal, danach direkt in den Rohspeicher schreiben
statt `cp_push` je Zeichen (Aufruf + Kapazitätstest). Dazu ein
ASCII-Schnellweg (`c0 < 0x80 && c0 != CR`), der die vier Breitenvergleiche
überspringt. Neu in `lib/html/mem.fi`: `cp_ptr`, `cp_set_len` (exportiert).

**1 793 235 323 → 1 427 485 223 Ir (−20,4 %)**

### C3 — Schnellweg an der Aufrufstelle

Auch der reine Funktionsaufruf von `eingabe_pruefen` kostet; derselbe
Bereichstest direkt in `tokenize` spart ihn für ~95 % aller Zeichen.

**1 427 485 223 → 1 297 240 896 Ir (−9,1 %)**

### C4 — widerlegt: „Textlauf am Stück" im Data-State

Hypothese: eine innere Schleife, die unbedenkliche Textzeichen ohne
Zustandsverzweigung am Stück in den Zeichenpuffer schiebt (das, was
html5ever aus `memchr` zieht), spart den `match`-Dispatch je Zeichen.

Gemessen: **1 297 240 896 → 1 297 140 701 Ir (−0,008 %)** — also nichts. Der
`match` über den Zustand ist bereits eine echte Sprungtabelle
(`jmp *(%rdx,%rax,8)` im Disassemblat), der Dispatch kostet praktisch nichts.
Die Änderung wurde **verworfen**: mehr Code ohne Gegenwert.

### Ergebnis Strang C

| Korpus | vor Runde 40 | nach Runde 40 | Ziel |
|---|---|---|---|
| html5lib (pathologisch) | 1,69× | **1,33×** | ≤ 2× ✅ |
| realweb (echte Seiten) | 4,34× | **2,68×** | ≤ 3× ✅ (Stretch), ≤ 2× offen |

Instruktionen realweb insgesamt: 2 334 236 911 → 1 297 240 896 = **−44,4 %**.
html5lib-Konformität unverändert 6810/6810 (Fehlermeldungen 6809/6810),
`test.sh` 649/649, `selbst_vergleich` 188/0/0, Fixpunkt 289 096 zeichengleich.

### Lehre

Der Wanduhr-Vergleich hat den ersten Gewinn (A2) **nicht** gezeigt und wäre
fast als „bringt nichts" verworfen worden; callgrind zeigte −8,47 %. Und der
größte Hebel lag in zwei Bibliotheksfunktionen, nicht im Optimierer. Reihen-
folge für Runde 41: erst profilieren, dann optimieren — und jede Optimierung
mit Instruktionszählung belegen, nie mit der Uhr.

### Offen für Runde 41

- `tokenize` ist jetzt mit 492 Mio. Ir (38 %) der größte Posten: ~100
  Instruktionen je Zeichen im Automaten, überwiegend Slot-Verkehr in einer
  Funktion mit 8 200 Assemblerzeilen → Intervall-Splitting im Regalloc.
- 52 Vergleiche in `tokenize` laden ihre Konstante aus einem Rahmen-Slot
  (`cmp -0x270(%rbp),%r9d`): `immediate_consts` verwirft eine Konstante
  global, sobald **eine** ihrer Verwendungen kein Immediate zulässt. Fix:
  Konstante an der problematischen Stelle klonen statt überall aufgeben.
- `tok_attr_value_push` 105 Mio. Ir (8 %) — noch ungeprüft.

## Strang B2 — der 30-Minuten-Dauerlauf MIT großer lebender Menge

Nachgeholt mit `pause_big.fi` (120 000 lebende Knoten, Heap 13 MiB,
1800 s). Rohdaten: `tools/gc_meas/dauer30_gross.tsv`.

- 23 840 Sammelläufe, 202 453 000 Zyklen, 3 übersehene Mehrfach-Sammlungen
- RSS über die ganze Zeit 12,9–13,9 MiB, Ende 12,89 MiB — **kein Drift**,
  auch nicht mit permanent großer lebender Menge
- Pausen-Histogramm:

| Klasse | Anzahl | Anteil | kumuliert |
|---|---|---|---|
| ≤ 500 µs | 11 472 | 48,1 % | 48,1 % |
| ≤ 1 ms | 11 663 | 48,9 % | 97,1 % |
| ≤ 2 ms | 295 | 1,2 % | 98,3 % |
| ≤ 4 ms | 372 | 1,6 % | 99,9 % |
| ≤ 8 ms | 25 | 0,10 % | 99,99 % |
| ≤ 16 ms | 9 | 0,04 % | 99,996 % |
| > 16 ms | 1 | 0,004 % | 100 % |

- längste Pause **19,34 ms**, und zwar in einer Scheibe vom Typ 1 (nicht nur
  in der Aufbauphase, wie der Kurzlauf noch nahelegte). Die Maxima wuchsen im
  Verlauf: 12,05 ms → 15,68 ms → 19,34 ms.
- Summe aller Pausen 1689,9 s von 1800 s Laufzeit — das sind **93,9 %**.
  Bei 120 000 dauerhaft lebenden Objekten und laufender Müllproduktion
  arbeitet der Sammler also fast durchgehend.

### Urteil

Der inkrementelle Pfad hält den **Normalfall** klar unter 1 ms (97,1 %), aber
er hält keine **Schranke**: 0,15 % der Pausen liegen über 4 ms, einzelne bei
19 ms. Für ein 16-ms-Bildbudget ist das ein sichtbarer Ruckler alle paar
Minuten. Zusammen mit dem Pausenanteil von 93,9 % ist das der klarste offene
Punkt des GC — vor Finalisierern und `Arc[T]`.

**Aufgabe für Runde 41:** herausfinden, warum eine Typ-1-Scheibe 19 ms lang
werden kann (unbegrenzte Arbeitsmenge je Scheibe? Nachmarkierung am
Zyklusende?), und die Scheibengröße an ein echtes Zeitbudget koppeln statt an
eine Objektzahl.

## Nachtrag (Runde 41, Vorarbeit) — die 19 ms waren nicht der Sammler

Die Diagnose des GC war unvollständig: der **volle Stop-the-World-Lauf**
(`__gc_collect_now`) buchte seine Dauer nur in `S_PAUSE_MAX`, aber in **keine**
Scheibenklasse. Dadurch war das globale Maximum (11,8 ms) größer als jedes
Typmaximum (3,6 ms) und niemand konnte sehen, woher es kam. Behoben: der
volle Lauf ist jetzt **Typ 4**, dazu ein Zähler `gc_volle_laeufe()`.

Damit gemessen (`pause_big.fi`, 120 000 lebende Knoten, 13 MiB Heap):

| Lauf | Sammelläufe | davon volle | längste Scheibe (Typ 0–3) | Typ 4 |
|---|---|---|---|---|
| 30 s | 434 | 3 | 1,89 ms | 11,23 ms |
| 120 s | ~1600 | **3** | 3,39 ms | 11,93 ms |
| 600 s (ruhig) | 7 967 | **3** | 3,12 ms | 11,67 ms |

**Es bleiben immer genau drei volle Läufe** — alle in der Aufbauphase, solange
der Heap noch unter `INKR_AB` (8 MiB) liegt. Im Dauerbetrieb läuft
**ausschließlich** der inkrementelle Pfad. Der Markstapel läuft nie über
(`ueberlaeufe = 0`), das teure Nachtragen kommt also gar nicht vor.

Pausen im ruhigen 10-Minuten-Lauf (längste Scheibe je Zyklus):

| Klasse | Anzahl | kumuliert |
|---|---|---|
| ≤ 500 µs | 3 356 | 42,1 % |
| ≤ 1 ms | 4 484 | **98,4 %** |
| ≤ 2 ms | 98 | 99,7 % |
| ≤ 4 ms | 26 | 100 % |
| > 4 ms | 0 | — |

**Korrektur zum Abschnitt B2:** die dort berichteten 19,34 ms und die 0,15 %
über 4 ms stammen aus einem Lauf, der **gleichzeitig mit `test.sh` und
callgrind** auf derselben Maschine lief. Ohne Fremdlast liegt nichts über
4 ms. Lehre: Pausenmessungen nur auf einer ruhigen Maschine, und Fremdlast
im Protokoll vermerken.

**Nicht belegt:** kleinere Markierscheiben (`SCHEIBE_TRACE` 512 → 128) zeigten
keinen sauberen Gewinn (Typ-1-Maximum 1,07 ms → 1,87 ms — im Rauschen der
Einzelmaxima). Zurückgesetzt auf 512. Vorher fehlt das Werkzeug: ein
Histogramm der **einzelnen Scheiben** statt nur der längsten Scheibe je
Zyklus. Das ist die erste Aufgabe der Runde 41.

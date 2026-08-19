# Runde 41 — Scheiben-Histogramm, Zeitbudget im Sammler, und ein Miscompile im Optimierer

Zwei geplante Punkte (Diagnose und Zeitbudget des GC) und ein ungeplanter,
der alles andere überwog: der Optimierer von `firnc0` erzeugte seit Runde 40
falschen Code, und der Prüfapparat hat es nicht gemeldet.

## 1. Histogramm der EINZELNEN Scheiben (Aufgabe 1 aus Runde 40)

Bisher buchte `__gc_pause_buche` nur Maxima: längste Pause je Zyklus, längste
je Typ. Für die Frage „ist die Scheibe zu groß?" ist das unbrauchbar — ein
Maximum rauscht, eine Verteilung nicht. Neu in `lib/gc/gc.fi`:

* `S_HIST`: 7 × 16 Fächer im Zustandsblock (448…1344, Blockgröße 4096).
  Fach 0 = unter 1 µs, Fach k = [2^(k−1) µs, 2^k µs), Fach 15 = ab 16,384 ms.
* Typen 0–4 wie `gc_pause_max_typ` (Start/Mark/Sweep/Ende/Voll), **Typ 5 =
  Nachtragen nach Markstapel-Überlauf** (im Maximum steckt es unter Typ 1),
  Typ 6 = alle Scheiben zusammen.
* Abfrage `gc_hist(typ, fach)`, Nullen mit `gc_hist_reset()`.
  `tools/gc_meas/pause_big.fi` nullt nach dem Aufbau und gibt alle
  besetzten Fächer aus.

Erster Befund gleich mit dem neuen Werkzeug: Typ 5 blieb über alle Läufe
**leer** — der Markstapel läuft nie über, das teure Nachtragen kommt im
Dauerbetrieb nicht vor. Das war in Runde 40 nur vermutet.

## 2. Zeitbudget statt Objektzahl (der offene Punkt aus Runde 40)

Die Markierscheibe verfolgte bis dahin **512 Objekte**, egal wie teuer die
sind. Ein Knoten mit vielen Zeigerfeldern kostet ein Vielfaches eines
Textknotens — im Histogramm streuten die Scheiben deshalb über drei
Zehnerpotenzen. Neu: `ZEIT_BUDGET_NS = 100 000` (100 µs), die Uhr wird alle
`ZEIT_PROBE = 16` Objekte gelesen (`__gc_jetzt_ns` kostet selbst ~25 ns).

Messung `pause_big.fi`, 120 000 lebende Knoten, Heap 13 MiB, je 20 s,
Markierscheiben (Typ 1):

| Fach (Scheibendauer) | vorher (nur Objektzahl) | Probe 64 | **Probe 16 (eingebaut)** |
|---|---|---|---|
| 64–128 µs   | 0      | 122 330 | **172 824** |
| 128–256 µs  | 10 306 | 32 268  | 137 |
| 256–512 µs  | 50 101 | 140     | 140 |
| ≥ 512 µs    | 458    | 3       | 2 |

Durchsatz (Zyklen im selben Zeitfenster) unverändert im Rauschen
(2 198 000 / 2 347 000 / 2 231 000), Pausensumme unverändert. Das Budget
kostet also nichts und schneidet den Schwanz ab.

60-Sekunden-Kontrolllauf auf dem Endstand (`tools/gc_meas/scheiben60.tsv`):
518 137 Markierscheiben, davon **99,88 % in 64–128 µs**, 284 darüber, drei
Einzelfälle über 1 ms (Ausplanen durch das Betriebssystem, nicht der
Sammler). `pause_max_typ4 = 11,7 ms` sind weiterhin die **drei vollen
Läufe der Aufbauphase** unter `INKR_AB` — bekannt und benannt.

## 3. Der eigentliche Fund: `.astdump` hing bei jedem `||`

Beim Verifizieren blieb `test.sh` in Abschnitt 12 (Parservergleich) stehen —
nicht langsam, **hängend**: `./.astdump bin/lexdump.fi` lief 45 Minuten mit
voller CPU und ignorierte SIGTERM. Eingegrenzt mit gdb auf den hängenden
Prozess:

```
#0 rt__buf_wachse (bin/rt.fi:187)   rsi = 0xffff8002d74f884b   <- Länge < 0
#1 rt__buf_push_bytes
#2 druck__schreib
#3 druck__drucke_binop
```

`buf_wachse` verdoppelt `kap`, bis `kap >= noetig`; bei einer unterlaufenen
Länge läuft `kap` über 0 und die Schleife dreht sich ewig. Minimalfall:
**jede Datei, die `||` enthält.** `&&` war unauffällig.

Ursache ist kein Parserfehler, sondern **falscher Maschinencode aus
`firnc0`** — im Assembler von `drucke_binop`:

```
lea r12, [rbp+r12-1491]   ; &tab[start]  ueberschreibt das Zellenregister von start
mov r13, 43
sub r13, r12              ; 43 - ADRESSE statt 43 - start
```

Schuldig ist der **Zellen-Alias aus Runde 40** (`compiler/src/regalloc.rs`):
ein `load` darf das Zellenregister direkt lesen, statt zu laden. Die
Verteilung war da aber längst gelaufen — sie kannte die vom Alias
**verlängerte Lebensspanne** nicht und durfte dasselbe Register an einen
anderen Wert vergeben. Zwei Löcher, beide jetzt geschlossen:

1. Zwischen Load und Verwendung darf **kein anderer Wert** in das
   Zellenregister schreiben (`belegt_register`).
2. Liegt das Zellenregister **nicht** in `CALLEE_SAVED`, beendet ein
   dazwischenliegender `call`/`syscall` den Alias — ein Aufruf zerstört
   caller-saved Register. (Fehlerbild: `bin/layoutdump.fi` stürzte in
   `intern_finde` mit `t = 0` ab.)

**Kosten der Korrektur: keine.** callgrind auf dem realweb-Korpus,
`tokenize_bench`:

| Compiler | I refs realweb |
|---|---|
| vor der Korrektur (fehlerhaft) | 1 297 226 146 |
| nach der Korrektur | 1 297 226 150 (+4) |

## 4. Warum 649/649 das nicht gemerkt haben

Zwei Gründe, beide ärgerlich:

* Die Dump-Binaries (`.astdump`, `.layoutdump`, …) wurden über lange Zeit
  **veraltet wiederverwendet**. Runde 40 hat das Neubauen zwar eingebaut —
  die Fassung, die den Fehler enthielt, wurde aber erst danach gebaut. Der
  erste ehrliche Lauf mit frischen Binaries war dieser hier, und er hing
  sofort.
* Zwei gleichzeitige Läufe (Hauptrepo + Worktree) benutzten **dieselben
  `/tmp`-Dateien** (`/tmp/parv_a.txt`, `/tmp/lexv_a.txt`, …) und
  überschrieben sich gegenseitig die Vergleichsausgaben. Das erzeugte
  „148 UNGLEICH", die beim Einzelnachlauf verschwanden. Alle sechs
  Vergleichswerkzeuge legen jetzt ein eigenes `mktemp -d` an.

Der Regressionstest `tests/opt/cells_alias_clobber.fi` beschreibt das
Muster; **ehrlich vermerkt**: er löst den Fehler in dieser Größe nicht selbst
aus (ob das Register überschrieben wird, hängt vom Registerdruck des ganzen
Moduls ab). Der zuverlässige Wächter bleibt Abschnitt 12 von `test.sh`.

## 5. Verifikation dieser Runde

* `bash ./test.sh` → **PASS 652/652**
* `bash tools/self_compare.sh` → **189 gleiches Verhalten**, 0 Abweichungen
* `bash tools/fixpunkt.sh` → Stufe 2 == Stufe 3, zeichengleich,
  **309 468 Zeilen** Assembler (gewachsen, weil `lib/gc/gc.fi` um Histogramm
  und Zeitbudget größer wurde)
* callgrind realweb unverändert (siehe oben)

## 6. Offen

* **realweb ≤ 2×** (aktuell 2,68×) — nächster dokumentierter Hebel:
  Intervall-Splitting im Regalloc, `tokenize` ist mit 38 % der größte Posten,
  `tok_attr_value_push` 8 %.
* **Aufbauphase des GC**: drei volle Läufe mit bis zu 11,7 ms, solange der
  Heap unter `INKR_AB` (8 MiB) liegt. Für ein 16-ms-Bild ist das zu viel;
  Abhilfe wäre, den inkrementellen Zyklus schon beim Wachsen zu benutzen.
* Finalisierer und `Arc[T]` weiterhin offen (Runde 38 benannt).

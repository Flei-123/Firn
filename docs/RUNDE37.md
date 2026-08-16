# Runde 37 — Optimierer-Angriff auf das ≤2×-Ziel

Ausgangslage (Commit 97ec31a, Runde 36): Tokenizer-Workload gegen html5ever
(`tools/tokenizer/durchsatz.sh`, bester von 3 bzw. 7 Laeufen):

| Korpus | Firn MB/s | html5ever MB/s | Faktor |
|---|---|---|---|
| html5lib (pathologisch) | 5,94 | 11,49 | **1,94×** |
| realweb (echte Seiten) | 8,96 | 43,19 | **4,82×** |

## Profil (callgrind, Korpus realweb, 2,63 G Instruktionen)

| Anteil | Funktion | Befund |
|---|---|---|
| 30,9 % | `dekodiere` | UTF-8→UTF-32-Vordurchgang (html5ever braucht ihn nicht) |
| 24,2 % | `eingabe_pruefen` | ~108 Ir pro Zeichen: Call+Prolog-Overhead, 49 FIR-Insts > Inline-Grenze 40 |
| 22,3 % | `tokenize` | Zustandsmaschine, nach Opt vollstaendig call-frei |
| 12,7 % | `main` | aeussere Schleife, Sink-Verwaltung |

Beobachtung am Maschinencode (`dekodiere`, 483 x86-Instr.): **47 % `mov`**
(87 reg→reg, 79 reg→mem, 59 mem→reg) und 57 Spruenge. 49 FIR-Instruktionen
werden zu ~135 x86-Instruktionen — Faktor ~2,7 Ueberhang durch
Stack-Verkehr und Kopien. `tokenize` und `dekodiere` sind nach der
Optimierungspipeline komplett aufruf-frei — der Emissionspfad selbst war
also der Hebel, nicht (nur) die Zuteilung.

## Messreihe

| # | Optimierung | html5lib | realweb | Status |
|---|---|---|---|---|
| 0 | Baseline (97ec31a) | 1,94× | 4,82× | — |
| A | Inline-Grenzen 40/8 → 60/10 | 2,04× | 4,87× | **VERWORFEN**: kein Gewinn, bricht `520_gc_weak [opt]` (Exit 6) + selbst_vergleich (firnc1: 127 bei bubblesort) — latenter inline.rs-Fehler bei groesseren Bloecken. Ruecknahme. |
| B | Sprung-Fallthrough (Br/BrCond/cmp+jcc, Ziel==naechster Block) + `cmp` direkt ins Zielregister | 1,76× | 4,64× | ✅ 640/640, Commit 977a2ad |
| C | Registerpools rsi/rdi (ohne call/memop) + rdx (ohne call/div/select) im linear scan | 1,71× | 4,42× | ✅ 640/640, Commit ef4e530 |
| D | Inline-Grenzen 60/32 + DAG-Ausnahme | 1,50× | 3,69×…8,20× | **VERWORFEN** (siehe unten) |
| E | Shift mit konstantem Abstand als Sofortform (`shl $k`), direkt im Zielregister | 1,71× | 4,36× | ✅ (im Rauschen; Ir −0,003 %) |

## Der Inline-Lehrstueckpfad (D)

Mehrere Grenzvarianten wurden durchgemessen; die Erkenntnisse sind wichtiger
als die verworfene Aenderung:

* **`__gc_scrub_tief` ist rekursiv und scrubbt den Stapel ueber die
  RekursionsTIEFE.** Inlining entrollt eine Stufe und verlagert den
  4-KiB-Puffer in den Aufrufer-Rahmen — das Scrubben verliert seine Wirkung,
  `520_gc_weak` fiel mit Exit 6 aus, selbst_vergleich mit firnc1 rc=127.
  → **Behalten: selbst-erreichbare (rekursive) Ruempfe werden nie
  eingebettet** (`erreicht_sich_selbst` in inline.rs, vorberechnet).
* **`520_gc_weak` ist rahmenlayout-fragil** (konservativer Stapel-Scan ohne
  Scrub zwischen `anlegen` und `gc_collect`): schon das Einbetten von
  `__gc_strong_raw` (Wertrumpf → Ergebnis-Alloca im Caller) kippte den Test.
  → Empfehlung an die GC-Runde: Stapel-Scrub im Testpfad robust machen.
* **`eingabe_pruefen` (49 Insts/29 Bloecke) einzubetten sprengt `tokenize`
  ueber die Regalloc-Sicherheitsgrenze** (nv×nb ≈ 8M → reines Stack-Modell)
  → 7,7×. Aggressiveres Inlining braucht ZUERST einen Regalloc, der grosse
  Funktionen verkraftet (Intervall-Splitting; die 8M-Grenze ist ein
  Sicherheitsnetz, keine Loesung).
* Nebenbefund: `hat_schleife` per Nummerntest (`ziel <= id`) erkannte nach
  `merge-blocks` falsche Schleifen (Join `bb14→bb12`) — ersetzt durch echten
  Zyklentest; Eigenschaften werden jetzt einmal vorberechnet statt pro Scan
  (Endlos-Uebersetzungszeit behoben).

## Staendig sichtbare Muster im erzeugten Code (Karte fuer Runde 38+)

* **445×** statisch `mov [rbp-X], rA` gefolgt von `mov rA, [rbp-X]`
  (Spill-Store mit sofortigem Reload desselben Werts) — ein
  Register-Deskriptor-Cache im RA-Emissionspfad wuerde sie streichen.
* **7391×** reg→reg-`mov` statisch: die SSA-Form schreibt jede Instruktion in
  "ihr" Register, der Verbraucher movt weiter — klassisches **Coalescing**
  (Use bekommt das Register des einzigen Produzenten) ist der naechste grosse
  Hebel, zusammen mit Intervall-Splitting.
* Strukturell: `dekodiere` (31 % Ir) ist ein eigener UTF-8→UTF-32-Durchgang,
  den html5ever nicht braucht; `eingabe_pruefen` kostet als Aufruf ~108
  Ir/Zeichen. Das ist Architektur des Benchmarks, nicht Optimierer-Versagen.

## Firn-Seite (firnc1)

firnc1 enthaelt bewusst KEINEN Optimierer (docs/SELBSTHOSTING.md: „Der
Optimierer kommt zuletzt; firnc1 darf ohne ihn arbeiten"). Der fir-Vergleich
laeuft auf `--emit=fir-raw`, also VOR jeder Optimierung — Aenderungen an
`opt.rs`/`inline.rs`/`regalloc.rs` veraendern die verglichene rohe FIR nicht.
Der Fixpunkt bleibt zeichengleich, weil der Quelltext von firnc1 unangetastet
bleibt. Es gibt daher in dieser Runde nichts zu spiegeln; geprueft ueber
selbst_vergleich (186/0/0) und fixpunkt.sh in jedem Testlauf.


## Endzahlen (Commit bf13ed4)

| Korpus | Runde-36-Baseline | Runde 37 | Bewertung |
|---|---|---|---|
| html5lib | 1,94× | **1,69×** | Ziel ≤2× erreicht |
| realweb | 4,82× | **4,34×** | Zwischenziel ≤3× verfehlt |

Gesamt gesund: **test.sh 640/640**, selbst_vergleich 186/0/0,
Fixpunkt 284207 Zeilen zeichengleich. Drei Commits: 977a2ad (Fallthrough +
cmp→Zielregister), ef4e530 (Registerpools), bf13ed4 (Inline-Korrektheit +
Shift-Sofortform).

## Naechster Hebel (Prioritaet fuer Runde 38)

1. **Regalloc: Intervall-Splitting + Coalescing** (7391 statische reg→reg-movs,
   445 Store/Reload-Paare; danach vertraegt der Compiler groessere Funktionen
   und das aggressive Inlining von `eingabe_pruefen` & Co. wird moeglich —
   das ist der dokumentierte Pfad zu ≤3×).
2. Register-Deskriptor im Emissionspfad (Store→Reload-Muster direkt streichen).
3. GC-Runde: `520_gc_weak` rahmenlayout-robust machen (Stapel-Scrub im
   Testpfad), sonst bleibt jede Inline-Verbesserung ein Roulette.

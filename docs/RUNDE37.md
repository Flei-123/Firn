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
| C | Registerpools rsi/rdi (ohne call/memop) + rdx (ohne call/div/select) im linear scan | 1,71× | 4,42× | in Verifikation |

## Firn-Seite (firnc1)

firnc1 enthaelt bewusst KEINEN Optimierer (docs/SELBSTHOSTING.md: „Der
Optimierer kommt zuletzt; firnc1 darf ohne ihn arbeiten"). Der fir-Vergleich
laeuft auf `--emit=fir-raw`, also VOR jeder Optimierung — Aenderungen an
`opt.rs`/`inline.rs`/`regalloc.rs` veraendern die verglichene rohe FIR nicht.
Der Fixpunkt bleibt zeichengleich, weil der Quelltext von firnc1 unangetastet
bleibt. Es gibt daher in dieser Runde nichts zu spiegeln; geprueft ueber
selbst_vergleich (186/0/0) und fixpunkt.sh in jedem Testlauf.

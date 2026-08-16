
## Runde 35 (16.08.2026) — comptime in firnc1, Commit 5e16d8a
Parallel-Experiment: eigenes git-Worktree (Branch r35-comptime), Merge fast-forward, null Konflikte.
Gebaut: lib/firnc1/zeit.fi (689 Z. Interpreter nach comptime.rs-Vorbild), Parser liest comptime-Bloecke,
Treiber bin/firnc1.fi fuehrt sie zwischen Wurzelparser und Monomorphisierung aus, erzeugter Text wird
als Modul ohne Alias in denselben Baum geparst. Messwerte: test.sh 634/634, selbst 166->169
(601 comptime_emit + 602 comptime_ucd + 760_kern), 0 abweichend, Fixpunkt 210324 Zeilen zeichengleich.
Grenzen ehrlich benannt: comptime nur in Wurzeldatei; Konstanten-Fall separat.
Werkzeug-Fix: fixpunkt.sh baut .firnc1 neu, wenn Quellen juenger (veraltetes Binary haette den Merge
mit 166 statt 169 "bestaetigt" — Fund erst durch Abweichung Worktree/Hauptrepo aufgefallen).
Gelernt: Worktree-Parallelisierung funktioniert fuer isolierte Bloecke; Worker ohne Commit = Arbeit weg
(Runde-34-Worker ans Limit gelaufen, nichts committet — neu beauftragt mit Commit-Regel).

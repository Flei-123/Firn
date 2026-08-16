
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

## Runde 34 (16.08.2026) — gc class / Gc[T] / #[no_gc] in firnc1, Commits 6b406cf + folgend
Der Worker lief ans Limit ohne Abschluss, aber mit unfertiger Arbeit im Baum (diesmal: 10 geaenderte
Dateien + gc.fi 854Z, gctext.fi 323Z, nogc.fi 341Z — gerettet und selbst zu Ende gefuehrt).
FUND am Korpus: GC-Scan im Treiber nutzte intern_finde — Nummern existieren nur, wenn die WURZEL
die Woerter enthaelt; stand `gc class` nur im Modul (560 -> modules/dom.fi), scannte er mit -1 und
fand nichts (stiller Sema-Fehler). main.rs nutzt intern_nummer — jetzt hier auch. Aufgespuert ueber
Bisektion mit Mini-Modulen + Instrumentierung (exit-Codes 101+, dann Zaehler-Prints in fehler.fi).
Messwerte: test.sh 637/637, selbst 169->180 (alle 9 gc-Dateien + 770_kern), 0 abweichend, Fixpunkt
279201 Zeilen zeichengleich. 6 gc/nogc-Negativtests brechen wie firnc0 ab.
Verbleibend: konstante Laufzeit (4), errdefer (1), must_consume (1) — Runde 36.

RUNDE 36 (16.08.2026, Commits 6ef2616 + 3144601 + 2e7d8a8): die letzten drei Kernbloecke.
#[must_consume] (attrs.rs-Vorbild, check_discard in sema.fi), errdefer (defer_bis_fehler /
ret_term_fehler, Union-Weitergabe abgelehnt), ct-Intrinsics select + secure_zero (ct.rs-Vorbild;
Parser-Kernregistrierung wie barrier, cmov-Codegen, secure_zero unweggoptimierbar).
Messwerte: test.sh 640/640, selbst 180->185 gleich / 0 abweichend / 0 fehlerhaft / NICHT KERN 0,
Fixpunkt 284207 Zeilen zeichengleich. Negativtests ct_select_*, ct_secure_zero_kein_zeiger,
errdefer_union_weitergabe, attr_must_consume_* brechen wie firnc0 ab (rc=1).
Verbleibend ehrlich benannt: 600_comptime.fi (rc=4, COMPTIME 1) — Kernsprache sonst vollstaendig.

## Runde 37+38+39 (16.08.2026) — drei parallele Tracks, gemergt
R37 Optimierer (Hauptrepo, Commits 977a2ad/ef4e530/bf13ed4): html5lib 1,94x->1,69x (Ziel <=2x
erreicht), realweb 4,82x->4,34x. firnc1 hat bewusst keinen Optimierer — fir-Vergleich auf --emit=fir-raw,
nichts zu spiegeln. Naechster Hebel: Intervall-Splitting+Coalescing (7391 reg->reg-movs).
R38 GC (Worktree r38-gc, f498740/5ab519f/47ab946): leere Chunks aller Klassen ans OS (Hysterese),
Grenzen-Kappe 4 MiB (frag2 RSS-Ende 24124->2112 KiB), hybrid inkrementell ab 8 MiB (Pausen ~0,5 ms
heap-unabhaengig, Durchsatz -8,7% innerhalb Vorgabe). NEBENBEFUND: Optimierer entfernt letztes
Null-Setzen als tot — Unerreichbarkeit in Hilfsfunktion sterben lassen (Scrubber). Finalisierer/Arc[T]
benannte Restarbeit. 30-Min-Dauerlauf nachgeholt (laeuft).
R39 std+Interpolation (Worktree r39-std, ebc3c1e/e4fc9bd/ed0faf7): Suchpfad $FIRNLIB + <exe>/../lib
in beiden Compilern, lib/std-Fassade (io/math/str/vec/map/num/mem), f"..." zur Uebersetzungszeit in
Fmt-Kette zerlegt (keine Varargs), 790/791 Kern-Tests, 3 Negativtests. Live aus /tmp verifiziert.
WERKZEUG-FIX (wieder dieselbe Falle): lex/parser/typen/fir/sema_vergleich bauten ihre Dump-Binaries
nur "wenn fehlend" — veraltete Dumps nach R39 liessen die Vergleiche fehlschlagen. Jetzt Neubau bei
juengeren Quellen (wie fixpunkt.sh seit R35). fixpunkt.sh exportiert jetzt auch FIRNLIB.

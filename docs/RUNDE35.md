# Runde 35 — comptime in firnc1

## Ziel
`comptime { ... }`-Bloecke zur Uebersetzungszeit ausfuehren: echter Interpreter in Firn
(Vorbild `compiler/src/comptime.rs`), erzeugter Quelltext wird als weiteres Modul
in denselben Baum geparst, bevor die Monomorphisierung laeuft.

## Gebaut
- `lib/firnc1/zeit.fi` (689 Z.): Registrierung der comptime-Bloecke + Interpreter
  (Ausdruecke, Anweisungen, Schleifen, Aufrufe auf comptime-fn), Ausgabe in rt.Buf.
- `ast.fi`: `ct_block`-Sammelliste in der Wurzeldatei.
- `parser.fi`: liest `comptime { }`, Vorabsuche weicht auf (Bloecke ohne Namensbindung),
  Flag `par_zeit_setzen`/`par_zeit_an` zur Steuerung.
- `bin/firnc1.fi`: Treiber-Anschluss — zwischen Wurzelparser und `mono.gen_lauf`:
  `zeit_lauf` ausfuehren, erzeugten Text ueber dieselbe Lex/Parse-Maschinerie als
  Modul ohne Alias in denselben Baum haengen (gleicher Interner).

## Ehrliche Grenzen (benannt, nicht verschwiegen)
- comptime in importierten Modulen wird von `sema_braucht_comptime` weiter gemeldet
  (nur die Wurzeldatei wird ausgefuehrt).
- Konstanten, die zur Uebersetzungszeit ausgewertet werden muessten, aber nicht
  koennen, bleiben ein separater bekannter Fall.

## Messwerte (Worktree-Checkout, Branch r35-comptime)
- test.sh: 634/634 (vorher 631)
- tools/selbst_vergleich.sh: 169 verhaltensgleich (vorher 166), 0 abweichend, 0 fehlerhaft
- Fixpunkt: Stufe 2 == Stufe 3, zeichengleich, 210324 Zeilen Assembler
- Neu: tests/760_comptime_kern.fi (comptime-Kernsprache)
- Zieldateien 601/602 (u. a. UCD-Tabellen-Erzeugung) laufen identisch zu firnc0

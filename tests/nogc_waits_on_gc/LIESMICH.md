# `#[no_gc]`-Regeln (i) und (iii) — warten auf den GC-Kern

Die Pruefung in `compiler/src/nogc.rs` fragt fuer

* **(i)** GC-Allokation  → `crate::gc::ist_gc_alloc_aufruf(name)`
* **(iii)** Schreiben in ein `Gc[T]`/`GcWeak[T]`-Feld → `crate::gc::ist_gc_zeiger(typ)`

ausschliesslich die beiden Vertragsfunktionen aus `compiler/src/gc.rs`
(Modul `gckern`). Solange die dort noch `false` liefern (Skelettfassung),
kann **kein Firn-Programm** die Regeln (i) und (iii) ausloesen — es gibt ja
weder `gc class` noch `Gc[T]`.

Die beiden Programme hier sind deshalb **nicht** in `tests/neg/`: `test.sh`
verlangt dort einen echten Compilerfehler, und ein solcher entstuende heute
nur aus dem Parser („der Gc-Heap ist nicht umgesetzt"), nicht aus der
`#[no_gc]`-Pruefung. Sobald `gc.rs` die beiden Abfragen beantwortet, gehoeren
die Dateien unveraendert nach `tests/neg/` — die erwarteten Meldungen stehen
in Zeile 1.

Bis dahin sind die Regeln (i) und (iii) **im Compiler selbst** nachgewiesen:
`cargo test --release` fuehrt in `nogc.rs` die Tests
`regel1_gc_allokation_ist_verboten`, `regel3_schreiben_in_gc_feld_ist_verboten`
und `regel3_zuweisung_an_oertliche_veraenderliche_ist_erlaubt` aus. Sie
setzen fuer die beiden Abfragen Vorhersagen ein und pruefen Meldung, Zeile
und Spalte. Der Compiler selbst benutzt immer `Regeln::echt()`, also `gc.rs`
(Test `echte_regeln_sind_die_aus_gc_rs`).

**Beim Umzug nach `tests/neg/` zu pruefen:** Zeile und Spalte in Zeile 1
stimmen mit der Stelle ueberein, die `gckern` fuer `gc …{…}` bzw. fuer den
Feldnamen als Span vergibt; notfalls dort nachziehen. Die Meldungstexte sind
in `nogc.rs` festgelegt und aendern sich nicht.

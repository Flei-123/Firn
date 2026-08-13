# PLAN.md — Bauplan `firnc0` (Stufe 0)

**Bezug:** `SPEC.md` (§10.1 Grammatik, §11 ABI/Zahlen, §12 + §12.1 verbindlicher
Umfang), `ROADMAP.md` Phase 1. **SPEC.md und ROADMAP.md werden inhaltlich nicht
umgeschrieben** — §12.1 wurde ergaenzt und haelt die bewussten Einschraenkungen
der Umsetzung fest.

Das Geruest steht bereits und **baut** (`cargo build --release`). Die geteilten
Datenstrukturen (`config.rs`, `diag.rs`, `ast.rs`, `types.rs`, `fir.rs`,
`main.rs`) sind FERTIG und gelten als **eingefrorene Schnittstelle**. Die
uebrigen Dateien sind Stubs, die einen sauberen Fehler melden (kein `todo!()`).

---

## 1. Dateieigentum — wer fasst was an

| Modul | ausschliesslich diese Dateien |
|---|---|
| **frontend** | `compiler/src/lexer.rs`, `compiler/src/parser.rs` |
| **sema** | `compiler/src/sema.rs` |
| **lowering** | `compiler/src/lower.rs`, `docs/FIR.md` |
| **opt** | `compiler/src/opt.rs`, `tests/opt/**`, `test_opt.sh` |
| **codegen** | `compiler/src/codegen_x86.rs` |
| **suite** | `tests/*.fi`, `tests/neg/*.fi`, `examples/*.fi`, `test.sh`, `README.md` |

**Niemand** aendert `config.rs`, `diag.rs`, `ast.rs`, `types.rs`, `fir.rs`,
`main.rs`, `Cargo.toml`, `SPEC.md`, `ROADMAP.md`. Faellt eine Schnittstelle als
unzureichend auf: im eigenen Modul umgehen und in der Rueckmeldung nennen —
nicht eigenmaechtig die geteilte Datei aendern (Merge-Konflikt = Bauabbruch).

Keine neuen Dateien in `compiler/src/` (das Modulverzeichnis in `main.rs` ist
fix: `ast, codegen_x86, config, diag, fir, lexer, lower, opt, parser, sema,
types`). Untermodule innerhalb der eigenen Datei sind erlaubt.

---

## 2. Harte Regeln fuer alle

* Keine externen Crates. Nur `std`. `Cargo.toml` bleibt ohne Abhaengigkeiten.
* **Null Warnungen** bei `cargo build --release`. Keine `#![allow(...)]`-
  Sammelunterdrueckung; einzelne, begruendete `#[allow]` nur im Ausnahmefall.
* Kein `todo!()`, `unimplemented!()`, `panic!("...")`, kein `unwrap()`/`expect()`
  auf Werten, die von der Eingabe abhaengen. Nicht Unterstuetztes meldet
  `dg.error(span, "... wird in Stufe 0 nicht unterstuetzt")`.
* Kein Absturz bei kaputter Eingabe: keine Endlosschleife (jeder Parser-Loop
  muss beweisbar vorankommen), keine Rekursionsexplosion (Tiefenzaehler mit
  klarem Fehler bei > 200 Verschachtelungen).
* Sprachname/Endung nur aus `config.rs` (`LANG_NAME`, `LANG_NAME_LOWER`,
  `FILE_EXT`). Nirgends sonst "Firn"/"fi" hartkodieren — auch nicht in Texten.
* Deutsche Fehlermeldungen, klein geschrieben nach `error: `.
* Selbstpruefung vor der Fertigmeldung: `cargo build --release` + eigene Tests
  tatsaechlich ausfuehren.

---

## 3. Gemeinsame Semantik (gilt fuer sema, lowering, codegen gleichermassen)

* **Typabbildung `types::Type` -> `fir::FTy`:**
  `i8..i64 -> I8..I64`, `u8..u64 -> U8..U64`, `usize -> U64`, `isize -> I64`,
  `bool -> Bool` (1 Byte, Werte 0/1), `*T`/`*mut T` -> `Ptr`.
  Arrays/Structs sind **keine** FIR-Werte: sie existieren nur als Adresse
  (`alloca` + `ptradd` + `load`/`store` auf Feldern, `copymem` beim Kopieren).
* **Keine impliziten Umwandlungen.** Nur `as`. `bool` <-> Ganzzahl nur mit `as`.
  `as` zwischen Zeigern und `usize`/`isize` ist erlaubt, zwischen Zeigern
  unterschiedlicher Zieltypen ebenfalls; `as` auf/aus Aggregaten ist ein Fehler.
* **Vorzeichen:** Erweiterung bei `as` richtet sich nach dem QUELLtyp
  (signed -> `movsx`, unsigned/bool -> `movzx`), Verkuerzung schneidet ab.
  `/` `%` und `>>` richten sich nach dem OPERANDENtyp (signed: `idiv`/`sar`,
  unsigned: `div`/`shr`).
* **Literale:** siehe SPEC §12.1 Punkt 2 — kein Vorgabetyp, sonst Fehler
  "typ des ganzzahlliterals ist nicht ableitbar, schreibe z. B. `5 as i32`".
* **Aggregate an Funktionsgrenzen verboten** (SPEC §12.1 Punkt 1): Parameter und
  Rueckgabetyp muessen skalar sein; `sema` meldet das mit klarem Fehler.
* **`main`:** muss existieren, keine Parameter, Rueckgabetyp `i32`.
* **`syscall(nr, a1..a6)`:** 1 bis 7 Argumente, jedes von Ganzzahl- oder
  Zeigertyp; jedes wird nach `i64` erweitert (signed -> sign-, sonst
  zero-extend). Ergebnistyp ist `i64`. Mehr als 7 Argumente = Fehler.
* **Namensraeume:** Funktionen, Structs und Konstanten liegen je in einem
  eigenen globalen Namensraum; lokale Namen verdecken Konstanten. Doppelte
  Deklaration im selben Namensraum = Fehler. Verdecken (`shadowing`) im selben
  Block = Fehler, in einem inneren Block erlaubt.

---

## 4. Eingefrorene Schnittstellen (bereits im Baum)

```rust
// lexer.rs      pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token>
// parser.rs     pub fn parse(toks: &[Token], dg: &mut Diags) -> ast::Program
// sema.rs       pub fn check(prog: &ast::Program, dg: &mut Diags) -> Option<TypeInfo>
// lower.rs      pub fn lower(prog: &ast::Program, info: &TypeInfo, dg: &mut Diags) -> Option<fir::Module>
// opt.rs        pub fn optimize(m: &mut fir::Module) -> OptStats
// codegen_x86.rs pub fn emit(m: &fir::Module) -> String
```

`diag::Diags` liefert das vorgeschriebene Fehlerformat (Datei, Zeile, Spalte,
Quelltextzeile, `^^^`-Markierung) — Module bauen keine eigenen Formate.

`main.rs` bietet bereits: `-o`, `--emit=exe|asm|fir|fir-raw|fir-opt|tokens|ast`,
`--no-opt`, `--keep-asm`, `--version`, `--help`; assembliert mit `as --64` und
linkt mit `ld -n` (nur als Assembler/Linker).

---

## 5. Modulauftraege im Ueberblick

1. **frontend** — Lexer + rekursiv absteigender Parser mit Fehlerwiederherstellung.
2. **sema** — Typpruefer, Struct-Layout, `TypeInfo` fuer das Lowering.
3. **lowering** — AST -> FIR (Basisbloecke, Kurzschluss, lvalues) + `docs/FIR.md`.
4. **opt** — Konstantenfaltung + Entfernen toten Codes, mit eigenen Nachweistests.
5. **codegen** — FIR -> x86_64-Assembler, System-V-ABI, freistehend ohne libc.
6. **suite** — >= 40 Testprogramme, >= 8 Negativtests, Beispiele, `test.sh`, README.

Reihenfolge der Abhaengigkeit ist 1->2->3->5, aber jedes Modul ist **einzeln
pruefbar**: opt und codegen bauen ihre Test-FIR programmatisch (`fir::Func::new`,
`push`, `set_term`) in `#[cfg(test)]`-Tests und pruefen mit `cargo test` bzw.
`as`/`ld`; die suite schreibt ihre `.fi`-Programme gegen SPEC §10.1 und laesst
sie am Ende gegen den fertigen Compiler laufen.

## 6. Abnahmekriterium

`cargo build --release` ohne Warnung, `bash test.sh` meldet real
`PASS n/n` (mit UND ohne `--no-opt`), `examples/hello.fi` gibt sichtbaren Text
aus, `examples/fib.fi`, `examples/bubblesort.fi`, `examples/structs.fi` liefern
nachpruefbare Ergebnisse, Negativtests brechen mit Exit-Code != 0 und
verstaendlicher Meldung ab — ohne Rust-Panik.

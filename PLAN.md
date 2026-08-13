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

---
---

# PLAN — Runde 2 (Phase 2 der ROADMAP): Sprachkern fuer die Browser-Engine

**Stand Beginn Runde 2:** `firnc0` laeuft, 7.760 Zeilen Rust, `bash test.sh`
meldet **PASS 166/166** (75 Programme x 2 Durchlaeufe, 15 Negativtests,
18 Optimierer-Nachweise), `cargo build --release` ohne Warnungen.

**Bezug:** `SPEC.md` v0.2 (§3 Speichermodell, §4.4 Vererbung, §6.3 match,
§8 Zeichenketten/WTF-16, §9 Constant-Time, §10.3 Leistungsziel, §14/§14.1),
`ROADMAP.md` Phase 2, `ABNAHME.md` (sechs Punkte),
`../karstos-browser/FIRN-ANFORDERUNGEN.md` (nur lesen).

**Grundsatz dieser Runde:** `SPEC.md` ist der Vertrag. Wer bewusst enger baut,
traegt das in **§14.1** als nummerierten Punkt ein und schreibt die SPEC NICHT
um. Wer einen Punkt aus §14.1 aufhebt, streicht ihn dort mit Begruendung.
Nicht Erreichtes kommt ehrlich in `ABNAHME.md` und `README.md`.

## 0. Was der Lead-Architekt in dieser Runde bereits gebaut hat (steht, nicht neu bauen)

Die geteilte IR wurde **einmalig zentral** erweitert, damit die sechs Module
danach parallel und konfliktfrei arbeiten koennen:

| Neu in `compiler/src/fir.rs` | Bedeutung | wer verdrahtet es |
|---|---|---|
| `Term::Switch { val, ty, cases, default }` | Mehrfachverzweigung, `cases` aufsteigend sortiert und duplikatfrei | `types` |
| `Op::Select { cond, a, b }` | datenunabhaengige Auswahl, wird **immer** `cmov` | `ct` |
| `Op::Barrier { val }` | undurchsichtige Sperre, nie wegoptimierbar (`is_pure() == false`) | `ct` |
| `Op::SecureZero { addr, size }` | `secure_zero`, gilt NIE als toter Code | `ct` |
| `Func::secret: HashSet<Val>` + `set_secret`/`is_secret` | `secret`-Markierung bis in die IR (SPEC §9.2) | `ct` |
| `Func::constant_time: bool` | schaltet die Codegen-Pruefung ein | `ct` |

Bereits funktionsfaehig und getestet (`cargo test`, 67 Tests gruen):
* `codegen_x86.rs` erzeugt fuer `Select` ein `cmov`, fuer `SecureZero` ein
  `rep stosb`, fuer `Barrier` einen undurchsichtigen Durchlauf.
* `codegen_x86.rs` bricht mit **Fehler** ab, wenn in einer `constant_time`-
  Funktion ein `brcond`/`switch` von einem `secret`-Wert abhaengt.
* `opt.rs` kennt `Term::Switch` (Erreichbarkeit, Neunummerierung, Faltung bei
  konstanter Marke) — die Optimierung bleibt verhaltenserhaltend.
* Neue Datei `codegen_switch.rs`: `emit_switch()` als **Vergleichskette**
  (korrekt, aber linear) — die Sprungtabelle baut `types` genau dort ein.
* `Emitter`, `Frame`, `load_full`, `load_ext`, `store_dst`, `reg`, `size_word`,
  `block_label`, `ARG_REGS` sind `pub(crate)`: fremde Dateien duerfen eigene
  `impl`-Bloecke und Emissionsfunktionen beisteuern, ohne `codegen_x86.rs`
  anzufassen.

Die fuenf `#[allow(dead_code)]`-Markierungen an genau diesen neuen Elementen
sind **temporaer**. Wer das Element verdrahtet, **entfernt die Markierung**.
Eine Sammelunterdrueckung (`#![allow(...)]`) ist und bleibt verboten.

## 1. Dateieigentum in Runde 2 — wer fasst was an

Zwei Module fassen **niemals** dieselbe Datei an. Wo ein Modul eine Funktion in
einer fremden Datei braucht, schreibt es sie in eine **eigene Datei** (Rust
erlaubt `impl Fremdtyp { }` innerhalb desselben Crates).

| Modul | ausschliesslich diese Dateien |
|---|---|
| **kern** | `compiler/src/lexer.rs`, `parser.rs`, `ast.rs`, `diag.rs`, `sema.rs`, `lower.rs`, `codegen_x86.rs`, neu: `abi.rs`, `modules.rs`, `dwarf.rs`; `tests/1xx_*.fi`, `tests/neg/kern_*.fi`; `tools/testrunner/**`; `docs/SELBSTHOSTING.md` |
| **types** | neu: `compiler/src/sema_match.rs`, `sema_generic.rs`, `mono.rs`, `lower_match.rs`; vorhandene `codegen_switch.rs`; `tests/2xx_*.fi`, `tests/neg/match_*.fi`, `tests/neg/generic_*.fi` |
| **str** | neu: `compiler/src/strings.rs` (Literale/Typen im Compiler), `lib/str/**.fi`, `lib/num/**.fi`; `tools/dtoa_vectors/**`; `tests/3xx_*.fi`, `tests/neg/str_*.fi` |
| **opt** | `compiler/src/opt.rs`, neu: `regalloc.rs`, `inline.rs`, `mem2reg.rs`; `tests/opt/**`, `test_opt.sh`; `bench/**` |
| **ct** | neu: `compiler/src/ct.rs`, `int128.rs`; `lib/ct/**.fi`; `tests/4xx_*.fi`, `tests/neg/ct_*.fi` |
| **tok** | `tokenizer/**.fi`, `tools/html5lib_harness/**`, `bench/tokenizer/**`; `tests/5xx_*.fi` |

Gemeinsame Dateien, an denen **nur zeilenweise angehaengt** wird (nie
umstrukturiert): `compiler/src/main.rs` (je Modul genau eine `mod`-Zeile und
hoechstens eine CLI-Option), `README.md`, `ABNAHME.md`, `SPEC.md` §14.1 — jedes
Modul schreibt dort in **seinen eigenen, mit dem Modulnamen ueberschriebenen
Abschnitt**. `config.rs`, `fir.rs`, `Cargo.toml` bleiben **unveraendert**
(Aenderungsbedarf an `fir.rs` wird gemeldet, nicht eigenmaechtig gemacht — er
bricht alle anderen Module).

## 2. Reihenfolge und Abhaengigkeiten

```
kern  (Aggregate/ABI, Stapelargumente, break/continue/for, [v;N], Module)
  |          \
  |           \--> types (enum/match/Sprungtabelle, Generics)
  |                   \
  |                    \--> str (Bytes/Str/Str16/Atom, strtod, dtoa)
  |                            \
  |                             \--> tok (HTML5-Tokenizer in Firn)
  +--> opt (Registerzuteilung, Inlining, mem2reg, Blockverschmelzung, bench/)
  +--> ct  (secret, select/secure_zero/barrier, u128, #[constant_time])
```

`opt` und `ct` haengen **nicht** an `types`/`str` und laufen sofort los.
`tok` beginnt mit dem Harness (Rust) und den Tokenizer-Zustaenden, die schon
mit dem heutigen Sprachumfang schreibbar sind, und zieht nach, sobald
`types`/`str` liefern.

**Wenn ein Modul blockiert ist:** liefert es den Teil, der ohne die fehlende
Zulieferung geht, und schreibt den Rest ehrlich als offen in `ABNAHME.md`.
Halbfertiges wird NICHT als fertig gemeldet.

## 3. Feste Schnittstellen zwischen den Modulen

### 3.1 kern -> alle: Aufrufgrenze und Module

* `abi.rs`: `pub enum ArgClass { Integer(u8 /*Anzahl 8-Byte-Woerter*/), Memory, Sse }`
  und `pub fn classify(ty: &types::Type, tcx: &TypeCtx) -> ArgClass` nach
  System V AMD64. Rueckgabe > 16 Byte = `Memory` (versteckter Zeiger in `rdi`,
  `rax` liefert ihn zurueck). Diese Funktion ist die **einzige** Wahrheit ueber
  die Aufrufkonvention; `opt` (Inlining) und `types` (Monomorphisierung) rufen
  sie, statt eigene Regeln zu erfinden.
* Stapelargumente: Argumente ab dem 7. INTEGER-Wort liegen bei `[rsp+8*k]` vor
  dem `call`, 16-Byte-Ausrichtung bleibt erhalten. §14.1 Punkt 1 und 9 werden
  danach in `SPEC.md` gestrichen (mit Begruendungszeile).
* `modules.rs`: `pub fn resolve(root: &Path) -> Result<Vec<SourceFile>, Diag>`;
  `SourceFile { id: u32, path: PathBuf, src: String }`. `diag::Span` bekommt ein
  Feld `file: u32`; `Diags` haelt eine `SourceMap`. **Das ist die einzige
  Aenderung an `diag.rs` und gehoert kern** — alle anderen Module benutzen
  `Span` weiterhin nur ueber die bestehenden Konstruktoren.
* Syntax (fest, danach eingefroren): `import pfad.modul`, `export { a, b }`,
  Namensraum-Zugriff `modul.name`. Aufloesung relativ zur Wurzeldatei.
* `for i in a..b { }`, `break`, `continue`, `[wert; N]` — Entzuckerung
  ausschliesslich in `lower.rs`.

### 3.2 types -> alle: Summentypen, Muster, Generics

* AST/Parser-Erweiterungen macht **kern nicht**, sondern `types` — aber
  ausschliesslich in eigenen Dateien: Parser-Einstiegspunkte werden als
  `impl<'a> crate::parser::Parser<'a> { pub(crate) fn parse_enum_decl(..) }`
  in `sema_match.rs`-Nachbardateien gelegt; kern haelt in `parser.rs` genau
  **eine** Aufrufzeile je Konstrukt frei (`enum`, `match`, generische
  Parameterliste `[T]`) und markiert sie mit `// HOOK types`.
* Vollstaendigkeitspruefung: `pub fn check_exhaustive(...) -> Result<(), Diag>`
  in `sema_match.rs`. Ein fehlender Fall ist ein **Fehler** mit Zeile/Spalte und
  Nennung der fehlenden Variante — kein Warnhinweis.
* Lowering erzeugt `Term::Switch`; die Sprungtabelle entsteht in
  `codegen_switch.rs` unter den dort dokumentierten Bedingungen
  (`MIN_TABLE_CASES = 8`, Dichte >= 40 %). Nachweis: `--emit=asm` einer
  Zustandsmaschine mit >= 30 Zustaenden zeigt `jmp qword ptr [...]` ueber eine
  `.rodata`-Tabelle, nicht 30 `cmp`.
* Monomorphisierung in `mono.rs`: erzeugt Funktionsnamen nach dem festen Schema
  `name__T1_T2`. Dieses Schema ist Vertrag (Debugger, Inlining, Tests).

### 3.3 str -> tok: Zeichenketten

* Compilerseite (`strings.rs`): Literale `"..."` (UTF-8, `Str`), `b"..."`
  (`Bytes`), `u"..."` (`Str16`), Escapes inkl. `\uXXXX` **mit** ungepaarten
  Surrogaten. Typen als eingebaute Structs mit festem Layout:
  `Bytes { ptr: *mut u8, len: usize, cap: usize }`,
  `Str` = gleiches Layout, geprueft; `Str16 { ptr: *mut u16, len, cap }`,
  `Atom = u32`.
* `Str16` prueft NICHTS. Pflichttest: einzelnes `0xD800` bleibt erhalten,
  `to_utf8()` liefert nichts, `to_utf8_lossy()` liefert U+FFFD.
* Firn-Bibliothek `lib/str/`: `str16.fi`, `utf8.fi`, `atom.fi`;
  `lib/num/`: `strtod.fi`, `dtoa.fi` (kuerzeste Ausgabe mit
  Rueckwandlungsgarantie). Testvektoren: 0.1, 1e23, 5e-324, 9007199254740993,
  2.2250738585072011e-308, Rundung auf gerade + Zufallstest ueber >= 100.000
  Doubles (f64 -> Text -> f64 bitgleich).
* API, auf die `tok` sich verlaesst (Namen sind Vertrag):
  `fn str16_new() -> Str16`, `fn str16_push(inout s: Str16, u: u16)`,
  `fn str16_len(s: &Str16) -> usize`, `fn str16_at(s: &Str16, i: usize) -> u16`,
  `fn atom_intern(b: &Bytes) -> Atom`.

### 3.4 opt -> alle: Optimierer

* Eintrittspunkt bleibt `pub fn optimize(m: &mut fir::Module) -> OptStats`.
  `OptStats` darf **nur erweitert** werden (neue Felder), nie umbenannt.
* `regalloc.rs`: `pub struct Alloc { ... }`, `pub fn allocate(f: &Func) -> Alloc`
  mit `pub fn loc(&self, v: Val) -> Loc` (`Loc::Reg(&'static str)` oder
  `Loc::Slot(u64)`). `codegen_x86.rs` fragt genau ueber diese Funktion —
  **die eine Aenderung, die `opt` in `codegen_x86.rs` machen darf**, und sie
  wird mit kern abgestimmt (eine Zeile, markiert `// HOOK opt`).
* Harte Regel: `Op::Select`, `Op::Barrier`, `Op::SecureZero` und jeder Wert aus
  `f.secret` werden von **keinem** Durchgang veraendert oder entfernt; ein
  `select` wird **nie** zu einer Verzweigung (SPEC §9.2).
* `bench/`: mindestens 6 Mikrobenchmarks, je einmal in Firn und einmal in Rust
  (`rustc -O`, Ergebnis wird benutzt / `black_box`, damit nichts wegoptimiert
  wird), gleicher Rechner, mehrere Laeufe, **Median**. Ausgabe als Tabelle
  Firn / Rust / Faktor. Der Faktor wird eingetragen, **auch wenn er 4x ist**.

### 3.5 ct -> alle: Constant-Time

* `ct.rs`: `pub fn check_fn(...)` — verbietet Verzweigen auf `secret`,
  Indizieren mit `secret`, `/` und `%` auf `secret`, implizites Entlassen.
  `declassify(x)` ist der einzige Ausweg. Jede Ablehnung ist ein Fehler mit
  Zeile/Spalte.
* `int128.rs`: `u128`/`i128` fuer `+ - *`, Vergleiche, Verschiebungen sowie
  `mul_wide(u64, u64) -> (u64, u64)`. Darstellung: zwei 64-Bit-Woerter,
  Aggregat-ABI von kern (§3.1).
* Nachweis im Test: `ct_eq` uebersetzen, erzeugten Assembler im Funktionsrumpf
  nach bedingten Spruengen durchsuchen (Test schlaegt fehl, wenn welche da
  sind); Negativtest `if secret_bool { }` muss ein **Compilerfehler** sein;
  `secure_zero` muss im Assembler sichtbar bleiben.
* Speichermodell (Punkt 7 des Ziels): `Rc[T]`/`Weak[T]` zuerst. `Gc[T]`,
  `gc class`, Mark-Sweep und DOM-Dauerlauf **nur, wenn der Rest steht** —
  sonst wird es in `ABNAHME.md` ehrlich als *verschoben* gefuehrt. Halb gebauter
  GC ist ausdruecklich unerwuenscht.

### 3.6 tok: HTML5-Tokenizer

* Der Tokenizer ist **Firn** (`tokenizer/*.fi`). Der Harness darf Rust/Python
  sein (`tools/html5lib_harness/`).
* Der Harness liest **alle** 14 `.test`-Dateien, beachtet `doubleEscaped: true`
  (zusaetzliche `\uXXXX`-Entschluesselung von `input` UND `output`) und den
  Schluessel `xmlViolationTests`. **Uebersprungene oder nicht unterstuetzte
  Faelle zaehlen als FEHLSCHLAG**, niemals still als Erfolg.
* Ausgabe: Gesamtquote `n / 6810` plus Aufschluesselung je `.test`-Datei, dazu
  die Liste der umgesetzten Zustaende. Eine ehrliche 31-%-Quote ist das Ziel
  dieser Runde, keine geschoente.
* Durchsatzvergleich gegen `html5ever` in `bench/tokenizer/` — `html5ever` ist
  **Messlatte in einem getrennten Verzeichnis**, niemals Abhaengigkeit von
  `compiler/Cargo.toml`.

## 4. Nicht verhandelbare Regeln fuer alle Module

1. `bash test.sh` muss am Ende jedes Moduls durchlaufen. Kein Test wird
   entfernt oder abgeschwaecht. Neue Tests kommen dazu.
2. Jedes Programm laeuft mit **und** ohne `--no-opt` und liefert dasselbe.
3. `cargo build --release`: **null Warnungen**, keine Sammelunterdrueckung.
4. Kein `todo!()`, `unimplemented!()`, `panic!("...")` in den geforderten
   Pfaden. Nicht Umgesetztes = sauberer Compilerfehler mit Zeile/Spalte.
5. Compiler ohne externe Crates. Kein LLVM/Cranelift/C-Compiler. `as`/`ld` nur
   als Assembler/Linker.
6. Sprachname weiterhin ausschliesslich in `config.rs`.
7. Selbst messen, bevor „fertig" gemeldet wird: Testsuite laufen lassen,
   Benchmarks wirklich messen, Harness wirklich fahren. **Echte Zahlen** in
   `README.md` und `ABNAHME.md`.

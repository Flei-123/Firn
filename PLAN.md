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

---

# PLAN — Runde 3 (Haertetest 1): Fehlerunionen `E!T` + HTML5-Tokenizer in Firn

Stand beim Start: `bash test.sh` **PASS 393/393**, `cargo build --release`
ohne Warnungen. Ziel dieser Runde:

1. **Fehlerunionen `E!T`** als Sprachmittel (SPEC §5.1) — `error`, `try`,
   `catch`, implizite Umwandlung bei `return`, `!T` ist `#[must_consume]`.
2. **HTML5-Tokenizer in Firn** unter `lib/html/`, gemessen an der offiziellen
   html5lib-Testsuite (**6.810 Faelle**), mit ehrlicher Quote je `.test`-Datei
   und ehrlichem Geschwindigkeitsvergleich gegen `html5ever`.

## 0. Was der Lead vor dieser Runde bereits gebaut hat — steht, laeuft, nicht neu bauen

Nachgemessen mit `bash test.sh` → **PASS 397/397** und
`bash tools/tokenizer/run.sh --schnell` → **2.854 / 6.810 (41,9 %)**.

| Datei | Inhalt | Zustand |
|---|---|---|
| `lib/html/mem.fi` | mmap-Heap, `Buf` (u8-Vektor), `CpBuf` (u32-Vektor), `read_all_stdin`, `write_all` | fertig, Vertrag |
| `lib/html/tokens.fi` | Token-Modell (`Sink`, `Attr`), Verschmelzen der `Character`-Token, doppelte Attribute, **html5lib-JSON-Ausgabe in Firn** (ASCII, `\uXXXX`) | fertig, Vertrag |
| `lib/html/tokenizer.fi` | `enum State` + `match`; umgesetzt: Data, PLAINTEXT, TagOpen, EndTagOpen, TagName, BeforeAttributeName, AttributeName, AfterAttributeName, BeforeAttributeValue, AttributeValue×3, AfterAttributeValueQuoted, SelfClosingStartTag, BogusComment | **Geruest — hier wird gebaut** |
| `lib/html/tokenize_main.fi` | Wurzeldatei: Auftragsprotokoll lesen, WTF-8 dekodieren, `\r\n`→`\n`, Zeile je Auftrag schreiben | fertig |
| `tools/tokenizer/harness.py` | Werkbank: `doubleEscaped`, `xmlViolationTests`, `initialStates`, `lastStartTag`, Bilanz je Datei, JSON-Bericht | fertig |
| `tools/tokenizer/run.sh` | baut, faehrt drei Baustufen gegen dieselbe Bilanz, misst Durchsatz, prueft Regressionsschranke | fertig |
| `tools/tokenizer/durchsatz.sh`, `korpus.py` | 4-MB-Korpus + MB/s; ruft `bench/tokenizer/…/html5ever_bench`, wenn gebaut | Firn-Seite fertig, Referenzseite offen |
| `tools/tokenizer/PROTOKOLL.md` | Vertrag Firn ↔ Harness | fertig |
| `test.sh` Abschnitt 9 | Tokenizer-Lauf als Teil der Suite, Schranke `tools/tokenizer/mindestquote.txt` | fertig |
| `compiler/src/modules.rs` + `sema_match.rs` | **Fehlerbehebung:** `match`-Rumpfbloecke liegen in der Registrierung, nicht im AST — das Modulsystem hat Namen darin bisher nicht umgeschrieben. `match` in einem importierten Modul war unbenutzbar. Nachweis: `tests/231_modul_match.fi` + `tests/modules/zustand.fi` | erledigt |

**Bekannte Grenzen (ehrlich, gehoeren nach SPEC §14.1, nicht wegdiskutieren):**
* Aufzaehlungsnamen sind programmweit, nicht je Modul: `Ampel::Rot`, nicht
  `zustand.Ampel::Rot`.
* Fehlermeldungen in importierten Modulen zeigen Zeile/Spalte, aber den
  Dateinamen der Wurzeldatei (Modul `fehlerunionen` darf das mitnehmen, wenn
  Zeit bleibt — sonst als offen vermerken).
* `lib/html/mem.fi` dopplet rund 80 Zeilen aus `lib/str/alloc.fi`, weil
  `lib/str` ueber `//#include` und nicht ueber `import` eingebunden wird.

## 1. Dateieigentum in Runde 3 — zwei Module fassen NIE dieselbe Datei an

| Modul | Darf schreiben | Darf nur lesen |
|---|---|---|
| **fehlerunionen** | `compiler/src/errors.rs` (neu), `compiler/src/lower_errors.rs` (neu), Hook-Zeilen in `parser.rs`, `ast.rs`, `lexer.rs`, `sema.rs`, `lower.rs`, `types.rs`, `attrs.rs`; `tests/4??_*.fi`, `tests/neg/err_*.fi`; `SPEC.md` §14.1, `docs/FEHLERUNIONEN.md` | alles andere |
| **tokenizer-kern** | `lib/html/tokenizer.fi` | `lib/html/mem.fi`, `tokens.fi` |
| **tokenizer-text** | `lib/html/entities.fi` (neu), `lib/html/entities_data.fi` (erzeugt), `tools/tokenizer/gen_entities.py` (neu) | `lib/html/mem.fi` |
| **tokenizer-tokens** | `lib/html/tokens.fi`, `lib/html/tokenize_main.fi`, `lib/html/mem.fi` | — |
| **harness-bench** | `tools/tokenizer/harness.py`, `run.sh`, `durchsatz.sh`, `korpus.py`, `mindestquote.txt`, `bench/tokenizer/**` (neu), `bench/RESULTS.md`, `ABNAHME.md`, `README.md` | alles andere |

`test.sh` fasst **niemand** an ausser dem Lead bei der Zusammenfuehrung.
`tools/tokenizer/mindestquote.txt` schreibt nur **harness-bench** — und nur
nach oben, nie nach unten.

## 2. Schnittstellen — exakt, damit ohne Absprache gebaut werden kann

### 2.1 `mem` (fest, aendert nur `tokenizer-tokens`)
```
heap_alloc(n: usize) -> *mut u8      heap_free(p: *mut u8, n: usize)
mem_copy(dst, src, n)                write_all(fd: i64, p: *mut u8, n: usize)
Buf   : buf_neu() -> Buf, buf_init/free/clear/reserve/push/len/at/ptr/
        buf_set_len, buf_push_dec, buf_flush(b, fd), read_all_stdin(b)
CpBuf : cp_neu() -> CpBuf, cp_init/free/clear/push/len/at/set/truncate,
        cp_copy_from(dst, src), cp_eq_cp(a, b), cp_eq_ascii_lower(a, b)
```

### 2.2 `tokens` (fest, aendert nur `tokenizer-tokens`)
```
sink_neu() -> Sink        sink_init(s)     sink_free(s)
sink_begin(s)             sink_end(s)      sink_abort(s)   sink_flush_out(s)
sink_emit_char(s, cp)     sink_flush_chars(s)
tok_start_tag(s) tok_end_tag(s) tok_comment(s) tok_doctype(s)
tok_name_push(s, cp)      tok_comment_push(s, cp)  tok_doctype_name_push(s, cp)
tok_pubid_start(s) tok_pubid_push(s, cp)  tok_sysid_start(s) tok_sysid_push(s, cp)
tok_set_force_quirks(s, b)                tok_set_self_closing(s, b)
tok_attr_start(s) tok_attr_name_push(s, cp) tok_attr_value_push(s, cp) tok_attr_finish(s)
tok_emit(s)               tok_is_appropriate_end_tag(s) -> bool
sink_set_last_start_tag_cp(s, cpbuf)
```
Wer eine Funktion braucht, die es nicht gibt (z. B. Zugriff auf den aktuellen
Tagnamen fuer RCDATA), meldet sie bei `tokenizer-tokens` an — **niemand baut
sie sich selbst in `tokenizer.fi` nach.**

### 2.3 `entities` (Modul `tokenizer-text` liefert, `tokenizer-kern` ruft)
```
// Verarbeitet eine Zeichenreferenz. `pos` zeigt hinter das '&'.
// `in_attr` = true im Attributwert (Sonderregel des Standards).
// Rueckgabe: neue Position in der Eingabe.
// Die erzeugten Codepunkte werden ueber `out` angehaengt:
//   out == 0  -> tokens.sink_emit_char,  sonst tokens.tok_attr_value_push
fn char_ref(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
            s: *mut tokens.Sink, in_attribut: bool) -> usize
```
Die Namenstabelle wird von `tools/tokenizer/gen_entities.py` aus
`python3 -c "import html.entities"` (offizielle WHATWG-Liste der
Standardbibliothek) nach `lib/html/entities_data.fi` erzeugt — **nicht** aus
den Testdaten abgeleitet. Der Erzeuger liegt im Baum und ist wiederholbar.

### 2.4 Fehlerunionen (Modul `fehlerunionen`)
Empfohlener Weg laut Aufgabenstellung: `E!T` als zweivariantige getaggte Union
im `TypeCtx` (`__err: u32`, `0` = Erfolg, `__val: T`) plus Seitentabelle wie
`enum_by_struct`. Damit funktionieren Aggregatrueckgabe, System-V-ABI,
Registerzuteilung und Codegen ohne Aenderung.

## 3. Reihenfolge, Abhaengigkeiten

* `fehlerunionen` ist von allem anderen unabhaengig (nur Compiler + `tests/`).
* `tokenizer-kern` kann sofort loslegen; solange `entities` fehlt, ruft es
  weiter `sink_abort` fuer `&`.
* `tokenizer-text` kann sofort loslegen (eigene Dateien).
* `harness-bench` kann sofort loslegen; die Zahlen fuer `ABNAHME.md`/`README.md`
  werden **zuletzt** selbst gemessen und nur dann eingetragen.

## 4. Nicht verhandelbar

1. `bash test.sh` bleibt gruen — alle neun Abschnitte. Kein Test wird
   entfernt, umgeschrieben oder abgeschwaecht.
2. `cargo build --release` ohne Warnungen, keine externen Kisten im Compiler,
   kein `#![allow(...)]`, kein `todo!()`/`unimplemented!()`.
3. Jeder neue `tests/*.fi` laeuft in drei Baustufen mit demselben Ergebnis.
4. SPEC.md wird nicht umgeschrieben; Abweichungen kommen nach §14.1.
5. Keine geschoenten Zahlen. Ein nicht unterstuetzter Fall ist ein
   **Fehlschlag** — der Harness filtert nichts.

---

# PLAN — Runde 4 (Haertetest 2): Speichermodell SPEC §3 + DOM-Prototyp mit Zyklen

Ziel dieser Runde ist **Abnahmepunkt 2** aus `ABNAHME.md`: das Speichermodell aus
`SPEC.md` §3 wirklich bauen (`Rc`/`Weak`, Opt-in-Tracing-GC, `#[no_gc]`) und mit
einem **DOM-Prototypen in Firn** belegen, dass zyklische Objektgraphen ohne Leck
getragen werden — gemessen, nicht behauptet.

Reihenfolge der Wichtigkeit, wenn die Zeit knapp wird:
**GC-Kern > `#[no_gc]` > DOM-Prototyp + Dauerlauf + Negativtest > `Rc`.**
Ein ehrlich gemessener Zehnminutenlauf schlaegt einen behaupteten 24-Stunden-Lauf.
Was nicht fertig wird, kommt als offener Punkt nach `SPEC.md` §14.1 — niemals als
Behauptung in `README.md`.

## 0. Was der Lead vor dieser Runde gebaut hat — steht, laeuft, nicht neu bauen

Zustand vor der Runde selbst gemessen: `bash test.sh` **PASS 485/485**, Exit 0,
`cargo build --release` **null Warnungen**, `cargo test --release` 122/122.

Neu im Baum (Skelett dieser Runde, schon gruen):

| Datei | Inhalt |
|---|---|
| `compiler/src/gc.rs` | **neu, leer bis auf den Vertrag.** Drei Abfragen, die andere Module benutzen duerfen: `ist_gc_alloc_aufruf(name) -> bool`, `ist_gc_zeiger(&Type) -> bool`. Liefern in der Skelettfassung `false`. Modul `gckern` fuellt die Datei. |
| `compiler/src/nogc.rs` | **neu, arbeitsfaehig.** Traegt `hat_no_gc(&FnDecl)` und `hook_check(ck, prog)`: laeuft ueber alle `#[no_gc]`-Funktionen, prueft **Regel 2** (Aufruf einer Funktion ohne `#[no_gc]`) vollstaendig und **Regel 1/3** ueber die zwei Abfragen aus `gc.rs`. Modul `nogc` haertet die Datei. |
| `compiler/src/sema.rs` | genau **eine** Zeile eingefuegt: `// HOOK nogc` + `crate::nogc::hook_check(self, prog)` in `Checker::run`, nach `add_items_inner`, vor `check_main`. |
| `compiler/src/main.rs` | `mod gc;` und `mod nogc;` eingetragen. |
| `lib/gc/`, `lib/rc/`, `lib/dom/`, `tools/dom_soak/`, `docs/berichte/` | leere Verzeichnisse fuer die Module dieser Runde. |

**Damit muss kein Modul mehr `sema.rs` oder `main.rs` anfassen, um seinen Hook
zu bekommen** — das war der Zweck des Skeletts.

## 1. Dateieigentum in Runde 4 — zwei Module fassen NIE dieselbe Datei an

| Modul | Darf schreiben | Darf **nicht** anfassen |
|---|---|---|
| **gckern** | `compiler/src/gc.rs`, `compiler/src/gc_lower.rs` (neu), `lib/gc/*.fi`, und als **einziges Modul** die bestehenden Compilerdateien `parser.rs`, `lexer.rs`, `ast.rs`, `sema.rs`, `sema_generic.rs`, `types.rs`, `layout.rs`, `lower.rs`, `lower_errors.rs`, `errors.rs`, `mono.rs`, `modules.rs`, `abi.rs`, `codegen_x86.rs`, `fir.rs`, `opt.rs`, `mem2reg.rs`, `regalloc.rs`, `main.rs`; `tests/50*_gc_*.fi` … `tests/53*_gc_*.fi`, `tests/neg/gc_*.fi`; `docs/GC.md`, `docs/berichte/gckern.md` | `compiler/src/nogc.rs`, `compiler/src/attrs.rs`, `lib/rc/`, `lib/dom/`, `tools/`, `test.sh`, `SPEC.md`, `ABNAHME.md`, `README.md` |
| **nogc** | `compiler/src/nogc.rs`, `compiler/src/attrs.rs`, `lib/html/*.fi`, `tests/54*_no_gc_*.fi`, `tests/neg/nogc_*.fi`, `docs/berichte/nogc.md` | alle anderen Compilerdateien (Hook steht bereits), `lib/gc/`, `lib/dom/`, `test.sh`, Dokumente |
| **rclib** | `lib/rc/*`, `tests/modules/rc.fi`, `tests/55*_rc_*.fi`, `tests/neg/rc_*.fi`, `docs/RC.md`, `docs/berichte/rclib.md` | Compilerquellen, `lib/dom/`, `lib/gc/`, `test.sh`, Dokumente |
| **dom** | `lib/dom/*.fi`, `tests/modules/dom.fi` (Symlink), `tests/56*_dom_*.fi`, `docs/berichte/dom.md` | Compilerquellen, `lib/rc/`, `lib/gc/`, `tools/`, `test.sh`, Dokumente |
| **mess** | `tools/dom_soak/*`, `test.sh` (nur **Anhaengen** von Abschnitt 10), `ABNAHME.md`, `README.md`, `SPEC.md` §14.1, `RUN.md`, `PLAN.md`, `docs/berichte/mess.md` | jede Quelldatei in `compiler/`, `lib/` |

Neue `tests/*.fi` werden von `test.sh` automatisch eingesammelt — dafuer muss
niemand `test.sh` anfassen. **Nur `mess`** darf `test.sh` erweitern, und nur
durch Anhaengen eines neuen Abschnitts; bestehende Abschnitte bleiben
byte-identisch.

## 2. Die Sprachoberflaeche des GC — verbindlicher Vertrag

`gckern` setzt genau das um, `dom` schreibt genau dagegen. Abweichungen nur mit
Eintrag in `SPEC.md` §14.1 (durch `mess`, nach Bericht des Moduls).

### 2.1 Deklaration

```firn
gc class Node {
    eltern:      GcWeak[Node],
    erstes_kind: Gc[Node],
    naechstes:   Gc[Node],
    kennung:     u32,
}

gc class Element extends Node {
    tag:   u32,
    attrs: Gc[Attribut],
}
```

* `gc class` ist die einzige Art, GC-verwaltete Werte zu deklarieren. Ein
  `gc class`-Wert lebt **nur** auf dem GC-Heap: kein Wert auf dem Stapel, kein
  Feld eines `struct`, kein Rueckgabetyp — jeder Versuch ist ein Compilerfehler
  mit Zeile/Spalte.
* Erlaubte Feldtypen: skalare Typen (Ganzzahlen, `bool`, `*T`/`*mut T`),
  `Gc[T]`, `GcWeak[T]` und Arrays davon (`[u8; 32]`). Alles andere ist ein
  Compilerfehler. `GcVec`/`GcMap` aus SPEC §3.5.2 sind **nicht** Teil dieser
  Runde (Eintrag §14.1); Listen baut `dom` aus `Gc`-Feldern.
* `extends`: eine Basis, Basisfelder liegen **vorne** (Praefixlayout), damit die
  Aufwaertsumwandlung kostenlos ist. Feldnamen der Basis duerfen nicht erneut
  vergeben werden. Keine Mehrfachvererbung. `virtual`/`override` sind **nicht**
  Teil dieser Runde (§14.1) — die Sprache kennt in Stufe 0 keine Methoden.

### 2.2 Ausdruecke

| Form | Typ | Bedeutung |
|---|---|---|
| `gc Name{ f: v, … }` | `AllocError!Gc[Name]` | Allokation auf dem GC-Heap. **Alle** Felder muessen angegeben werden. Bei Erschoepfung: erst Sammellauf, dann `AllocError::OutOfMemory`. Ergebnis ist `#[must_consume]` (kommt von der Fehlerunion) — Verwerfen ist Compilerfehler. |
| `gc_null[Name]()` | `Gc[Name]` | Nullwert |
| `weak_null[Name]()` | `GcWeak[Name]` | Nullwert |
| `weak(g)` | `GcWeak[T]` | schwacher Verweis auf `g: Gc[T]`; haelt nicht am Leben |
| `stark(w)` | `Gc[T]` | Aufwertung; **Nullwert**, wenn das Ziel eingesammelt wurde |
| `g.feld` | Feldtyp | Lesen mit implizitem Deref, auch geerbte Felder |
| `g.feld = v` | — | Schreiben; die Einfuegebarriere sitzt genau hier |
| `g.as?[Element]` | `Gc[Element]` | geprueft abwaerts; Nullwert, wenn die Typkennung nicht in der Ahnenkette liegt. (`?Gc[T]` aus SPEC §4.4 ist in Stufe 0 der nullbare `Gc[T]` — §14.1) |
| `g == h`, `g != h` | `bool` | Identitaetsvergleich, auch gegen `gc_null[T]()` |
| `Gc[Element]` → `Gc[Node]` bei `let`/Zuweisung/Argument/`return` | — | kostenlose Aufwaertsumwandlung |

`AllocError { OutOfMemory }` wird von der GC-Laufzeit programmweit deklariert
(Fehlermengennamen sind programmweit, siehe `tests/414_modul_fehler.fi`) und ist
in jedem Programm verfuegbar, das den GC benutzt. `rclib` benutzt **dieselbe**
Menge; solange `gckern` sie noch nicht bereitstellt, deklariert `rclib` sie
selbst in seinem Modul und `mess` traegt am Ende ein, welche der beiden
Fassungen im Baum steht.

### 2.3 Sammler-Schnittstelle (`gc.stats()` in Stufe-0-Form, §14.1)

Aufrufbar ohne `import`, vom Compiler bereitgestellt:

| Aufruf | Typ | Bedeutung |
|---|---|---|
| `gc_init()` | `bool` | einmal als Erstes in `main`: Heap anlegen, Stapelboden merken. `false` = `mmap` fehlgeschlagen — der Aufrufer **muss** sichtbar scheitern. |
| `gc_collect()` | `u64` | erzwingt einen Sammellauf, liefert die Pausenzeit in ns |
| `gc_collections()` | `u64` | Anzahl Sammellaeufe |
| `gc_live_objects()` | `u64` | lebende Objekte nach dem letzten Lauf, **gezaehlt** |
| `gc_heap_bytes()` | `u64` | vom Sammler beim Betriebssystem geholte Bytes |
| `gc_live_bytes()` | `u64` | belegte Bytes der lebenden Objekte |
| `gc_pause_ns_last()`, `gc_pause_ns_max()`, `gc_pause_ns_total()` | `u64` | Pausenzeiten, echt gemessen (`clock_gettime`) |

Zusagen, die `gckern` einhaelt (SPEC §3.5.3):
Mark-Sweep, **praezise** Heap-Verfolgung ueber compilergenerierte `trace`-Tabellen
aus dem Feldlayout; **konservativer** Stapel- **und Register**-Scan (Register
werden vor dem Lauf auf den Stapel gerettet, sonst waere die Zusage falsch);
**kein Kompaktieren**; Groessenklassen-Allokator gegen Fragmentierung; Sammlung
**nur** an `gc Name{…}`-Stellen und bei `gc_collect()`; ein Heap pro Faden;
kein `MAP_FIXED`, keine feste Adresse, `mmap`-Fehler sichtbar.
Inkrementelles Sammeln (`S5`) und Finalisierer (`S4`) sind **nicht** Teil dieser
Runde und werden in §14.1 als offen gefuehrt.

Empfohlener Weg fuer die Laufzeit, damit es keine Modulpfad-Probleme gibt: die
Sammler-Laufzeit steht als **lesbares Firn** in `lib/gc/gc.fi` und wird vom
Compiler per `include_str!` eingebettet und als zusaetzliches Modul eingezogen,
sobald ein `gc class` im Programm vorkommt. Der Zustand des Sammlers liegt in
einem vom Codegenerator angelegten Datenblock; ein Intrinsic liefert dessen
Adresse (Stufe 0 hat keine globalen Variablen). Ein Programm braucht **kein**
`import` und keine zusaetzliche Kommandozeilenoption.

### 2.4 `#[no_gc]` (Modul `nogc`)

Verboten in einer `#[no_gc]`-Funktion, transitiv, Fehler mit Zeile/Spalte:
(i) GC-Allokation bzw. Aufruf einer Sammler-Funktion, (ii) Aufruf einer
Funktion ohne `#[no_gc]`, (iii) Schreiben in ein `Gc[T]`/`GcWeak[T]`-Feld.
`attrs.rs`: `no_gc` auf `umgesetzt: true` und den Test
`nur_must_consume_ist_umgesetzt` mitziehen (`vec!["must_consume", "no_gc"]`).
Nachweis: der vorhandene HTML5-Tokenizer in `lib/html/` wird mit `#[no_gc]`
markiert und uebersetzt weiter — die Quote in `tools/tokenizer/run.sh` darf
sich **nicht** verschlechtern (6.810/6.810 bzw. 6.809/6.810).

## 3. Der DOM-Prototyp (Modul `dom`) — Vertrag

`lib/dom/dom.fi` ist **eine** Moduldatei ohne eigene `import`-Zeilen (Importe
werden relativ zur Wurzeldatei aufgeloest; ein Symlink `tests/modules/dom.fi`
macht dasselbe Modul fuer `tests/*.fi` erreichbar, ohne den Code zu doppeln).
Wurzelprogramme liegen daneben und binden es mit `import dom` ein.

Diese Zyklenarten **muessen** wirklich vorkommen — ein Baum ohne Rueckverweise
zaehlt nicht:

1. `gc class Node` mit **Elternverweis UND Kinderliste**; mindestens eine
   Variante mit **starkem** Elternverweis (`Gc[Node]`), damit der Zyklus echt
   ist und nicht durch `GcWeak` wegdefiniert wird.
2. `gc class Element extends Node` mit Attributen (Atom-Kennung → Text).
3. **Listener als eigene Objekte**, die ihren Knoten halten, waehrend der Knoten
   den Listener haelt (Zyklus ueber zwei Objekte).
4. eine **live `HTMLCollection`**-artige Struktur, die ihren Wurzelknoten haelt.
5. ein **Observer ueber `GcWeak`**, der sein Ziel nicht am Leben haelt, mit Test,
   dass das Ziel wirklich eingesammelt wird und `stark(w)` danach den Nullwert
   liefert.
6. ein simulierter **JS-Wrapper**: Wrapper haelt Knoten, Knoten haelt Wrapper.

Pflichtfunktionen im Modul `dom` (Namen fest, `mess` und die Tests haengen dran):

```
fn dom_zyklus_bauen() -> AllocError!u64      // baut EINEN vollstaendigen Zyklus-Satz
                                             // (1..4 und 6), liefert die Zahl der
                                             // dabei angelegten Objekte
fn dom_zyklus_verwerfen()                    // laesst den letzten Satz unerreichbar werden
fn dom_selbsttest() -> i32                   // 0 = alle Zyklenarten geprueft, sonst Fehlercode
```

## 4. Dauerlauf und Messung (Modul `mess`, Programme von `dom`)

`dom` liefert zwei **Wurzelprogramme** mit identischem Aufbau:
`lib/dom/soak_gc.fi` (GC-Fassung, darf **nicht** lecken) und
`lib/dom/soak_leck.fi` (absichtlich leckende Fassung: Zyklus ueber Zaehlverweise
statt `Gc`, selbst enthalten, **ohne** `import` von `lib/rc`).

Beide enthalten diese drei Zeilen woertlich, damit `tools/dom_soak/run.sh` sie
mit `sed` in eine Arbeitskopie umstellen kann:

```firn
const BUDGET_MS: i64 = 600000  // SOAK_BUDGET_MS
const ZYKLEN_MAX: i64 = 100000000  // SOAK_ZYKLEN_MAX
const STICHPROBE: i64 = 1000  // SOAK_STICHPROBE
```

Ausgabeprotokoll auf der Standardausgabe (TSV, verbindlich):

```
# firn-dom-soak v1 variante=gc
# spalten: t_ms  zyklen  rss_kib  lebende  sammellaeufe  heap_bytes  pause_max_ns
0       0       1234    0       0       0       0
150     1000    1560    412     3       262144  180000
…
# fertig zyklen=123456 t_ms=600001 sammellaeufe=987 rss_kib=1560
```

* Eine Datenzeile je `STICHPROBE` Zyklen, Felder mit Tabulator getrennt.
* `rss_kib` kommt aus `/proc/self/statm` (Feld 2 × Seitengroesse), `lebende`,
  `sammellaeufe`, `heap_bytes`, `pause_max_ns` aus den `gc_*`-Aufrufen. Die
  Leckfassung darf `0` in den GC-Spalten schreiben, `rss_kib` aber **nie**.
* Abbruch, wenn `t_ms >= BUDGET_MS` oder `zyklen >= ZYKLEN_MAX`. Exit 0 nur bei
  vollstaendigem Lauf; jeder Fehler (auch `mmap`) endet mit Exit != 0 und einer
  Zeile `# fehler …`.

`tools/dom_soak/run.sh` (Modul `mess`):
1. baut beide Programme in **allen drei Baustufen** (`opt`, `--no-opt`,
   `--opt-level=dev-fast`) und prueft, dass der Kurzlauf ueberall dasselbe
   Ergebnis liefert;
2. faehrt die GC-Fassung mit `SOAK_SEK` (Standard 600 s, in `test.sh` deutlich
   kuerzer) und mindestens 100.000 Zyklen;
3. faehrt die Leckfassung im **gleichen** Aufbau;
4. wertet aus: Aufwaermphase = erstes Viertel der Stichproben, danach Vergleich
   der Mediane des zweiten und des letzten Viertels fuer `rss_kib` und
   `lebende`. **Urteil:** GC-Fassung `PASS`, wenn kein monotoner Anstieg
   (Schwelle in der Datei dokumentiert, z. B. < 5 % und keine durchgehend
   steigende Folge); Leckfassung muss **anschlagen** — bleibt sie gruen, endet
   `run.sh` mit Exit != 0 und der Meldung, dass die Messung nichts taugt;
5. schreibt die Messreihe als Tabelle nach `tools/dom_soak/messung-*.tsv` und
   eine Zusammenfassung auf die Standardausgabe;
6. Exit 0 nur, wenn (4) fuer beide Fassungen das erwartete Urteil liefert.

`mess` traegt danach in `ABNAHME.md` Punkt 2 die **echten** Werte ein: Laufzeit,
Zyklenzahl, RSS-Verlauf, Sammellaeufe, lebende Objekte, dazu ausdruecklich den
Satz, dass der 24-Stunden-Lauf aus der Abnahme **noch aussteht**, und was nicht
gebaut wurde (inkrementelles Sammeln, `virtual`, `GcVec`/`GcMap`,
Finalisierer).

## 5. `Rc[T]`/`Weak[T]` (Modul `rclib`) — Vertrag

Reines Firn, keine Compileraenderung. Implementierung genau einmal im Baum:
`tests/modules/rc.fi` (Modul `rc`), erreichbar aus `tests/*.fi` per
`import modules.rc`; `lib/rc/rc.fi` ist ein **Symlink** darauf, damit der
Bibliothekspfad existiert, ohne Code zu doppeln.

* `Rc[T]` ist **immer unveraenderlich** — keine Innenveraenderlichkeit, kein
  `RefCell`-Aequivalent. Lesende Zugriffsfunktion, keine schreibende.
* Fehlbare Allokation: `rc_neu[T](inout alloc, wert) -> AllocError!Rc[T]`
  (Stufe 0 kennt keine Methoden — `Rc[T].neu` aus SPEC §3.4 wird als Funktion
  geschrieben, Eintrag §14.1).
* `weak_von`/`aufwerten` fuer `Weak[T]`, `rc_klonen`, `rc_freigeben`.
* `Arc[T]` (atomarer Zaehler) darf entfallen, wenn die Zeit knapp ist — dann
  ehrlich in §14.1.
* **Pflichttest:** ein `Rc`-Zyklus **leckt** und das wird sichtbar gemacht
  (Test, der belegt, dass der Zaehler nie 0 wird und der Speicher gehalten
  bleibt) plus ein Satz in `docs/RC.md`, dass das so gewollt ist.

## 6. Reihenfolge und Abhaengigkeiten

* `gckern` legt sofort los und ist der kritische Pfad. Es liefert **zuerst** die
  Sprachoberflaeche aus §2 in kleinster tragfaehiger Form (Deklaration,
  Allokation, Feldzugriff, Sammellauf, Statistik) und **erst danach** `extends`
  und `as?`.
* `nogc` ist unabhaengig: Regel 2 laesst sich vollstaendig ohne `gckern` bauen
  und testen; Regeln 1 und 3 werden ueber die zwei Abfragen aus `gc.rs` scharf,
  sobald `gckern` sie fuellt. Die `lib/html/`-Markierung geht sofort.
* `rclib` ist vollstaendig unabhaengig — die Rueckfallposition der Runde.
* `dom` schreibt gegen §2 und prueft laufend gegen den jeweils gebauten Stand.
  Wenn `gckern` in Teilen nicht fertig wird, liefert `dom` trotzdem den
  Selbsttest, die Leckfassung und den Bericht — und schreibt ehrlich, was nicht
  uebersetzt.
* `mess` baut `run.sh` gegen das Protokoll aus §4 und kann es lange vor `dom`
  fertig haben (Probe mit einer selbst geschriebenen TSV-Datei). Die Zahlen in
  `ABNAHME.md`/`README.md` werden **zuletzt** selbst gemessen.

## 7. Nicht verhandelbar

1. `bash test.sh` bleibt gruen, alle Abschnitte, 485 Tests als Untergrenze.
   Kein Test wird entfernt, umgeschrieben oder abgeschwaecht.
2. `cargo build --release` **ohne Warnungen**; keine externen Kisten; kein
   `#![allow(...)]`, kein `#[allow(dead_code)]` auf nicht verdrahteten Feldern,
   kein `todo!()`/`unimplemented!()`. Nicht Umgesetztes meldet einen sauberen
   Compilerfehler mit Zeile/Spalte.
3. Jedes neue `tests/*.fi` laeuft in drei Baustufen mit demselben Ergebnis.
4. Keine feste Speicheradresse, kein `MAP_FIXED`; fehlgeschlagenes `mmap`
   scheitert sichtbar.
5. `SPEC.md` wird nicht umgeschrieben — Abweichungen kommen nach §14.1.
6. Jedes Modul schreibt seinen Bericht nach `docs/berichte/<modul>.md`:
   was gebaut, was gemessen (echte Ausgaben), was offen. `mess` faltet das in
   `ABNAHME.md` und `README.md` zusammen.
7. Ein GC, der im Test nicht nachweislich sammelt, ist wertlos. Jeder
   GC-Testfall belegt seine Behauptung mit `gc_collections()` und
   `gc_live_objects()`.

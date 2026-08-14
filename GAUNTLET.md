# Gauntlet-Log — firn
**Ziel:** HÄRTETEST 1 für die Sprache Firn: einen HTML5-Tokenizer IN FIRN schreiben und gegen die offizielle html5lib-Testsuite messen. Dazu die Fehlerunionen als Sprachmittel bauen, weil ein Tokenizer ohne Fehlerbehandlung nicht ehrlich ist.

=== AUSGANGSLAGE — ZUERST LESEN ===
Der Compiler firnc läuft und ist reif genug für diese Aufgabe. `bash test.sh` meldet aktuell **PASS 393/393**, `cargo build --release --manifest-path compiler/Cargo.toml` läuft mit NULL Warnungen. Lest, bevor ihr etwas anfasst:
- README.md — was die Sprache HEUTE kann, mit echten Beispielen. Besonders: Sprachtour, Zeichenketten, Attribute, Baustufen.
- SPEC.md — §5.1 Fehlerunionen (das Ziel), §8 Zeichenketten, §14 was Stufe 0 kann, §14.1/§14.2 die bewussten Einschränkungen.
- ABNAHME.md — die sechs Abnahmepunkte. Punkt 3 (Tokenizer) steht bei **0 von 6.810 (0,0 %)**. Diese Runde soll daraus eine echte Zahl machen.
- DESIGNZIELE.md §2 (fehlbare Allokation) und §10 (welche Fundamente stehen).
- docs/FIR.md, docs/SELBSTHOSTING.md.
NUR LESEN: ../karstos-browser/TODO-FIRN.md Block 0 (Aufgabe 0.8), ../karstos-browser/FIRN-ANFORDERUNGEN.md §13 Punkt 3.

VORHANDEN und benutzbar — nicht neu bauen:
- **Modulsystem**: `import pfad.modul`, `export { … }`, mehrere .fi-Dateien zu einem Binary.
- **enum + match** mit Vollständigkeitsprüfung und echter Sprungtabelle (`tests/230_zustandsmaschine.fi` hat 32 Zustände). GENAU DAS BRAUCHT DER TOKENIZER.
- **Generics** per Monomorphisierung, `Vec[T]`, `Map[K,V]`.
- **Zeichenketten**: `Bytes`, `Str` (UTF-8), `Str16` (WTF-16, hält ungepaarte Surrogate), `Atom` (interniert). Bibliothek unter `lib/str/`, Zahlen unter `lib/num/`.
- **Attribute**: `compiler/src/attrs.rs`, `firnc --list-attrs`. `#[must_consume]` ist umgesetzt.
- **Testdaten**: `testdata/html5lib-tokenizer/` — 14 `.test`-Dateien, **6.810 Testfälle**, Zählbefehl in `testdata/README.md`.
- **Referenz**: `cargo add html5ever` funktioniert (geprüft). Nur in einem getrennten `bench/`-Unterordner als Messlatte, NIEMALS als Abhängigkeit des Compilers.

=== AUFGABE 1: FEHLERUNIONEN `E!T` (SPEC §5.1) ===
Baut sie in einem EIGENEN Modul (z. B. `compiler/src/errors.rs` + `compiler/src/lower_errors.rs`), analog zu `sema_match.rs`/`lower_match.rs`. Das Hook-Muster ist im Parser bereits vorhanden (`// HOOK types`) — macht es genauso.
**Empfohlener Weg, spart euch die halbe Arbeit:** Stellt `E!T` als zweivariantige getaggte Union dar, genau wie `enum` es schon tut — ein Struct in `types::TypeCtx` mit `__err: u32` (0 = Erfolg) und `__val: T`, plus Seitentabelle wie `enum_by_struct`. Dann funktionieren Aggregatrückgabe, System-V-ABI, Registerzuteilung und Codegen SOFORT, ohne dass ihr sie anfasst.
Umfang:
  1) `error IoError { NotFound, Permission, Closed }` — Fehlermenge, Codes ab 1.
  2) Typsyntax `IoError!Buf` als **Rückgabetyp** und als Typ lokaler Variablen.
  3) Implizite Umwandlung bei `return`: `return wert` ergibt Erfolg, `return IoError::NotFound` ergibt Fehler. Kein `ok(...)`-Zeremoniell.
  4) `try ausdruck` — bei Fehler sofort mit demselben Code aus der Funktion zurück, sonst der Wert. Nur in Funktionen erlaubt, die selbst eine passende Fehlerunion liefern; sonst klarer Fehler mit Zeile/Spalte.
  5) `ausdruck catch ersatzwert` — Ersatzwert bei Fehler. Wenn ihr noch `catch |e| { … }` schafft: gern, aber Punkt 4 und 5 zuerst.
  6) Ein `!T`-Wert ist implizit `#[must_consume]`: als Anweisung verworfen = Fehler (die Prüfung dafür gibt es schon in `sema.rs`, `check_discard`).
  7) `defer { … }` und `errdefer { … }` wenn Zeit bleibt — sonst weglassen und in SPEC §14.1 als offen vermerken.
Mindestens 15 Testprogramme unter `tests/` plus 6 Negativtests (`try` außerhalb einer Fehlerfunktion, verworfenes `!T`, unbekannte Fehlervariante, Typfehler beim `catch`-Ersatzwert, doppelte Fehlervariante, Fehlermenge stimmt nicht überein).

=== AUFGABE 2: HTML5-TOKENIZER IN FIRN (der eigentliche Härtetest) ===
Ein Tokenizer nach dem WHATWG-HTML-Standard, **geschrieben in Firn** (`.fi`), unter `lib/html/`. Der Testtreiber darf Rust oder Python sein (Werkbank, kein Produkt) — der Tokenizer selbst muss Firn sein.
- Zustände als `enum` + `match` mit Sprungtabelle. Fangt mit den Kernzuständen an: Data, TagOpen, EndTagOpen, TagName, BeforeAttributeName, AttributeName, AfterAttributeName, BeforeAttributeValue, AttributeValue(Double/Single/Unquoted), AfterAttributeValueQuoted, SelfClosingStartTag, BogusComment, MarkupDeclarationOpen, CommentStart, Comment, CommentEnd, Doctype-Zustände, RCDATA, RAWTEXT, ScriptData, CharacterReference.
- Ausgabe: Token-Strom (DOCTYPE, StartTag mit Attributen und self-closing-Flag, EndTag, Comment, Character, EOF) im html5lib-JSON-Format, damit der Vergleich maschinell läuft.
- **ACHTUNG beim Harness** — hier wird oft geschummelt, tut es nicht: `"doubleEscaped": true` heißt, `input` UND `output` müssen zusätzlich \uXXXX-entschlüsselt werden. Einige Dateien nutzen den Schlüssel `"xmlViolationTests"` statt `"tests"`. `initialStates` und `lastStartTag` müssen beachtet werden. Fälle, die ihr nicht unterstützt, zählen als **FEHLSCHLAG**, niemals als Erfolg und niemals als „übersprungen".
- **Ergebnis: exakte Zahl bestandener von 6.810**, aufgeschlüsselt pro `.test`-Datei, in `ABNAHME.md` und `README.md`. Eine ehrliche Quote wie „2.145 / 6.810 (31,5 %) — umgesetzt sind die Zustände X, Y, Z, nicht umgesetzt A, B" ist das GEWÜNSCHTE Ergebnis. Eine geschönte Zahl ist wertlos.
- **Geschwindigkeit**: Durchsatz in MB/s auf einem Eingabekorpus, daneben `html5ever` auf demselben Korpus, Faktor ehrlich ausweisen. Zielwert laut Abnahme ist ≤ 2×; wird er verfehlt, wird der echte Faktor dokumentiert.
- Skript `tools/tokenizer/run.sh`, das alles baut, fährt und die Bilanz ausgibt. Einbinden als neuer Abschnitt in `test.sh`.

=== HARTE REGELN ===
- **Nichts Bestehendes kaputtmachen.** `bash test.sh` muss weiter grün sein, inklusive der acht vorhandenen Abschnitte: 393 Tests, Optimierer-Nachweis, Ergebnisort-Garantie, Architekturwächter Feldzugriff↔Speicherort, Symbolschema. Tests entfernen oder abschwächen, um grün zu werden, ist Betrug.
- **NULL Compilerwarnungen** bei `cargo build --release`. Keine `#![allow(...)]`-Sammelunterdrückung.
- Keine externen Crates im Compiler. Kein LLVM, kein Cranelift, kein C-Compiler als Backend.
- Keine `todo!()`/`unimplemented!()` in den geforderten Pfaden. Nicht Umgesetztes meldet einen sauberen Compilerfehler mit Zeile/Spalte.
- Alle Testprogramme laufen weiterhin in DREI Baustufen (`opt`, `--no-opt`, `--opt-level=dev-fast`) und liefern überall dasselbe.
- **SPEC.md ist der Vertrag** und wird nicht umgeschrieben, um sich dem Code anzupassen — Abweichungen kommen nach §14.1.
- **ABNAHME.md am Ende mit den echten Zahlen aktualisieren**, besonders Punkt 3.
- Vor der Fertigmeldung SELBST `bash test.sh` und `bash tools/tokenizer/run.sh` laufen lassen und die echten Ausgaben in die Dokumente schreiben.
- Lieber Aufgabe 1 vollständig und Aufgabe 2 mit ehrlichen 30 % als beides halb und behauptet.
**Messlatte:** Die Jury führt SELBST aus: `cargo build --release --manifest-path compiler/Cargo.toml`, `bash test.sh`, `bash tools/tokenizer/run.sh`. Keine Zahl aus README.md oder ABNAHME.md zählt ohne eigene Reproduktion.

(a) TOKENIZER-QUOTE: Die Jury lässt den Harness selbst laufen und vergleicht mit der behaupteten Zahl. Gefordert ist eine exakte Quote aus **6.810** Fällen, aufgeschlüsselt pro .test-Datei. Prüfen — hier wird am ehesten geschummelt: Werden nicht unterstützte Fälle als FEHLSCHLAG gezählt oder heimlich übersprungen? Wird `doubleEscaped` wirklich behandelt? Werden `xmlViolationTests` mitgezählt? Werden `initialStates` und `lastStartTag` beachtet? Ein Harness, der Fälle stillschweigend filtert, ist Betrug und muss hart bestraft werden. Eine ehrliche 30-%-Quote schlägt eine unreproduzierbare 80-%-Quote deutlich.

(b) TOKENIZER IST FIRN: Der Tokenizer muss in `.fi` geschrieben sein. Steckt die eigentliche Zustandsmaschine in Rust oder Python, ist der Punkt null. Prüfen: Zeilenzahl der `.fi`-Dateien gegen die des Harness; wo liegt die Logik wirklich?

(c) FEHLERUNIONEN: Selbst Programme mit `error`, `E!T`, `try` und `catch` schreiben und übersetzen. Funktioniert die implizite Umwandlung bei `return`? Reicht `try` den Fehler wirklich nach oben durch (Exit-Code prüfen)? Liefert `catch` den Ersatzwert? Ist ein verworfenes `!T` ein Fehler? Melden die Negativfälle Zeile und Spalte, ohne dass der Compiler abstürzt (kein Rust-Panic)?

(d) KEINE REGRESSION: Laufen die 393 Tests aus dem Ausgangsstand weiter durch, in allen drei Baustufen? Sind die vier Architekturnachweise (Optimierer, Ergebnisort, Feldzugriff↔Speicherort, Symbolschema) noch grün? Wurden Tests entfernt, umgeschrieben oder abgeschwächt? `git log` und `git diff` gegen den Ausgangsstand ansehen. Abgeschwächte Tests sind ein schwerer Mangel.

(e) GESCHWINDIGKEIT: Ist der Vergleich gegen html5ever ehrlich gemessen und dokumentiert, auch bei Verfehlung? Selbst nachmessen. Wird derselbe Eingabekorpus benutzt? Wurde html5ever mit `--release` gebaut? Ein dokumentiertes „6× langsamer" ist ein gutes Ergebnis dieser Kategorie; ein fehlender oder frisierter Vergleich ist ein schlechtes.

(f) SAUBERKEIT: `cargo build --release` selbst ausführen und auf NULL Warnungen prüfen. `grep` auf `todo!()`/`unimplemented!()`. Cargo.toml des Compilers auf externe Crates. Keine `#![allow]`-Sammelunterdrückung.

(g) EHRLICHKEIT DER DOKUMENTE: Deckt sich SPEC.md mit dem Implementierten? Wurde SPEC.md umgeschrieben, statt Abweichungen in §14.1 zu vermerken — massiver Abzug. Ist ABNAHME.md Punkt 3 mit der ECHTEN Quote aktualisiert? Behauptet README etwas, das die Jury nicht reproduzieren kann — massiver Abzug.

GRUNDREGEL: Bewertet wird ausschließlich, was die Jury selbst zum Laufen bringt und selbst nachmisst. Ein kleinerer, ehrlich gemessener Fortschritt wird deutlich besser bewertet als ein großer mit Behauptungen. Ehrlich als offen geführte Lücken kosten wenig; behauptete Fähigkeiten, die nicht funktionieren, kosten sehr viel.
**Bester Score:** 86/100 (Runde 1)
**Agenten:** 13 · **Dauer:** 6820s
**Runden-Snapshots:** je Runde ein Git-Commit + Tag (gauntlet-r<N>-score<S>). Bester Stand: Branch `gauntlet-best` (Runde 1) — Wechsel mit `git checkout gauntlet-best`, zurueck mit `git checkout -`.
**Runde 0 — Architektur**: 3 Module (teil-1, teil-2, teil-3) — bestehendes Projekt
**Runde 1** — Score 86/100 (Ziel 88)
  Maengel: • Der Tokenizer erzeugt KEINE Parse-Fehler-Codes; harness.py:60 wirft 'ParseError' aus Erwartung UND Ist-Ausgabe heraus. Die 6807/6810 sind damit nur Token-Konformitaet, nicht html5lib-Konformitaet — ehrlich vermerkt (ABNAHME.md:114-116), aber die Quote ist optimistischer als eine strenge Auswertung.  • lib/html/entities.fi:41-102: die Namenstabelle liegt an der FESTEN Adresse 0x600000000000 per mmap(MAP_FIXED_NOREPLACE). Auf Kerneln < 4.17 wird das Flag ignoriert (fremde Mappings werden ueberschrieben); ist die Adresse belegt und die Kennung passt nicht, liefert tabelle() 0 und nachschlagen() gibt still 0 zurueck -> alle benannten Referenzen fallen stumm auf '&' zurueck statt zu scheitern.  • ABNAHME.md:208-210 ('Keine Regression … PASS 259/259, 114 Programme, 30 Negativtests') widerspricht README.md:89 (PASS 468/468, 139 Programme, 46 Negativtests) und PLAN.md:356 (Ausgangsstand 393/393). Punkt g der Messlatte verlangt eine aktuelle Regressionszahl im Abnahmedokument.  • tools/tokenizer/korpus.py:29-31 filtert fuer den Durchsatzkorpus alle Faelle mit doubleEscaped oder initialStates heraus und haengt den Rest 4 MB lang aneinander. Gemessen wird also ein Korpus aus Pathologie-Schnipseln, nicht HTML; die 2,79x sind fuer echte Seiten nicht aussagekraeftig (steht so nicht in README/ABNAHME).  • compiler/src/fir.rs:168,172,176,378 und abi.rs:32 tragen #[allow(dead_code)] mit dem Kommentar 'wird vom Modul ct verdrahtet' — die Null-Warnungs-Aussage stuetzt sich teilweise auf Unterdrueckung nicht verdrahteter Felder (fir::Func::secret/constant_time, ohne Frontend nicht ausloesbar; ABNAHME.md:201 raeumt das ein).
**Runde 2** — Score n/a/100 (Ziel 88)
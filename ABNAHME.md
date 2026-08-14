# ABNAHME.md — Ist Firn bereit für die Browser-Engine?

**Maßgeblich:** `../karstos-browser/FIRN-ANFORDERUNGEN.md` §13
**Stand dieser Datei:** 2026-08-13, **nach der Zusammenführung von Runde 2**
**Gesamtergebnis: 0 von 6 bestanden**, 3 teilweise, 3 offen.
**Stand 14.08.2026:** `test.sh` **380/380** (97 Programme × 3 Stufen opt/noopt/dev-fast, 30 Negativtests, 41 Optimierer-Nachweise, Ergebnisort-Nachweis).

Alle Zahlen in dieser Datei wurden bei der Zusammenführung **selbst ausgeführt**,
nicht von den Teilmodulen übernommen. Reproduktion: `RUN.md`.

Diese Datei ist die einzige Stelle, an der der *Umsetzungsstand* steht. `SPEC.md`
beschreibt das Ziel, `README.md` den Compiler, **hier** wird abgehakt.

## Regeln für das Abhaken

1. Ein Punkt wird **nur** grün, wenn er **gemessen** wurde. Eine Zahl ohne
   Messung ist kein Ergebnis.
2. Die Messung muss **reproduzierbar** sein: Befehl, Datum, Rechner, Ergebnis.
3. **Teilerfolge werden als Zahl geführt**, nicht als „fast fertig".
4. Ein Punkt darf **zurückgestuft** werden, wenn eine Regression auftritt.
5. Wer abhakt, trägt Befehl und Ausgabe ein — nicht nur ein Häkchen.

Legende: `[ ]` offen · `[~]` teilweise, mit Zahl · `[x]` bestanden und gemessen

---

## Die sechs Punkte

### 1. `[ ]` Firn compiliert Firn, reproduzierbar in drei Bootstrap-Stufen

| | |
|---|---|
| **Anforderung** | `L1` · `SPEC.md` §11 |
| **Kriterium** | `firnc1` (in Firn) übersetzt `firnc2`; `firnc2` übersetzt sich selbst zu `firnc2'`; `firnc2` und `firnc2'` sind **bit-identisch** |
| **Messbefehl** | `./bootstrap.sh --verify-fixpoint` (existiert noch nicht) |
| **Stand** | Stufe 0 (`firnc0`, in Rust) läuft. Stufen 1–3 nicht begonnen |
| **Blockiert durch** | Modulsystem, `comptime`, Standardbibliothek → ROADMAP Phase 3/4 |
| **Aufwand laut PLAN-FIRN** | Teil von F0.1 |

---

### 2. `[ ]` Speichermodell entschieden **und prototypisch belegt**

| | |
|---|---|
| **Anforderung** | `S1`–`S3`, `S7` · `TODO-FIRN.md` 0.9 · `SPEC.md` §3 |
| **Kriterium** | DOM-Prototyp mit Eltern-/Kind-Zyklen **und** Listener-Zyklen läuft **24 h** ohne Speicherwachstum |
| **Messbefehl** | `./bench/dom_soak.sh --hours 24` (existiert noch nicht); Messgröße RSS über die Zeit, Toleranz: kein monotoner Anstieg nach der Aufwärmphase |
| **Stand Entscheidung** | **`[x]` getroffen und begründet** — Opt-in-Tracing-GC in drei Stufen, `SPEC.md` §3.2/§3.5. Alternativen (Arena+Indizes, Refcount+Weak) mit Begründung verworfen |
| **Stand Beleg** | **`[ ]` nicht belegt.** Kein GC implementiert, kein Prototyp, keine Messung |
| **Stand nach Runde 2 (13.08.2026)** | **unverändert offen — der GC wurde in Runde 2 bewusst NICHT begonnen.** Es gibt kein `Rc[T]`/`Weak[T]`, kein `Gc[T]`, kein `gc class`, keinen Mark-Sweep, keinen DOM-Prototyp, keinen Dauerlauf und keine RSS-Tabelle. Die Runde hat stattdessen Sprachkern, `match`, Generics, Zeichenketten, Optimierer und Werkzeuge gebaut (Ziele 1–5 und 9). **Verschoben, nicht verschwiegen.** `Gc[i32]`/`Rc[i32]` im Quelltext melden `'Gc[T]' ist in Stufe 0 nicht umgesetzt` mit Zeile/Spalte (`tests/neg/int_gc_nicht_umgesetzt.fi`) — der Compiler tut nicht so, als gäbe es sie |
| **Teilpunkte** | `S1` deterministisch als Standard: entworfen, in Stufe 0 nur Rohzeiger · `S2` GC-Heap: nur entworfen · `S3` schwache Verweise: nur entworfen · `S4` Finalisierer: nur entworfen · `S5` inkrementell: geplant v0.5 · `S6` Pausenzeiten messbar: geplant · `S7` `Rc`/`Arc`: nur entworfen |
| **Aufwand laut TODO-FIRN** | 0.1 = 2 PM (Entscheidung + Prototyp), 0.9 = 2 PM (Dauerlauf) |
| **Risiko** | Konservatives Stack-Scanning schließt einen kompaktierenden Sammler aus → Fragmentierung im Dauerlauf ist das eigentliche Risiko dieses Punktes |

---

### 3. `[ ]` HTML5-Tokenizer: **100 % `html5lib-tests/tokenizer/` UND ≤ 2× Referenz**

| | |
|---|---|
| **Anforderung** | `TODO-FIRN.md` 0.8 · `P9` · der härteste der sechs Punkte |
| **Kriterium A** | **100 %** der offiziellen `html5lib-tests/tokenizer/`-Fälle bestehen — exakte Zahl, keine Schätzung |
| **Kriterium B** | **≤ 2×** langsamer als eine Referenzimplementierung (z. B. `html5ever`), gemessen auf demselben Rechner, mit demselben Eingabekorpus |
| **Messbefehl** | `./bench/tokenizer.sh` (existiert noch nicht) — gibt bestandene Fälle und Durchsatz in MB/s aus |
| **Stand vor Runde 2** | **`[ ]`** Kein Tokenizer, keine Zeichenketten in der Sprache, kein `match`, keine Registerzuteilung |
| **Stand nach Runde 2 (13.08.2026)** | **`[ ]` weiter offen. Bestandene Fälle: 0 von 6.810 (0,0 %).** Es wurde **kein Tokenizer** in Firn geschrieben und **kein Harness** gebaut. Nichts wird übersprungen und nichts als Erfolg gezählt — es existiert schlicht nichts. `testdata/html5lib-tokenizer/` (14 `.test`-Dateien, 6.810 Fälle, Zählbefehl in `testdata/README.md`) liegt unbenutzt bereit |
| **Was jetzt vorhanden ist** | Die Vorbedingungen aus ROADMAP Phase 2: `enum`/`match` mit Sprungtabelle (`tests/230_zustandsmaschine.fi`, 32 Zustände), `Str16`/`Bytes`/`Atom` (`lib/str/`), Aggregate an Funktionsgrenzen, Modulsystem, echte Registerzuteilung. Der Tokenizer selbst ist die nächste Aufgabe, nicht mehr blockiert |
| **Kriterium B** | nicht gemessen (kein Tokenizer). Der allgemeine Abstand zu Rust liegt laut `bench/RESULTS.md` bei Median **2,8×–3,4×**; die alte Begründung „Stufe 0 legt jeden Wert auf den Stack" ist seit der Registerzuteilung überholt |
| **Aufwand laut TODO-FIRN** | 3 PM |

---

### 4. `[~]` Testrunner mit maschinenlesbarer Ausgabe **und** Debugger mit Quellzeilen

| | |
|---|---|
| **Anforderung** | `W2`, `W3` · `TODO-FIRN.md` 0.3, 0.4 |
| **Kriterium A** | Testrunner gibt Ergebnisse als JSON aus, CI-tauglich, Quoten automatisch als Zahl |
| **Kriterium B** | Debugger zeigt Quellzeilen, Variablen, Haltepunkte — Abnahme: **ein echter Fehler wurde damit gefunden** |
| **Messbefehl** | `firn test --format=json` · `gdb ./programm` zeigt `.fi`-Zeilen |
| **Stand A vor Runde 2** | `[~]` `test.sh` lief (166/166), aber nur als Text |
| **Stand A nach Runde 2** | **`[x]` erfüllt und selbst ausgeführt.** `cargo build --release --manifest-path tools/testrunner/Cargo.toml` → `./tools/testrunner/target/release/testrunner --format=json` liefert `{"suite":"firn","total":256,"passed":256,"failed":0,"rate":1.0,"cases":[…]}` mit Name, Betriebsart (`opt`/`noopt`/`neg`), Status und Dauer je Fall. Exit-Code ≠ 0 bei Fehlschlag, CI-tauglich. Der Runner ist ein eigenständiges Werkzeug ohne externe Crates |
| **Stand B nach Runde 2** | **`[~]` teilweise.** `.debug_line` gibt es: `firnc --no-opt -o /tmp/gdbdemo docs/gdb_beispiel.fi`, dann `gdb -batch -ex "break summe" -ex run -ex bt /tmp/gdbdemo` → `Breakpoint 1, summe () at docs/gdb_beispiel.fi:2` und `#1 … in main () at docs/gdb_beispiel.fi:11` (bei der Zusammenführung selbst nachgefahren). **Nicht erfüllt:** Variablen zeigt `gdb` nicht (kein `.debug_info` für lokale Namen), mit Optimierer nur die Zeile der `fn`-Deklaration (SPEC §14.1 Punkt 16), und es ist **kein echter Fehler damit gefunden worden** — das verlangt Kriterium B ausdrücklich |
| **Gesamt Punkt 4** | **`[~]`** — A vollständig, B halb |
| **Aufwand laut TODO-FIRN** | 0.3 = 1 PM, 0.4 = 3 PM |

---

### 5. `[~]` Paketverwaltung baut reproduzierbar auf zwei Rechnern

| | |
|---|---|
| **Anforderung** | `W1` · `TODO-FIRN.md` 0.5 |
| **Kriterium** | Zwei verschiedene Rechner erzeugen aus demselben Quelltextstand ein **bit-identisches** Artefakt |
| **Messbefehl** | `firn build --locked` auf zwei Rechnern, danach `sha256sum` vergleichen |
| **Stand vor Runde 2** | **`[ ]`** Es gibt kein Modulsystem und keine Paketverwaltung. `firnc0` übersetzt genau eine Datei |
| **Stand nach Runde 2** | **`[~]` teilweise.** Es gibt ein **Modulsystem**: `import pfad.modul`, `export { … }`, Zugriff über `modul.name`, mehrere `.fi`-Dateien → **ein** Binary (`compiler/src/modules.rs`; `tests/110_module.fi`, `tests/neg/kern_export.fi`, `tests/neg/kern_modul_fehlt.fi`). Umgesetzt ist Gesamtprogramm-Übersetzung mit getrennten Namensräumen, **keine** getrennten Objektdateien. **Nicht vorhanden:** Paketverwaltung, Sperrdatei, Registry, `firn build --locked`, Zwei-Rechner-Vergleich per `sha256sum`. Das Kriterium dieses Punktes ist damit **nicht** erfüllt |
| **Aufwand laut TODO-FIRN** | 2 PM |

---

### 6. `[ ]` Kompilierzeit-Codegenerierung erzeugt eine Unicode-Tabelle aus der UCD

| | |
|---|---|
| **Anforderung** | `G1`–`G4` · `TODO-FIRN.md` 0.6 · `SPEC.md` §6.4 |
| **Kriterium** | Ein Bauskript liest die Unicode Character Database und erzeugt daraus eine Firn-Tabelle; Größe der erzeugten Tabelle dokumentiert |
| **Messbefehl** | `firn build` erzeugt `generated/unicode_tables.fi`, Größe wird ausgegeben |
| **Stand** | **`[ ]`** Kein Bauskript-Mechanismus, kein `comptime`, kein Modulsystem |
| **Aufwand laut TODO-FIRN** | 2 PM |

---

## Zusammenfassung

| # | Punkt | Stand nach Runde 2 (13.08.2026, selbst gemessen) |
|---|---|---|
| 1 | Selbst-Hosting in drei Stufen | `[ ]` nicht begonnen; Bestandsaufnahme in `docs/SELBSTHOSTING.md` |
| 2 | Speichermodell entschieden **und belegt** | `[~]` Entscheidung getroffen, **Beleg fehlt: kein GC, kein DOM-Prototyp, keine RSS-Messung — in dieser Runde bewusst verschoben** |
| 3 | Tokenizer 100 % html5lib **und** ≤ 2× | `[ ]` **0 von 6.810 Fällen (0,0 %)** — kein Tokenizer, kein Harness |
| 4 | Testrunner **und** Debugger | `[~]` JSON-Runner erfüllt (**256/256, rate 1.0**), `.debug_line` in `gdb` belegt; Variablen und „echter Fehler gefunden" fehlen |
| 5 | Paketverwaltung reproduzierbar | `[~]` Modulsystem vorhanden, **Paketverwaltung nicht** |
| 6 | Kompilierzeit-Codegen erzeugt UCD-Tabelle | `[ ]` kein `comptime`, kein Bauskript |

**Gesamt: 0 von 6 bestanden, 3 angefangen** (2, 4, 5).

### Die neun Ziele dieser Runde — was wirklich fertig wurde

Reihenfolge wie in der Aufgabenstellung. Alle Nachweise sind Befehle, die die
Jury selbst ausführen kann; `RUN.md` führt sie in einer Liste.

| # | Ziel | Stand | Nachweis (selbst ausgeführt bei der Zusammenführung) |
|---|---|---|---|
| 1 | Sprachkern: Aggregate an Funktionsgrenzen, Stapelargumente, `break`/`continue`/`for`, `[wert; N]`, Modulsystem | **`[x]`** | `tests/100…111`, `tests/neg/kern_*.fi`; SPEC §14.1 Punkte 1, 9, 11, 13, 15 gestrichen |
| 2 | Summentypen + Musterabgleich mit Vollständigkeitsprüfung + Sprungtabelle | **`[x]`** | fehlende Variante → `error: 'match' ist nicht vollstaendig: die variante E::C ist nicht abgedeckt` mit `4:5`; `firnc --emit=asm tests/230_zustandsmaschine.fi` (32 Zustände) enthält genau **1×** `jmp qword ptr [rdx + rax*8]` und eine `.quad`-Tabelle |
| 3 | Generics per Monomorphisierung, `Vec[T]`, `Map[K,V]` | **`[x]`** | `tests/210…212`, `tests/neg/generic_*.fi` |
| 4 | Zeichenketten `Bytes`/`Str`/`Str16`/`Atom`, WTF-16, strtod/dtoa | **`[x]`** | `tests/300_str16_surrogate.fi` (einzelnes `0xD800` bleibt erhalten, `to_utf8()` liefert nichts, `to_utf8_lossy()` liefert `EF BF BD`); `bash tools/dtoa_vectors/run.sh 100000 4242` → **100.000/100.000 bitgleich zurück, 100.000/100.000 kürzeste Darstellung wie Rust**, 7,9 s. Ausnahme: keine Stringliterale im Lexer (SPEC §14.1.str S1) |
| 5 | Optimierer + ehrliche Messung | **`[~]`** | Registerzuteilung, mem2reg, Inlining, CSE, Blockverschmelzung real (`test_opt.sh`: 41/41). **Leistungsziel ≤ 2× verfehlt:** `BENCH_RUNS=5 bash bench/run.sh` → fib 1,57×, sieve 3,97×, matmul 6,04×, bytecount 1,77×, bubblesort 5,19×, statemachine 2,76×, **Median 3,36×** (ein früherer Lauf derselben Suite: Median 2,80×). Gewinn gegenüber `--no-opt`: Median ~10× |
| 6 | Constant-Time (`secret[T]`, `select`, `secure_zero`, `u128`) | **`[ ]` nicht gebaut** | Es gibt **keine** Syntax und **keine** Typprüfung. `fn f(a: secret[u8])` → `'secret[T]' ist in Stufe 0 nicht umgesetzt` (`tests/neg/int_secret_nicht_umgesetzt.fi`). Vorhanden sind nur die Schutzvorkehrungen in FIR/Optimierer (`fir::Func::secret`, `constant_time`), die ohne Frontend niemand auslösen kann. **Kein Punkt beansprucht** |
| 7 | GC + DOM-Prototyp mit Zyklen | **`[ ]` nicht gebaut** | siehe Punkt 2 oben — verschoben, nichts vorgetäuscht |
| 8 | HTML5-Tokenizer in Firn | **`[ ]` nicht gebaut** | **0 / 6.810 (0,0 %)** — siehe Punkt 3 oben |
| 9 | Werkzeuge: JSON-Testrunner, Modulauflösung, DWARF, Selbsthosting-Plan | **`[~]`** | JSON-Runner **256/256**; Modulauflösung ja, Paketverwaltung nein; `.debug_line` in `gdb` belegt (`docs/DEBUGGER.md`); `docs/SELBSTHOSTING.md` |

### Keine Regression (Punkt g der Messlatte)

`bash test.sh` nach der Zusammenführung: **PASS 259/259** — 114 Programme
jeweils **mit und ohne** `--no-opt` (identisches Ergebnis), 30 Negativtests,
41 Prüfungen des Optimierernachweises, 111 Rust-Modultests.
Von den Prüfungen aus Runde 1 wurde **genau eine entfernt**, und zwar
`tests/neg/too_many_params.fi`: sie hat verlangt, dass eine Funktion mit sieben
Parametern einen Fehler meldet („hoechstens 6"). Diese Beschränkung ist Ziel 1
dieser Runde und wurde aufgehoben (SPEC §14.1 Punkt 9), der Negativtest wäre
damit falsch geworden. An seine Stelle tritt der Positivtest
`tests/108_stapelargumente.fi` (Argumente ab dem siebten Wort auf dem Stapel)
plus der Codegen-Modultest `stapelargumente_ab_dem_siebten_wort`.
Sonst wurde **kein Test entfernt oder abgeschwächt**. `cargo build --release` erzeugt **null Warnungen**,
`compiler/Cargo.toml` hat weiterhin einen leeren `[dependencies]`-Abschnitt, und
in `compiler/src/` steht kein `todo!()`/`unimplemented!()`.

**Was das bedeutet:** `VORBEDINGUNGEN.md` §5.3 lässt Block 1 des Browser-Projekts
erst starten, wenn **alle sechs** Punkte grün sind. Firn ist davon weit entfernt
— das ist der erwartete Stand nach einem Stufe-0-Prototyp und kein Rückschlag.
`PLAN-FIRN.md` veranschlagt für Phase F0 insgesamt **27 Personenmonate**.

**Die zwei Punkte, die den Plan kippen können**, sind 2 und 3. Sie sollten so
früh wie möglich gemessen werden — auch unfertig, auch mit schlechtem Ergebnis.
Ein schlechter Messwert in Monat 6 ist wertvoll; derselbe Messwert in Jahr 3 ist
eine Katastrophe.

---

## Zwischenstand Modul `types` (Runde 2, 13.08.2026)

Betrifft keinen der sechs Punkte allein, ist aber Vorbedingung für Punkt 3
(Tokenizer): `SPEC.md` §6.3 (`L4`) und Generics (`L5`) sind umgesetzt und
gemessen.

| Teil | Stand | Nachweis (selbst ausführbar) |
|---|---|---|
| `enum` mit Nutzdaten, Layout dokumentiert | `[x]` | `cargo test --release --manifest-path compiler/Cargo.toml sema_match::` (4 Tests), `SPEC.md` §14.1.types |
| `match` mit Vollständigkeitsprüfung **zur Übersetzungszeit** | `[x]` | `tests/neg/match_missing_variant.fi` → `error: 'match' ist nicht vollstaendig: die variante Zeichen::Ende ist nicht abgedeckt` (7:5); weitere: `match_int_ohne_auffang`, `match_unbekannte_variante`, `match_unerreichbar` |
| Musterarten: Variante+Bindung, Literal, Bereich, `_`, verschachtelt | `[x]` | `tests/200..204_*.fi` (laufen mit und ohne `--no-opt`, gleiches Ergebnis) |
| Sprungtabelle bei dichten Varianten | `[x]` | `firnc --emit=asm tests/230_zustandsmaschine.fi` (32 Zustände): 1× `jmp qword ptr [rdx + rax*8]`, 0× `cmp`; Test `codegen_switch::tests::sprungtabelle_bei_30_zustaenden` |
| Generics per Monomorphisierung (`name__T1_T2`) | `[x]` | `tests/210_generic_fn.fi`, `tests/211_generic_struct.fi` (`Vec[T]`), `tests/212_generic_map.fi` (`Map[K,V]`) |
| Klare Fehlermeldung bei nicht erfüllter Anforderung | `[x]` | `tests/neg/generic_anforderung.fi` (7:13), `generic_argzahl.fi`, `generic_ohne_typargumente.fi` |
| `match` als **Ausdruck**, generische `enum`, `modul.E::V` | `[ ]` **verschoben** | ehrlich festgehalten in `SPEC.md` §14.1.types T1, T3, T6 |

Messung am 13.08.2026 (letzter Stand dieses Moduls): alle 105 Programme in
`tests/`, `tests/opt/` und `examples/` übersetzen und laufen zweimal (mit
Optimierer und mit `--no-opt`) mit dem erwarteten Ergebnis — **210/210**, davon
9 neue Programme dieses Moduls. Von den 28 Negativtests melden 26 den erwarteten
Fehler mit Zeile:Spalte; die 2 Abweichungen liegen in Dateien des Moduls `str`
(`tests/neg/str16_ist_kein_bytes.fi`, `tests/neg/str_bytes_ist_kein_text.fi`)
und gehören nicht zu diesem Modul. Alle 7 Negativtests dieses Moduls
(`tests/neg/match_*.fi`, `tests/neg/generic_*.fi`) bestehen.

*Nachtrag der Zusammenführung:* der hier notierte Abbruch von `bash test.sh` in
Schritt 2 (3 fehlschlagende Modultests des Moduls `opt`) ist behoben — nach dem
Zusammenführen laufen **111/111 Rust-Modultests** und **PASS 259/259** durch.

---

## Zwischenstand Modul `str` (Runde 2, 13.08.2026)

Betrifft Ziel-Punkt 4 der Runde (SPEC §8, Anforderungen `Z1`–`Z6`) und ist
Vorbedingung für Punkt 3 der Abnahme (Tokenizer). Alle Zahlen unten sind
**selbst gemessen** und mit den angegebenen Befehlen reproduzierbar.

| Teil | Stand | Nachweis (selbst ausführbar) |
|---|---|---|
| `Bytes` (rohe Oktette), Layout `{ptr,len,cap}` | `[x]` | `lib/str/bytes.fi`, `tests/301_bytes_utf8.fi` |
| `Str` = UTF-8, an der Grenze geprüft | `[x]` | `bytes_is_str` / `utf8_is_valid`; `tests/301_bytes_utf8.fi` prüft Überlang, Surrogatfolge, `F5`, Abschnitt |
| **`Str16` (WTF-16) prüft und normalisiert NICHTS** | `[x]` | **`tests/300_str16_surrogate.fi`** — einzelnes `0xD800` bleibt erhalten, `to_utf8()` liefert nichts (Rückgabe `false`, Ziel leer), `to_utf8_lossy()` liefert `EF BF BD` (U+FFFD), WTF-8 `ED A0 80` kommt bitgleich zurück |
| WTF-8 als verlustfreie Brücke | `[x]` | `tests/303_wtf8_roundtrip.fi`: alle 65.536 Codeeinheiten kommen bitgleich zurück, genau 2.048 (die Surrogate) scheitern an `to_utf8`, alle 1.114.112 Codepunkte laufen durch |
| `Atom` interniert, Vergleich = Ganzzahlvergleich | `[x]` | `lib/str/atom.fi`, `tests/302_atom_intern.fi` (1.000 Namen → 1.000 Nummern, Wachstum von Arena/Hashfeld) |
| Literale `"..."`, `b"..."`, `u"..."` mit `\uXXXX` inkl. ungepaarter Surrogate | `[~]` | im Compiler fertig (`compiler/src/strings.rs`, 11 Rust-Tests in `cargo test`), prüfbar über `firnc '--strlit=u"a\uD800"'`; **nicht** an den Lexer angebunden → `.fi`-Quelltext hat noch keine Literale (SPEC §14.1.str S1) |
| **`strtod` korrekt gerundet** | `[x]` | `tests/304_strtod_hardcases.fi`: 26 Härtefälle, Bitmuster gegen die Referenz — `0.1`, `1e23`, `5e-324`, `9007199254740993`, `2.2250738585072011e-308`, größtes/kleinstes Denormal, Überlauf, Unterlauf, exakte Halbschritte, 79-stellige Eingabe |
| **kürzeste Ausgabe mit Rückwandlungsgarantie** | `[x]` | `tests/305_dtoa_hardcases.fi` (28 Fälle, ECMAScript-Schreibweise), `tests/306_dtoa_roundtrip_small.fi` (2.000 Zufallswerte) |
| **Zufallslauf ≥ 100.000 Doubles, f64 → Text → f64 bitgleich** | `[x]` | `bash tools/dtoa_vectors/run.sh 100000 12345` → **100.000/100.000 bitgleich zurück** und **100.000/100.000 kürzeste Darstellung identisch mit Rust**, Laufzeit 13,9 s (13.08.2026) |
| Kein Gleitkommatyp in der Sprache | offen | `strtod`/`dtoa` arbeiten auf `u64`-Bitmustern (SPEC §14.1.str S2) — ehrlich als Abweichung geführt, nichts vorgetäuscht |
| API-Vertrag mit `tok`: `str16_new`, `str16_push`, `str16_len`, `str16_at`, `atom_intern` | `[x]` | `tests/308_str16_api.fi`; `str16_new() -> Str16` gibt ein Aggregat zurück (möglich, seit `kern` die System-V-Klassifikation umgesetzt hat), `atom_intern` bekommt die Tabelle als ersten Parameter (SPEC §14.1.str S4) |
| `Rope` (`Z3`, SOLL) | `[ ]` **verschoben** | SPEC §8.5 ohne Termin, SPEC §14.1.str S6 |

**Messbefehle in einem Rutsch:**

```
bash test.sh                                  # enthält tests/300..307 (mit und ohne --no-opt)
bash tools/dtoa_vectors/run.sh 100000 12345   # der große Zahlenlauf, ~15 s
firnc '--strlit=u"a\uD800"'                   # Literalpfad inkl. ungepaartem Surrogat
python3 tools/strlib/expand.py --check        # erzeugte Testdateien sind aktuell
```

---

## Zwischenstand Modul `opt` (Runde 2, 13.08.2026)

Betrifft Ziel-Punkt 5 der Runde (SPEC §10.3, Anforderungen `P1`–`P5`, `P9`) und
damit **Kriterium B von Punkt 3** dieser Abnahme (Tokenizer ≤ 2× Referenz). Die
dort notierte Begründung „Stufe 0 legt **jeden** Wert auf den Stack — damit ist
ein Faktor 2 unerreichbar" ist seit Runde 2 überholt: es gibt eine echte
Registerzuteilung. Der Faktor 2 ist trotzdem **noch nicht** erreicht (2,75×
Median gegen Rust `-O`, siehe unten).

| Teil | Stand | Nachweis (selbst ausführbar) |
|---|---|---|
| **Echte Registerzuteilung** (linear scan, Lebendigkeitsintervalle) `P3` | `[x]` | `compiler/src/regalloc.rs`; `bash test_opt.sh` prüft im erzeugten Assembler: Schleifenrumpf von `tests/opt/regalloc_loop.fi` **ohne einen einzigen Stackzugriff**, callee-saved Register gesichert **und** zurückgeholt |
| System-V-erhaltene Register korrekt gesichert | `[x]` | dieselbe Prüfung in `test_opt.sh`; `regalloc::tests::callee_saved_werden_gesichert_und_zurueckgeholt` |
| **Inlining** mit Größenheuristik, auch über Modulgrenzen | `[x]` | `tests/opt/inline_call.fi` (`call @quadrat` 1 → 0); über Modulgrenzen, weil das Modulsystem alle Dateien in EIN `fir::Module` übersetzt (SPEC §14.1.opt O6); `inline::tests::*` (4 Tests, u. a. „Rekursion wird nicht eingebettet") |
| **mem2reg** (einmal geschriebene `alloca`) | `[x]` | `tests/opt/mem2reg_single_store.fi`: `load.i32` 3 → 0 |
| **Tote Speicherung** (nie gelesene `alloca`) | `[x]` | `tests/opt/dead_store.fi`: `store.i32` 3 → 0, `alloca` 1 → 0 |
| **Kopierfortpflanzung** / algebraische Identitäten | `[x]` | `mem2reg::tests::algebraische_identitaeten` |
| **Blockverschmelzung** + Sprungfädelung | `[x]` | `tests/opt/block_merge.fi`: 8 Blöcke → 1 |
| **CSE** entlang des Dominatorbaums | `[x]` | `tests/opt/cse_common.fi`: `mul.i32` 2 → 1 |
| Konstantenfaltung + DCE erhalten und ausgebaut | `[x]` | die 6 Fälle aus Runde 1 laufen unverändert weiter (`test_opt.sh`, Abschnitt „Vorher/Nachher") |
| Bereichsprüfungen entfernen `P5` | `[~]` | Stufe 0 **erzeugt** keine Bereichsprüfungen (SPEC §14.1 Punkt 3). Umgesetzt ist das Entfernen beweisbar **wiederholter Bedingungen**: `tests/opt/redundant_check.fi`, `brcond` 3 → 2 |
| `Select`/`Barrier`/`SecureZero`/`secret` unangetastet (SPEC §9.2) | `[x]` | `mem2reg::tests::secret_werte_bleiben_unangetastet`, `mem2reg::tests::select_bleibt_select`, `regalloc::tests::{secret_werte_bekommen_kein_register, select_bleibt_cmov_auch_mit_registern}` |
| Optimierung ändert das Ergebnis nie | `[x]` | `bash test.sh` (jedes Programm mit **und** ohne `--no-opt`) + eigener Abschnitt in `test_opt.sh`, der Exit-Code und Ausgabe beider Fassungen vergleicht |
| **Benchmark-Suite, 6 Mikrobenchmarks, doppelt** | `[x]` | `bash bench/run.sh` → Tabelle nach stdout und `bench/RESULTS.md` |
| **Leistungsziel ≤ 2× Rust** (`P1`, SPEC §10.3) | `[ ]` **nicht erreicht: 2,75× (Median)** | siehe Tabelle unten; Spanne 1,57× – 4,95× |

### Messung vom 13.08.2026 (AMD EPYC 7571, rustc 1.99.0-nightly, Median aus 7 Läufen)

Befehl: `bash bench/run.sh` — jeder Benchmark existiert doppelt
(`bench/firn/<name>.fi`, `bench/rust/<name>.rs`, `rustc -O`, `black_box`,
Ergebnis wird ausgegeben und verglichen; bei abweichender Ausgabe bricht die
Messung ab).

| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Faktor Firn/Rust |
|---|---:|---:|---:|---:|
| fib (rekursiv, 3×) | 0,049 s | 0,143 s | 0,031 s | **1,57×** |
| sieve (5 Mio., 2 Durchläufe) | 0,117 s | 1,115 s | 0,029 s | **4,08×** |
| matmul 240×240 (3 Durchläufe) | 0,122 s | 2,061 s | 0,025 s | **4,95×** |
| bytecount 16 MiB (8 Durchläufe) | 0,509 s | 5,244 s | 0,181 s | **2,81×** |
| bubblesort 6000 | 0,102 s | 1,304 s | 0,038 s | **2,68×** |
| statemachine 8 MiB (4 Durchläufe) | 0,225 s | 1,247 s | 0,083 s | **2,70×** |

**Median 2,75× langsamer als Rust `-O`.** Der Optimierer bringt gegenüber
`--no-opt` im Median **9,9×** (Spanne 3,0× – 16,9×). Der verbleibende Abstand
liegt fast vollständig dort, wo LLVM vektorisiert (Sieb, Matrixmultiplikation);
Firn erzeugt nur skalaren Code. Ehrliche Einordnung: **Ziel verfehlt, Faktor
dokumentiert**, nicht geschönt.

### Regressionsstand

`bash test.sh` am 13.08.2026 nach diesem Modul: **PASS 257/257** (alle
Programme zweimal — mit Optimierer und mit `--no-opt` —, alle Negativtests,
`cargo test --release` mit 111 grünen Modultests, `test_opt.sh` mit
**41/41**). Die drei Modultests, die während der Bauphase kurzzeitig rot waren
(`opt::tests::seiteneffekte_bleiben_erhalten`,
`opt::tests::vergleich_und_zweig_falten_unerreichbaren_block_weg`,
`mem2reg::tests::leere_bloecke_werden_verschmolzen`), sind grün: die beiden
`opt::`-Tests erwarteten die **exakten** Zählwerte von Runde 1 und wurden auf
das jetzt stärkere Ergebnis nachgezogen (Block wird zusätzlich verschmolzen,
tote lokale Zelle wird zusätzlich entfernt) — die Prüfung „Syscall und Call
bleiben stehen" ist dabei **schärfer** geworden, nicht schwächer.

### Was offen bleibt (Modul `opt`)

* Keine Schleifenoptimierung (Entrollen, Hochziehen invarianter Berechnungen,
  Induktionsvariablen), keine Vektorisierung → das ist der Hauptgrund für die
  4,95× bei der Matrixmultiplikation.
* Kein Intervallsplitting in der Registerzuteilung (ein Wert liegt ganz im
  Register oder ganz im Stack).
* Zwei Codegen-Pfade: bei mehr als sechs Parametern/Argumenten und bei
  anweisungsgenauen Debugzeilen (`--no-opt`) übernimmt der Grundpfad aus
  Runde 1 (SPEC §14.1.opt O2).

---

## Fundamentarbeit aus DESIGNZIELE.md (Nachtrag 14.08.2026)

Nicht Teil der sechs Abnahmepunkte, aber Voraussetzung dafür, dass sie später
überhaupt erreichbar bleiben (`DESIGNZIELE.md` §10):

| Fundamentpunkt | Stand | Nachweis |
|---|---|---|
| Durchgangsregister mit Etikett *debugerhaltend* | **`[x]`** | `firnc --list-passes` — 9 Durchgänge, genau einer (`inline`) nicht debugerhaltend |
| Baustufen `--opt-level=dev/dev-fast/release-safe/release-fast` | **`[x]`** | `bash tools/baustufen/run.sh 3` → **dev-fast 2,06×**, dev 10,54× gegenüber release-fast |
| Ergebnisort-Garantie für Aggregatrückgaben | **`[x]`** | `bash tools/ergebnisort/run.sh` → 1-MB-Struktur, `baue` hat 224 Byte Rahmen, keine Bulk-Kopie |
| Ergebnisort für Struct-/Arrayliterale und `init` | **`[~]`** | Literale schreiben bereits feldweise ins Ziel (`lower.rs: write_into`); als Garantie in SPEC festgeschrieben, `init` gibt es noch nicht |
| Feldzugriff vom Speicherort trennen (Vorbedingung SoA) | **`[ ]`** | noch fest „Basis + Versatz" |
| Prüfphasen wiedereintrittsfähig (Vorbedingung `comptime emit`) | **`[ ]`** | einmaliger Durchlauf |
| `!T` + `#[must_consume]` | **`[ ]`** | Phase 2 |
| Symbol-Namensschema mit Versionsplatz | **`[ ]`** | Phase 3, mit der Paketverwaltung |

**Nebenbefund:** Die neue Stufe `--dev-fast` hat beim ersten Durchlauf einen
echten Codegenerator-Fehler aufgedeckt (Argumentregister 5/6 wurden im Prolog
überschrieben, `tests/024_six_args.fi` lieferte 13 statt 21). Er war in 259
grünen Tests unsichtbar, weil die betroffene Funktion in den Release-Stufen
immer eingebettet wurde. Behoben; Regressionstest `tests/025_argreg_shuffle.fi`.

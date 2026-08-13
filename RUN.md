# RUN.md — bauen, starten, selbst nachmessen

Alles hier ist **so ausgeführt worden**, wie es dasteht (13.08.2026, AMD EPYC
7571, Linux x86_64, rustc 1.99.0-nightly, binutils `as`/`ld`). Nur relative
Pfade, alles innerhalb dieses Ordners.

## 0. Voraussetzungen

* `cargo`/`rustc` (nur zum Bauen des Compilers und der Messlatten — der
  Compiler selbst hat **keine** externen Crates)
* GNU `as` und `ld` (Assembler und Linker, **kein** C-Compiler als Backend)
* `python3` (nur für die Werkbänke: Benchmarks, Einbinder)
* `gdb` (nur für den Debugger-Nachweis)

## 1. Compiler bauen

```sh
cargo build --release --manifest-path compiler/Cargo.toml
```

Erwartung: **null Warnungen**, Binary unter `compiler/target/release/firnc`.

## 2. Ein Programm übersetzen und ausführen

```sh
compiler/target/release/firnc -o /tmp/hello examples/hello.fi
/tmp/hello ; echo "exit=$?"
```

Weitere Betriebsarten:

```sh
firnc --no-opt -o /tmp/a datei.fi     # ohne Optimierer (gleiches Ergebnis!)
firnc --emit=asm datei.fi             # x86_64-Assembler (Intel-Syntax)
firnc --emit=fir datei.fi             # eigene IR, lesbar
firnc --help
```

Mehrere Dateien zu **einem** Binary (Modulsystem): die Wurzeldatei angeben,
`import pfad.modul` löst relativ zu ihrem Verzeichnis auf:

```sh
compiler/target/release/firnc -o /tmp/mod tests/110_module.fi
/tmp/mod ; echo "exit=$?"     # exit=60, so steht es in Zeile 1 der Datei
```

## 3. Die gesamte Testsuite

```sh
bash test.sh
```

Gemessenes Ergebnis dieses Stands: **PASS 259/259**
(114 Programme × 2 Durchläufe mit *und* ohne `--no-opt`, 30 Negativtests,
41 Prüfungen des Optimierernachweises, 111 Rust-Modultests).
Laufzeit ca. 1 Minute.

Maschinenlesbar (CI, Ziel 9 / ABNAHME Punkt 4 A):

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json > /tmp/firn.json
python3 -c "import json;d=json.load(open('/tmp/firn.json'));print(d['total'],d['passed'],d['failed'],d['rate'])"
# 256 256 0 1.0
```

(256 statt 259: der Runner enthält den Optimierernachweis `test_opt.sh` nicht.)

## 4. Die Nachweise einzeln — das, was die Jury prüft

| Was | Befehl | Gemessenes Ergebnis |
|---|---|---|
| **Vollständigkeitsprüfung `match`** | `firnc -o /tmp/m tests/neg/match_missing_variant.fi` | `error: 'match' ist nicht vollstaendig: die variante … ist nicht abgedeckt` **mit Zeile:Spalte**, Exit ≠ 0 |
| **Sprungtabelle bei 32 Zuständen** | `firnc --emit=asm -o /tmp/zm.s tests/230_zustandsmaschine.fi && grep -c "jmp qword ptr" /tmp/zm.s` | `1` — ein indirekter Sprung über eine `.quad`-Tabelle, keine Vergleichskette |
| **WTF-16, ungepaartes Surrogat** | `firnc -o /tmp/s tests/300_str16_surrogate.fi && /tmp/s` | `3 97 55296 98 0 0 5 97 239 191 189 98 5 97 237 160 128 98 1 55296` — `0xD800` bleibt erhalten, `to_utf8()` liefert nichts, `to_utf8_lossy()` liefert `EF BF BD` |
| **strtod/dtoa Härtefälle** | `firnc -o /tmp/h tests/304_strtod_hardcases.fi && /tmp/h` | 26 Bitmuster, beginnend mit `4591870180066957722` (= `0.1`); die Sollwerte stehen als `// expect_out:` in Zeile 1 derselben Datei |
| **100.000 Doubles hin und zurück** | `bash tools/dtoa_vectors/run.sh 100000 4242` | `OK: 100000/100000 bitgleich zurück, 100000/100000 kürzeste Darstellung wie Rust` (7,9 s) |
| **Benchmarks gegen Rust `-O`** | `BENCH_RUNS=5 bash bench/run.sh` | Median **3,36×** langsamer (Spanne 1,57×–6,04×), Tabelle in `bench/RESULTS.md`. **Ziel ≤ 2× verfehlt** |
| **Optimierer wirkt** | `bash test_opt.sh` | `PASS 41/41` (FIR vorher/nachher) |
| **Debugger zeigt `.fi`-Zeilen** | `firnc --no-opt -o /tmp/gdbdemo docs/gdb_beispiel.fi && gdb -batch -ex "break summe" -ex run -ex bt /tmp/gdbdemo` | `Breakpoint 1, summe () at docs/gdb_beispiel.fi:2` und `#1 … main () at docs/gdb_beispiel.fi:11` |
| **Erzeugte Str-Tests sind aktuell** | `python3 tools/strlib/expand.py --check` | `expand.py: 0 veraltete Dateien` |
| **Sauberkeit** | `grep -rn "todo!\|unimplemented!" compiler/src` | keine Treffer |

## 5. Was NICHT läuft, weil es nicht gebaut wurde

Ehrlich und vollständig (ausführlich in `ABNAHME.md`):

* **Constant-Time (`secret[T]`, `select`, `secure_zero`, `u128`)** — nicht
  umgesetzt. Prüfbar: `firnc -o /tmp/x tests/neg/int_secret_nicht_umgesetzt.fi`
  meldet `'secret[T]' ist in Stufe 0 nicht umgesetzt` mit Zeile/Spalte.
* **GC, `Rc`/`Gc`, DOM-Prototyp, RSS-Dauerlauf** — nicht umgesetzt. Prüfbar:
  `tests/neg/int_gc_nicht_umgesetzt.fi`.
* **HTML5-Tokenizer** — nicht geschrieben. Bestandene html5lib-Fälle:
  **0 von 6.810**. Es gibt keinen Harness; nichts wird stillschweigend
  übersprungen. Die Testdaten liegen in `testdata/html5lib-tokenizer/`
  (Zählbefehl in `testdata/README.md`).
* **Selbst-Hosting, Paketverwaltung, `comptime`/UCD-Tabelle** — offen,
  siehe `docs/SELBSTHOSTING.md` und `ABNAHME.md` Punkte 1, 5, 6.

## 6. Aufräumen

Alle Arbeitsverzeichnisse sind wegwerfbar und stehen in `.gitignore`:

```sh
rm -rf .test-work .opt-work .strwork .dtoa-work .testrunner-work bench/.work
```

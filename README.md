# Firn

Eigene systemnahe Programmiersprache mit eigenem Compiler. `firnc` liest
`.fi`-Quelltext und erzeugt echten Maschinencode für x86-64 und aarch64:
Lexer → Parser → Typprüfer → eigene IR (FIR) → Optimierer → eigener
Codegenerator → `as` → `ld`. Kein LLVM, kein Cranelift, kein C-Backend,
keine externen Crates, kein Parsergenerator. Der Compiler **hostet sich
selbst**: `firnc1` (in Firn geschrieben) übersetzt seinen eigenen
Quelltext, Stufe 2 und Stufe 3 sind zeichenidentisch.

**BEGONNEN AM:** 13.08.2026.

## Stand heute (10.09.2026)

- 697 Commits, aktueller Zweig `speed` (Optimierungsarbeit); neuster
  Arbeitszweig `fui` in `/root/firn-fui` (eigene Oberflächenbibliothek).
- Selbst-hostend, Fixpunkt bewiesen (`tools/fixpoint.sh`).
- Zwei Zielplattformen: x86-64 und aarch64, 296/301 Programme
  byte-identische Ausgabe.
- Sprache: Structs, Arrays, `enum`+`match` mit Exhaustiveness-Prüfung,
  Generics, Interfaces, Closures, Error-Unions `E!T`, `defer`/`errdefer`,
  `comptime`, `f32`/`f64`, `str` mit Interpolation, Threads, `extern fn`.
- Eigener Garbage Collector (opt-in, inkrementell, längste Pause 0,45 ms
  bei 120.000 lebenden Knoten).
- Werkzeuge: Formatter, DWARF-Debuginfo, Language Server, Paketsystem,
  Testläufer.
- Browser-Bausteine in Firn geschrieben: HTML-Tokenizer (100 % html5lib),
  CSS+Layout (1087/1087 Kästen exakt gegen Chromium), Malwerk, DOM+JS-Motor
  (test262: Parser 91,94 %, Motor 76,00 %), HTTP/1.1-Client. Diese Bausteine
  sind die Grundlage von Certus (`../certus-ui`).
- Kryptografie/Kompression (SHA-256, AES, DEFLATE) gegen OpenSSL/zlib/NIST
  geprüft.
- Kann ein eigenständiges Kernel-Objektfile erzeugen (`profile kernel`) —
  Grundlage für Osum (`../osum-merge11`).
- Geschwindigkeit: median 2,08–2,19× langsamer als `rustc -O` (sechs
  Mikrobenchmarks, Spanne 1,43×–4,16×).
- `bash test.sh`: zuletzt lief die ganze Suite mit `FAIL 6/1204`, davon
  2 real reproduzierbar (aarch64/Krypto), 4 Last-Flakes bei Parallelbetrieb
  (Einzelläufe bestanden).

Alle Zahlen mit dem Befehl dahinter: `docs/BENCHMARKS.md`.

## Was noch fehlt

- aarch64: 5 von 301 Programmen weichen ab (siehe Testlauf).
- Layout gegen die offiziellen Web Platform Tests: 59/186 (Chromium 141
  erreicht 138/186 auf demselben Korpus).
- Paint gegen WPT-Referenztests: 202/541 (37,34 %).
- DOM/JS gegen WPT-Subtests: 390/1714 (22,75 %), 169 Dateien deren Harness
  nie durchläuft.
- `https://` wird von Firns eigenem HTTP-Client zurückgewiesen — TLS ist
  eine eigene, noch offene Baustelle.
- Kryptografie/Kompression 1,38×–1,88× langsamer als OpenSSL/zlib.
- Zweig `fui` (Oberflächenbibliothek) noch nicht mit `speed` zusammengeführt.

## Bauen und testen

```sh
cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
./compiler/target/release/firnc -o /tmp/tour examples/tour.fi && /tmp/tour
```

```sh
bash test.sh    # baut den Compiler, testet jedes Programm auf drei
                 # Optimierungsstufen, dann rund vierzig Abschnittsnachweise
```

## Aufbau

| Pfad | Inhalt |
|---|---|
| `compiler/src/` | Stufe-0-Compiler in Rust, 57 Module, keine Abhängigkeiten |
| `lib/firnc1/` | derselbe Compiler in Firn (erreicht den Fixpunkt) |
| `lib/std/`, `lib/str/`, `lib/num/`, `lib/rt/`, `lib/gc/` | Standardbibliothek |
| `lib/html/`, `lib/css/`, `lib/dom/`, `lib/layout/`, `lib/js/` | Browser-Bausteine |
| `tests/`, `tests/opt/`, `tests/neg/` | Testprogramme |
| `bench/`, `tools/` | Benchmarks und Nachweisskripte |

## Dokumentation

| Datei | Inhalt |
|---|---|
| `SPEC.md` | Sprachspezifikation, Abschnitt 14.1 listet jede Implementierungsabweichung |
| `docs/BENCHMARKS.md` | jede Zahl mit dem Befehl, der sie erzeugt hat |
| `docs/FIR.md` | die IR: Befehle, Typen, Invarianten |
| `RUN.md` | alles bauen, alles laufen lassen, alles messen |
| `docs/ROUND*.md` | ein Bericht je Runde — Archiv, kein Handbuch |

## Lizenz

MIT — siehe `LICENSE`.

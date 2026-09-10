# Firn

Eigene systemnahe Programmiersprache mit eigenem Compiler. `firnc` liest
`.fi`-Quelltext und erzeugt echten Maschinencode für x86-64 und aarch64:
Lexer → Parser → Typprüfer → eigene IR (FIR) → Optimierer → eigener
Codegenerator → `as` → `ld`. Kein LLVM, kein Cranelift, kein C-Backend,
keine externen Crates, kein Parsergenerator. Der Compiler hostet sich
selbst: `firnc1` ist in Firn geschrieben, übersetzt seinen eigenen
Quelltext, und Stufe 2 und Stufe 3 sind zeichenidentisch.

**BEGONNEN AM:** 13.08.2026.

Dieser Baum steht auf dem Zweig **`speed`**.

## Stand heute (10.09.2026)

**Umfang:** 698 Commits auf `speed`.

**Sprache und Compiler**
- Selbst-hostend, Fixpunkt bewiesen (`tools/fixpoint.sh`).
- Zwei Zielarchitekturen: x86-64 und aarch64, gleicher Quelltext,
  296 von 301 Programmen mit byte-identischer Ausgabe.
- Structs, Arrays, `enum` + `match` mit Exhaustiveness-Prüfung, Generics,
  Interfaces, Closures und Funktionswerte, Error-Unions `E!T`,
  `defer`/`errdefer`, `comptime` + `emit`, `f32`/`f64`, `str` mit
  `f"…"`-Interpolation, Threads, `extern fn` in beide Richtungen,
  `option_env!`, `const` mit Text.
- Überlaufprüfung in `release-safe`, Bereichsanalyse im Optimierer.
- Eigener Garbage Collector, opt-in, inkrementell: längste Pause 0,45 ms
  bei 120.000 lebenden Knoten. Schwache Referenzen, Finalizer, `GcVec`/`GcMap`.

**Werkzeuge**
- Formatter, DWARF-Zeileninfo mit `gdb`, Language Server (`firnc --lsp`),
  Paket- und Projektsystem, Testläufer mit JSON-Ausgabe.

**Bibliotheken in Firn**
- Standardbibliothek: `lib/std`, `lib/str`, `lib/num`, `lib/rt`, `lib/gc`.
- Browser-Bausteine: HTML-Tokenizer (6810/6810 html5lib, 100 %), CSS und
  Layout (1087/1087 Kästen exakt gegen Chromium), Malwerk mit eigenem
  Scanline-Rasterer, DOM mit JS-Bindung, HTTP/1.1-Client. Sie sind die
  Grundlage von Certus (`../certus-ui`).
- JavaScript gegen test262 (63.364 Fälle, nichts gefiltert): Parser
  91,94 %, Motor 76,00 %.
- TLS und X.509 (`lib/tls`), Kryptografie und Kompression (SHA-256, AES,
  DEFLATE) gegen OpenSSL, zlib und die NIST-Vektoren geprüft.
- `lib/fui` — Oberflächenbibliothek, **4991 Zeilen**, englische Bezeichner,
  Kern läuft freistehend im Kernel-Profil. Liegt auf dem Zweig `fui`
  (`../firn-fui`), nicht auf `speed`.

**Betriebssystem**
- `profile kernel` erzeugt ein freistehendes Objektfile; `demos/kernel`
  bootet in QEMU mit Tasks, Adressräumen, Systemaufrufen und Dateien.
  Grundlage von Osum (`../osum-merge11`).

**Geschwindigkeit**
- Median 2,08×/2,19× von `rustc -O` über sechs Mikrobenchmarks,
  Spanne 1,43×–4,16×.
- Kryptografie und Kompression 1,38×–1,88× hinter OpenSSL/zlib.

Alle Zahlen mit dem Befehl dahinter: `docs/BENCHMARKS.md`.

## Was noch fehlt

- aarch64: 5 von 301 Programmen weichen ab; `tests/1613_crypto.fi` fällt
  in beiden Baustufen durch.
- Layout gegen die offiziellen Web Platform Tests: 59 von 186 — Chromium 141
  erreicht 138 von 186 auf demselben Korpus durch dieselbe Halterung.
- Malwerk gegen WPT-Referenztests: 202 von 541 (37,34 %).
- DOM und JS gegen WPT: 390 von 1714 Teilprüfungen (22,75 %); bei 169
  Dateien läuft die Halterung nicht durch.
- Der HTTP-Client weist `https://` ab — TLS ist als Bibliothek da, aber
  nicht in den Client eingehängt.
- Der Zweig `fui` ist nicht mit `speed` zusammengeführt.
- `origin/main` auf GitHub ist eine **eigene, unverbundene Historie**
  (öffentlicher Anfangsstand unter MPL-2.0). Dieser Baum lässt sich nicht
  dagegen mergen; die Zusammenführung ist offen. Die Lizenz in DIESEM Baum
  ist MIT, auf `origin/main` MPL-2.0.

## Bauen und testen

```sh
cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
./compiler/target/release/firnc -o /tmp/tour examples/tour.fi && /tmp/tour
```

```sh
bash test.sh      # baut den Compiler, übersetzt und startet jedes Programm
                  # auf drei Optimierungsstufen, dann rund vierzig
                  # Abschnittsnachweise. Dauert.
```

Maschinenlesbare Teilmenge ohne die Abschnittsnachweise:

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json
```

## Befehlszeile

```
firnc [OPTIONEN] datei.fi
  -o <pfad>            Ausgabedatei
  --package <dir>      Projekt aus <dir>/firn.package übersetzen
  --emit=exe|asm|fir|fir-opt|comptime|tokens|ast|layout|types
  --target=<name>      x86_64-linux (Vorgabe) | aarch64-linux
  --profile=<name>     kernel | app
  --opt-level=<stufe>  dev | dev-fast | release-safe | release-fast
  --no-opt             Optimierer aus
  --lsp                Language Server über stdin/stdout
  -c, --object         nur assemblieren: ELF-Objektdatei, kein ld
```

## Aufbau

| Pfad | Inhalt |
|---|---|
| `compiler/src/` | Stufe-0-Compiler in Rust, 57 Module, keine Abhängigkeiten |
| `lib/firnc1/` | derselbe Compiler in Firn — der, der den Fixpunkt erreicht |
| `lib/std/`, `lib/str/`, `lib/num/`, `lib/rt/`, `lib/gc/` | Standardbibliothek |
| `lib/html/`, `lib/css/`, `lib/dom/`, `lib/layout/`, `lib/js/` | Browser-Bausteine |
| `lib/tls/`, `lib/net/` | TLS, X.509, Netzwerk |
| `tests/`, `tests/opt/`, `tests/neg/` | Testprogramme (positiv, Optimierer, negativ) |
| `examples/`, `demos/` | kleine Programme; `demos/kernel` bootet in QEMU |
| `bench/`, `tools/` | Benchmarks und alle Nachweisskripte |
| `testdata/` | html5lib-tests, gespeicherte echte Seiten, test262 |

## Dokumentation

| Datei | Inhalt |
|---|---|
| `SPEC.md` | Sprachspezifikation; 14.1 listet jede Abweichung der Implementierung |
| `ACCEPTANCE.md` | die Abnahmepunkte, nur gegen Messung abgehakt |
| `RUN.md` | alles bauen, alles laufen lassen, alles messen |
| `docs/BENCHMARKS.md` | jede Zahl mit dem Befehl, der sie erzeugt |
| `docs/FIR.md` | die IR: Befehle, Typen, Invarianten |
| `docs/SELF_HOSTING.md` | der Bootstrap |
| `docs/ROUND*.md` | ein Bericht je Runde, Archiv |

## Lizenz

MIT in diesem Baum — siehe `LICENSE`. Der öffentliche Stand auf GitHub
(`origin/main`) steht unter MPL-2.0; welche Lizenz künftig für den
zusammengeführten Stand gilt, ist offen.

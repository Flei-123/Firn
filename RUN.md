# RUN.md — bauen, starten, selbst nachmessen

Alles hier ist **so ausgeführt worden**, wie es dasteht (14.08.2026, AMD EPYC
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

Gemessenes Ergebnis dieses Stands: **PASS 485/485**
(143 Programme × 3 Baustufen `opt` / `--no-opt` / `--opt-level=dev-fast` = 429,
51 Negativtests, dazu je ein Abschnittsnachweis für Optimierer (`test_opt.sh`,
seinerseits 41 Prüfungen), Ergebnisort-Garantie, Architekturwächter,
Symbolschema und HTML5-Tokenizer gegen html5lib; ausserdem 122 Rust-
Modultests, die nicht einzeln in PASS zählen). Laufzeit ca. 4 Minuten.

Maschinenlesbar (CI, Ziel 9 / ABNAHME Punkt 4 A):

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json > /tmp/firn.json
python3 -c "import json;d=json.load(open('/tmp/firn.json'));print(d['total'],d['passed'],d['failed'],d['rate'])"
# 337 337 0 1.0
```

(337 statt 485: der Runner enthält weder den Optimierernachweis `test_opt.sh`
noch die Abschnitte 6–9 von `test.sh`.)

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


## 4a. HTML5-Tokenizer und Fehlerunionen (Runde 3)

```sh
bash tools/tokenizer/run.sh
```

Baut den Tokenizer aus `lib/html/*.fi` in **drei** Baustufen, fährt alle
**6.810** html5lib-Fälle, prüft, dass alle drei Baustufen dieselbe Bilanz
liefern, und misst den Durchsatz gegen html5ever. Es werden **zwei** Quoten
ausgewiesen: nur Tokenstrom (linke Spalte) und zusätzlich mit Vergleich der
Parse-Fehlercodes (rechte Spalte, `harness.py --mit-fehlern`). Gemessenes
Ergebnis (14.08.2026):

```
GESAMT                      6810 /  6810 100.00 %    6809 /  6810  99.99 %
   noopt: 6810 ohne / 6809 mit Fehlercodes — gleich
   devfast: 6810 ohne / 6809 mit Fehlercodes — gleich
   -- Korpus 'html5lib' (Grenzfaelle der Testsuite, absichtlich pathologisch)
      Firn      :     4.59 MB/s  (0.889 s fuer 4.08 MB, bester von 3)
      html5ever :    11.22 MB/s  (0.363 s, bester von 3)
      Faktor    : 2.45x langsamer als html5ever (Abnahmeziel <= 2.00x)
   -- Korpus 'realweb' (acht echte Seiten aus testdata/realweb/)
      Firn      :     7.44 MB/s  (0.632 s fuer 4.70 MB, bester von 3)
      html5ever :    42.60 MB/s  (0.110 s, bester von 3)
      Faktor    : 5.72x langsamer als html5ever (Abnahmeziel <= 2.00x)
```

Gemessen wird auf **zwei** Korpora: `html5lib` (die Eingaben der Testsuite,
absichtlich pathologisch — fast nur Grenzfälle, schlechtester Fall) und
`realweb` (acht gespeicherte echte Seiten, `testdata/realweb/MANIFEST.md`).
Zwei weitere vollständige Läufe ergaben 2,25× / 2,79× (html5lib) und
7,72× / 7,84× (realweb), ein fünfter 3,09× bzw. 6,39×; Spanne also
2,25×–3,09× (html5lib) und 5,72×–8,31× (realweb). Die Bilanz war in allen Läufen und in allen drei
Baustufen identisch, der Durchsatz schwankt um ~30 %.

Schritt 0 von `run.sh` beweist, dass die Erwartungen nicht angefasst wurden:

```sh
bash tools/tokenizer/verifiziere_testdaten.sh              # sha256 gegen den Repo-Satz
bash tools/tokenizer/verifiziere_testdaten.sh --gegen-upstream   # zusätzlich gegen GitHub
```

Schritt 2b von `run.sh` ist die **Gegenprobe ohne XML-Anpassung**:

```
python3 tools/tokenizer/harness.py .tokenizer-work/tokenize --ohne-xml-modus
GESAMT                           6807 /   6810    99.96 %
```

Die XML-Anpassung (`xmlViolationTests`) ist ein optionaler Modus des Treibers
(Auftragsflagge Bit 0, `tools/tokenizer/PROTOKOLL.md`); der Harness setzt sie
nur für die vier Fälle aus `xmlViolation.test`, der HTML-Pfad bleibt gleich.

Die Messlatte html5ever muss dafür einmal gebaut werden (eigenes Cargo-Projekt,
**keine** Abhängigkeit des Compilers):

```sh
cargo build --release --manifest-path bench/tokenizer/Cargo.toml
```

Ohne sie läuft `run.sh` weiter und weist die fehlende Messlatte aus.

Einzelne Nachweise:

| Was | Befehl | Gemessenes Ergebnis |
|---|---|---|
| **Tokenizer ist Firn** | `wc -l lib/html/*.fi tools/tokenizer/harness.py` | 8.647 Zeilen `.fi` gegen 295 Zeilen Harness; die Zustandsmaschine steht in `lib/html/tokenizer.fi` (1.516 Zeilen) |
| **Sprungtabelle über 73 Zustände** | `firnc --emit=asm -o /tmp/tok.s lib/html/tokenize_main.fi && grep -c "jmp qword ptr" /tmp/tok.s` | `1` — indirekter Sprung über `.Ltbl_tokenizer__tokenize_0` |
| **Zeichenreferenzen einzeln** | `python3 tools/tokenizer/pruefe_entities.py` | `bestanden: 4657 / 4657` |
| **Fehlerunion: `catch` liefert Ersatz** | `firnc -o /tmp/e tests/403_catch_replacement.fi && /tmp/e; echo $?` | `0` |
| **Fehlerunion: `try` reicht durch** | `firnc -o /tmp/e tests/401_try_chain.fi && /tmp/e; echo $?` | der in Zeile 1 als `// expect_exit:` eingetragene Wert |
| **Verworfenes `!T` ist ein Fehler** | `firnc -o /tmp/e tests/neg/err_discarded.fi` | `error: das ergebnis darf nicht verworfen werden: der typ 'E!i32' ist mit #[must_consume] gekennzeichnet` mit Zeile:Spalte |
| **`try` außerhalb einer Fehlerfunktion** | `firnc -o /tmp/e tests/neg/err_try_outside.fi` | `error: 'try' ist nur in einer funktion mit fehlerunions-rueckgabetyp erlaubt, diese liefert i32` mit `8:13` |

## 4b. Freistehend übersetzen: `profile kernel` (Runde 52)

```sh
bash tools/freestanding/run.sh
```

Gemessenes Ergebnis (19.08.2026): **41 bestanden, 0 fehlgeschlagen** — darunter
ein echter QEMU-Boot des Kernel-Beispiels mit **beiden** Compilern.

| Was | Befehl | Gemessenes Ergebnis |
|---|---|---|
| **ELF-Objekt statt Binary** | `firnc -o /tmp/k.o demos/kernel/core.fi && readelf -h /tmp/k.o \| grep Type` | `REL (Relocatable file)` — kein `ld`, kein `_start` |
| **Keine undefinierten Symbole** | `nm -u /tmp/k.o` | leer |
| **Kein Systemaufruf im Code** | `objdump -d /tmp/k.o \| grep -c syscall` | `0` |
| **Bootet** | `ld -n -T demos/kernel/linker.ld --defsym=KERN_START=_F0.kern_start -o /tmp/k.elf /tmp/start.o /tmp/k.o && objcopy -O elf32-i386 /tmp/k.elf /tmp/k.mb && qemu-system-x86_64 -kernel /tmp/k.mb -serial stdio -display none` | `FIRN: profile kernel ist` / `freistehend.` |
| **`syscall` im Kernel-Profil** | `firnc -o /tmp/x tests/neg/free_syscall_in_kernel.fi` | `error: 'syscall' gibt es im profil 'kernel' nicht` mit Zeile:Spalte |
| **Gleitkomma ohne `#[allow_fp]`** | `firnc -o /tmp/x tests/neg/free_float_without_allow_fp.fi` | `error: gleitkomma (der typ f64) ist im profil 'kernel' nur mit #[allow_fp] erlaubt` |
| **`#[interrupt]` ist nicht aufrufbar** | `firnc -o /tmp/x tests/neg/free_interrupt_call.fi` | `error: 'ih' ist ein interrupt-einsprungpunkt und kann nicht aufgerufen werden` |
| **volatile hält** | `firnc --emit=fir tools/freestanding/volatile.fi \| grep -c 'asm.void "pause"'` | `3` — drei wörtlich gleiche Blöcke, kein CSE |

Ausführlich in `docs/RUNDE52.md`.

## 5. Was NICHT läuft, weil es nicht gebaut wurde

Ehrlich und vollständig (ausführlich in `ABNAHME.md`):

* **Constant-Time — teilweise umgesetzt, Punkt bleibt offen.** Gebaut sind die
  drei Primitive (`compiler/src/ct.rs`): `select(b, a, c)` → `cmov` ohne
  bedingten Sprung, `barrier(x)`, `secure_zero(p, n)` (überlebt den
  Optimierer). Nachweis: `tests/430_ct_select.fi` … `tests/433_ct_secure_zero.fi`
  in drei Baustufen, `tests/neg/ct_*.fi` (5 Negativtests).
  **Nicht** umgesetzt: `secret[T]`, Ausbreitung der Markierung, `declassify`,
  `u128`, `mul_wide`, Wirkung von `#[constant_time]`. Ohne `secret[T]` gibt es
  keine Typprüfung auf Geheimnisdaten. Prüfbar:
  `firnc -o /tmp/x tests/neg/int_secret_not_implemented.fi` meldet
  `'secret[T]' ist in Stufe 0 nicht umgesetzt` mit Zeile/Spalte.
  Siehe `ABNAHME.md` Punkt 6.
* **GC, `Rc`/`Gc`, DOM-Prototyp, RSS-Dauerlauf** — nicht umgesetzt. Prüfbar:
  `tests/neg/int_gc_not_implemented.fi`.
* **HTML5-Tokenizer: gebaut.** Bestandene html5lib-Fälle:
  **6.810 von 6.810 (100,00 %)** im Tokenstrom-Vergleich und
  **6.809 von 6.810 (99,99 %)**, wenn zusätzlich die `errors`-Einträge der
  Suite (Parse-Fehlercode, `line`, `col`) verglichen werden
  (`harness.py --mit-fehlern`, Schritt 2a von `run.sh`). Der eine Fehlschlag
  ist `xmlViolation.test #0`. Die XML-Anpassung der vier `xmlViolationTests`
  ist als optionaler Modus umgesetzt (Gegenprobe `--ohne-xml-modus`: 6.807).
  Geschwindigkeitsziel ≤ 2× **verfehlt**: Spanne 2,25×–3,09× (Korpus
  `html5lib`) und 5,72×–8,31× (Korpus `realweb`). Siehe Abschnitt 4a.
* **`defer` / `errdefer`, abgeleitete Fehlermenge `!T`, `catch |e| { Block }`**
  — nicht umgesetzt, siehe `SPEC.md` §14.1.fehlerunionen F1–F10.
* **Selbst-Hosting, Paketverwaltung, `comptime`/UCD-Tabelle** — offen,
  siehe `docs/SELBSTHOSTING.md` und `ABNAHME.md` Punkte 1, 5, 6.

## 6. Aufräumen

Alle Arbeitsverzeichnisse sind wegwerfbar und stehen in `.gitignore`:

```sh
rm -rf .test-work .opt-work .strwork .dtoa-work .testrunner-work .tokenizer-work bench/.work
```

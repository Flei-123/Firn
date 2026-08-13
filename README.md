# firnc0 — Stufe-0-Compiler für Firn

`firnc0` übersetzt eine bewusst kleine Teilmenge der Sprache **Firn** (`.fi`)
in **echten x86_64-Maschinencode**: Lexer → Parser → Typprüfer → eigene IR
(**FIR**) → Optimierer → eigener x86_64-Codegenerator → `as` → `ld` →
freistehendes Linux-Binary ohne libc.

* **Kein LLVM, kein Cranelift, kein C als Backend.** Der Assemblertext wird in
  `compiler/src/codegen_x86.rs` selbst erzeugt; `as` und `ld` werden
  ausschließlich als Assembler bzw. Linker benutzt.
* **Keine externen Crates.** `compiler/Cargo.toml` hat einen leeren
  `[dependencies]`-Abschnitt; es genügt `std`.
* **Kein Parser-Generator.** Handgeschriebener Lexer und rekursiv absteigender
  Parser mit Fehlerwiederherstellung.

Der verbindliche Umfang steht in [SPEC.md §12](SPEC.md) (Abweichungen der
Umsetzung in §12.1), die IR ist in [docs/FIR.md](docs/FIR.md) dokumentiert.

---

## Voraussetzungen

* `rustc` / `cargo` (getestet mit `rustc 1.99.0-nightly`, Edition 2021)
* GNU binutils: `as` und `ld` (getestet mit Binutils 2.40)
* Linux auf x86_64 (die erzeugten Binaries benutzen Linux-Syscalls direkt)

## Bauen

```sh
cargo build --release --manifest-path compiler/Cargo.toml
```

Der Compiler liegt danach unter `compiler/target/release/firnc`.
Der Build läuft mit **null Warnungen** durch (keine `#![allow(...)]`-Sammel­unter­drückung).

## Schnellstart

```sh
./compiler/target/release/firnc -o /tmp/hello examples/hello.fi
/tmp/hello
```

Echte Ausgabe:

```
Hallo Welt aus Firn!
```

```sh
./compiler/target/release/firnc -o /tmp/fib examples/fib.fi
/tmp/fib; echo "Exit: $?"
```

```
Exit: 89
```

## Alle Tests

```sh
bash test.sh
```

Echtes Ergebnis dieses Baustands (Auszug, ungekürzt am Ende):

```
== 1. Compiler bauen ==
== 2. Modul-Tests des Compilers ==
   cargo test: ok
== 3. Positivtests (jeweils mit und ohne Optimierer) ==
   75 Programme x 2 Durchlaeufe
== 4. Negativtests (Fehlermeldungen) ==
== 5. Nachweis des Optimierers ==
   PASS 18/18 (Optimierer-Nachweis)

PASS 166/166
```

`test.sh` baut den Compiler, lässt `cargo test` laufen (65 Modultests),
übersetzt **jedes** Programm aus `tests/`, `tests/opt/` und `examples/`
**zweimal** (mit Optimierer und mit `--no-opt`), assembliert, linkt, **führt
aus** und vergleicht Exit-Code bzw. Standardausgabe mit der Erwartung in Zeile 1
(`// expect_exit: N` / `// expect_out: TEXT`). Danach prüft es die 15
Negativtests in `tests/neg/` (Compiler muss mit Exit ≠ 0 abbrechen, die
erwartete Meldung samt `Zeile:Spalte` und Markierung ausgeben und darf **nicht**
paniken) und den Optimierernachweis (`test_opt.sh`).

Bestand: 65 Testprogramme in `tests/`, 6 Optimierer-Programme in `tests/opt/`,
4 Beispiele in `examples/`, 15 Negativtests in `tests/neg/`.

## Kommandozeile

```
firnc [OPTIONEN] datei.fi
  -o <pfad>        Ausgabedatei
  --emit=exe       ausführbare Datei (Standard)
  --emit=asm       x86_64-Assembler
  --emit=fir       FIR nach der Optimierung
  --emit=fir-raw   FIR direkt nach dem Lowering
  --emit=fir-opt   FIR nach dem Optimierer
  --emit=tokens    Tokenstrom
  --emit=ast       AST + Anweisungsübersicht
  --no-opt         Optimierer aus
  --stats          Größe der FIR (Funktionen/Blöcke/Instruktionen)
  --keep-asm       erzeugte .s-Datei behalten
```

---

## Kurze Sprachtour

```firn
struct Point {
    x: i32,
    y: i32,
}

const LIMIT: i32 = 10          // nur skalare, konstant auswertbare Ausdrücke

fn dist2(p: *Point, q: *Point) -> i32 {   // Aggregate nur per Zeiger (§12.1)
    let dx: i32 = (*p).x - (*q).x
    let dy: i32 = (*p).y - (*q).y
    return dx * dx + dy * dy
}

fn main() -> i32 {
    var a: Point = Point{ x: 3, y: 4, }
    var b: Point = Point{ x: 0, y: 0, }
    var arr: [i32; 4] = [1, 2, 3, 4]      // feste Größe, Index ist usize
    var i: usize = 0
    var s: i32 = 0
    while i < 4 {
        s = s + arr[i]
        i = i + 1
    }
    if dist2(&a, &b) == 25 && s < LIMIT as i32 * 2 {
        return s as i32                    // nur explizite Umwandlung mit 'as'
    }
    return 0
}
```

* `let` unveränderlich, `var` veränderlich; Parameter sind `let`-artig.
* **Keine impliziten Umwandlungen**: `i32 + i64` ist ein Fehler, `a as i64 + b`
  ist richtig. Ganzzahlliterale brauchen einen aus dem Kontext ableitbaren Typ.
* `&&`/`||` schließen kurz (im FIR als Verzweigung sichtbar, keine `and.bool`).
* Ausgabe ohne libc über `syscall(nr, a1..a6)`, z. B.
  `syscall(1, 1, &buf[0], 21)` = `write(1, buf, 21)`.

## Fehlermeldungen

```
$ ./compiler/target/release/firnc tests/neg/implicit_cast.fi
error: operator '+' erwartet zwei operanden desselben ganzzahltyps, gefunden i32 und i64
  --> tests/neg/implicit_cast.fi:5:18
   |
 5 |     let c: i64 = a + b
   |                  ^^^^^ hier
   = hinweis: es gibt keine implizite umwandlung, benutze 'as'
```

Der Parser meldet mehrere Fehler pro Durchlauf (Wiederherstellung auf
Anweisungsebene), siehe `tests/neg/two_errors.fi`. Bei kaputter Eingabe bricht
der Compiler mit Exit-Code 1 ab — kein Panic, kein `unwrap`-Absturz.

## IR und Optimierer nachprüfen

```
$ ./compiler/target/release/firnc --emit=fir-raw tests/opt/fold_arith.fi
; FIR v0
fn @main() -> i32 {
bb0:
  %0 = alloca.ptr size=4 align=4
  %1 = const.i32 20
  %2 = const.i32 2
  %3 = mul.i32 %1, %2
  %4 = const.i32 2
  %5 = add.i32 %3, %4
  store.i32 %5, %0
  %6 = load.i32 %0
  ret %6
bb1:
  %7 = const.i32 0
  ret %7
}

$ ./compiler/target/release/firnc --emit=fir-opt --stats tests/opt/fold_arith.fi
fir (roh):  1 Funktionen, 2 Bloecke, 9 Instruktionen
fir (opt):  1 Funktionen, 1 Bloecke, 4 Instruktionen
; FIR v0
fn @main() -> i32 {
bb0:
  %0 = alloca.ptr size=4 align=4
  %5 = const.i32 42
  store.i32 %5, %0
  %6 = load.i32 %0
  ret %6
}
```

`bash test_opt.sh` prüft für sechs Programme automatisch, dass die
Instruktionszahl sinkt, die gefaltete Konstante wirklich im Dump steht und tote
Blöcke verschwinden (echtes Ergebnis: `PASS 18/18`). Dass die Optimierung das
Verhalten **nicht** ändert, prüft `test.sh`, indem es jedes Programm mit und
ohne `--no-opt` ausführt und dasselbe Ergebnis verlangt.

## Erzeugter Code (Auszug aus `examples/fib.fi`)

```asm
.globl fib
fib:
    push rbp
    mov rbp, rsp
    sub rsp, 144
    mov qword ptr [rbp-8], rdi
.Lfib__bb0:
    lea rax, [rbp-132]
    mov qword ptr [rbp-16], rax
    ...
```

Registerzuteilung ist naiv (ein Stack-Slot je FIR-Wert, gerechnet in `rax`/`rcx`),
aber korrekt: System-V-Argumentregister, Rückgabe in `rax`, callee-saved
Register (`rbx`, `r12`–`r15`) werden nie angefasst, Rahmen immer 16-Byte-
ausgerichtet. Ansehen mit `--emit=asm` oder `--keep-asm`.

---

## Was Firn (Stufe 0) noch NICHT kann — ehrliche Liste

Nicht implementiert, weder im Compiler noch als Sprachmittel:

* **Module/Imports** (`import`), **Sichtbarkeit/`export`**
* **`comptime`**, **Generics**, **Interfaces**
* **`enum` und `match`**, **Fehlerunionen `!T`**, **Optionals**
* **`defer`**, **`drop`**, **Move-Prüfer**, **Arenen/Allokatoren**
* **Referenztypen `&T` / `inout T` als geprüfte Typen** — Stufe 0 hat nur
  Rohzeiger `*T`/`*mut T`; `mut` an Zeigern wird geparst, aber nicht geprüft
* **Zeichenketten** als Typ (nur `[u8; N]`), **keine String-/Zeichenliterale**
* **Gleitkomma** (`f32`/`f64`), **Vektortypen**
* **Standardbibliothek**, **Allokation**, **Panik-Handler**, Laufzeitprüfungen
  (Überlauf, Division durch null, Indexgrenzen sind ungeprüft)
* **aarch64**, **WASM**, **LLVM-Backend**, Selbst-Hosting (Stufen 1–3 der
  ROADMAP)
* Optimierung über **Konstantenfaltung und Entfernen toten Codes** hinaus
  (kein Inlining, kein mem2reg/SSA, keine Registerzuteilung mit Lebendigkeit)

Weitere Einschränkungen der Umsetzung, die in SPEC.md §12.1 festgehalten sind:

* höchstens **6 Funktionsparameter** (nur Registerargumente)
* **Structs/Arrays nicht an Funktionsgrenzen** — nur per Zeiger
* **kein Vorgabetyp für Literale**: `let x = 5` ist ein Fehler
* **kein `break`/`continue`**, kein `for`
* **kein Wiederholungsliteral `[0; N]`** — Arrays werden elementweise
  initialisiert
* **globale Variablen** gibt es nicht (nur `const`)
* `extern fn` wird erkannt und mit klarem Fehler abgelehnt
* `profile` wird geprüft, hat aber keine Wirkung

## Verzeichnisse

```
SPEC.md, ROADMAP.md      Sprachspezifikation und Fahrplan (Vertrag)
docs/FIR.md              die eigene IR: Instruktionen, Typen, Invarianten
compiler/src/            config.rs main.rs lexer.rs ast.rs parser.rs diag.rs
                         types.rs sema.rs fir.rs lower.rs opt.rs codegen_x86.rs
tests/                   65 Programme + tests/opt (6) + tests/neg (15)
examples/                hello.fi fib.fi bubblesort.fi structs.fi
test.sh                  gesamte Testsuite (baut, führt aus, vergleicht)
test_opt.sh              Vorher/Nachher-Nachweis des Optimierers
```

Der Sprachname steht ausschließlich in `compiler/src/config.rs`
(`LANG_NAME`, `LANG_NAME_LOWER`, `FILE_EXT`) — Umbenennen = drei Konstanten.

# Debugger: `.debug_line` und eine echte `gdb`-Sitzung

**Anforderung:** `W3` · `ACCEPTANCE.md` Punkt 4 Kriterium B · `TODO-FIRN.md` 0.4
**Stand:** Zeilennummern funktionieren, Variablen noch nicht (siehe „Grenzen").

## Wie es erzeugt wird

`compiler/src/dwarf.rs` sammelt beim Lowering die Zuordnung
*Instruktion → Quellzeile*; `compiler/src/codegen_x86.rs` schreibt daraus
`.file`- und `.loc`-Direktiven in den Assembler. `as` erzeugt daraus die
Abschnitte `.debug_line`, `.debug_info`, `.debug_abbrev`, `.debug_aranges`,
`.debug_str`. Es wird **kein** externes Werkzeug und **kein** C-Compiler
benutzt — nur der Assembler, der ohnehin schon im Bauweg steht.

Genauigkeit:

| Bauart | Zeileninformation |
|---|---|
| `firnc --no-opt datei.fi` | **anweisungsgenau** — jede Anweisung hat ihre Quellzeile |
| `firnc datei.fi` (mit Optimierer) | Zeile der `fn`-Deklaration je Funktion |

Der Grund für die Einschränkung steht in `SPEC.md` §14.1 Punkt 16: die FIR trägt
keine Quellpositionen (`fir.rs` ist in dieser Runde eingefroren), und der
Optimierer entfernt Instruktionen und nummeriert Blöcke neu. Eine falsche Zeile
wäre schlimmer als keine.

## Nachweis: die Sitzung, wörtlich kopiert

Programm `docs/gdb_beispiel.fi`:

```firn
// expect_exit: 55
fn summe(n: i32) -> i32 {
    var s: i32 = 0
    for i in 1 as i32..n + 1 as i32 {
        s = s + i
    }
    return s
}

fn main() -> i32 {
    let r: i32 = summe(10)
    return r
}
```

Befehle (im Projektverzeichnis, nach `cargo build --release`):

```console
$ compiler/target/release/firnc --no-opt -o /tmp/gdbdemo docs/gdb_beispiel.fi
$ readelf -S /tmp/gdbdemo | grep debug
  [ 2] .debug_aranges    PROGBITS         0000000000000000  000000e0
  [ 3] .debug_info       PROGBITS         0000000000000000  00000110
  [ 4] .debug_abbrev     PROGBITS         0000000000000000  0000013e
  [ 5] .debug_line       PROGBITS         0000000000000000  00000152
  [ 6] .debug_str        PROGBITS         0000000000000000  00000192

$ gdb -batch -ex "break summe" -ex run -ex bt -ex "info line" \
        -ex next -ex next -ex next -ex "info line" -ex continue /tmp/gdbdemo
Breakpoint 1 at 0x4000c6: file docs/gdb_beispiel.fi, line 2.

Breakpoint 1, summe () at docs/gdb_beispiel.fi:2
2	fn summe(n: i32) -> i32 {
#0  summe () at docs/gdb_beispiel.fi:2
#1  0x0000000000400254 in main () at docs/gdb_beispiel.fi:11
Line 2 of "docs/gdb_beispiel.fi" starts at address 0x4000c6 <summe> and ends at 0x40010b <summe+69>.
3	    var s: i32 = 0
4	    for i in 1 as i32..n + 1 as i32 {
5	        s = s + i
Line 5 of "docs/gdb_beispiel.fi" starts at address 0x400190 <summe+202> and ends at 0x400202 <summe+316>.
[Inferior 1 (process 536651) exited with code 067]
```

Was die Sitzung belegt:

* Ein Haltepunkt auf einen **Firn**-Funktionsnamen trifft und meldet
  Datei + Zeile der `.fi`-Datei.
* `gdb` zeigt den **Quelltext der `.fi`-Datei** an, nicht Assembler.
* `next` läuft **zeilenweise** durch das Firn-Programm (2 → 3 → 4 → 5).
* Der **Rückverfolgungsstapel** (`bt`) benennt den Aufrufer `main` mit der
  richtigen Zeile 11.
* Exit-Code `067` oktal = 55 dezimal — das erwartete Ergebnis von `summe(10)`.

Reproduzieren: die drei Befehle oben eins zu eins ausführen. Die Adressen
können sich mit dem Codegenerator ändern, Datei und Zeilen nicht.

## Grenzen (ehrlich)

* **Keine Variablen.** `print s` funktioniert nicht: es gibt keine
  `DW_TAG_variable`-Einträge und keine Typinformation im `.debug_info`.
  Dafür müsste der Compiler das `.debug_info` selbst schreiben, statt es von
  `as` erzeugen zu lassen.
* **Keine Zeilen im optimierten Bau** außer der Funktionszeile.
* `ACCEPTANCE.md` Punkt 4 Kriterium B verlangt zusätzlich, dass **ein echter
  Fehler** mit dem Debugger gefunden wurde. Das ist noch nicht der Fall und
  bleibt dort als offen geführt.

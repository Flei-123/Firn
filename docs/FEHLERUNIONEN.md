# Fehlerunionen `E!T` in Firn

Bezug: `SPEC.md` §5.1 (Vertrag), `SPEC.md` §14.1.error_unions (bewusste
Einschränkungen der Umsetzung), `PLAN.md` Runde 3.
Umgesetzt in `compiler/src/errors.rs` (Syntax, Anmeldung, Typprüfung) und
`compiler/src/lower_errors.rs` (Lowering nach FIR).
Tests: `tests/400_*.fi` … `tests/419_*.fi`, `tests/neg/err_*.fi`.

## 1. Wozu

Ein Tokenizer, ein Parser, ein Allokator — alles, was fehlschlagen darf, ohne
dass das Programm abstürzt — braucht einen Rückgabeweg für erwartete Fehler.
Firn nimmt dafür den Weg von `L7`: eine zweiwertige Union aus *Fehlercode* und
*Erfolgswert*, kein Abwickeln, keine Landing-Pads, keine versteckten Kosten auf
dem Erfolgspfad.

## 2. Sprachumfang

### 2.1 Fehlermenge

```firn
error IoError { NotFound, Permission, Closed }
```

Deklaration auf oberster Ebene. Die Codes werden in Deklarationsreihenfolge ab
`1` vergeben; `0` ist für „kein Fehler" reserviert. Eine doppelte Variante ist
ein Fehler mit Zeile und Spalte, ebenso eine doppelt deklarierte Fehlermenge.

Der Name `IoError` ist selbst ein **Typ**: der reine Fehlerwert.

```firn
let e: IoError = IoError::Permission
if e == IoError::Permission { … }        // == und != je Fehlermenge
```

### 2.2 Fehlerunion als Typ

```firn
fn lies(x: i32) -> IoError!i32 { … }     // Rückgabetyp
let r: IoError!i32 = lies(3)             // Variablentyp
struct Halter { r: IoError!i32 }         // Feldtyp
fn nimm(r: IoError!i32) -> i32 { … }     // Parametertyp
```

### 2.3 `return` wandelt implizit um

```firn
fn lies(x: i32) -> IoError!i32 {
    if x < 0 {
        return IoError::NotFound         // Fehler
    }
    return x + 7                         // Erfolg
}
```

Kein `ok(...)`, kein `err(...)`. Dieselbe Umwandlung gilt bei `let` mit
Typangabe, bei einer Zuweisung, beim Feld eines Struct-Literals und beim
Argument eines Aufrufs.

### 2.4 `try` — Fehler nach oben durchreichen

```firn
fn kette(x: i32) -> IoError!i32 {
    let v = try lies(x)                  // bei Fehler: sofort zurück, gleicher Code
    return v * 2
}
```

`try` ist nur in einer Funktion erlaubt, die selbst eine Fehlerunion **derselben
Fehlermenge** liefert. Sonst gibt es einen Fehler mit Zeile und Spalte
(`tests/neg/err_try_outside.fi`, `tests/neg/err_wrong_set.fi`).

`try` bindet so stark wie ein unärer Operator: `try f() + 1` ist `(try f()) + 1`.

### 2.5 `catch` — Ersatzwert

```firn
let v = lies(x) catch 0                  // Ersatzwert bei Fehler
let w = lies(x) catch ersatz()           // beliebiger Ausdruck
let z = lies(x) catch |e| deute(e)       // mit Bindung des Fehlerwertes
```

`catch` bindet schwächer als jeder Operator: `a catch b * 2` ist
`a catch (b * 2)`. Der Ersatzwert muss den Erfolgstyp haben; sonst Fehler mit
Zeile und Spalte (`tests/neg/err_catch_ty.fi`).

### 2.6 `!T` darf nicht verworfen werden

```firn
fn main() -> i32 {
    lies(1)                              // Fehler: das Ergebnis darf nicht
    return 0                             // verworfen werden (#[must_consume])
}
```

Der Struct einer Fehlerunion trägt `must_consume = true`; die Prüfung ist die
vorhandene in `sema::check_discard`.

## 3. Darstellung

```text
error IoError { NotFound, Permission, Closed }     // Codes 1, 2, 3

IoError        ->  struct { __err: u32 }                      4 Byte
IoError!i32    ->  struct { __err: u32, __val: i32 }           8 Byte
IoError!i64    ->  struct { __err: u32, __val: i64 }          16 Byte
IoError!Gross  ->  struct { __err: u32, __val: Gross }        40 Byte
```

`__val` liegt bei `round_up(4, align(T))`. `__err == 0` heißt Erfolg.

Das ist der ganze Trick: eine Fehlerunion ist ein **gewöhnlicher Struct** in
`types::TypeCtx`. Damit tragen Aggregat-ABI (`abi.rs`), Registerzuteilung
(`regalloc.rs`) und Codegen (`codegen_x86.rs`) sie ohne eine einzige Änderung —
bis 8 Byte in `rax`, darüber über den versteckten Rückgabezeiger.
Die Seitentabelle `union_by_struct` in `errors.rs` spielt dieselbe Rolle wie
`enum_by_struct` für Aufzählungen.

## 4. Was das Lowering erzeugt

| Quelltext | FIR |
|---|---|
| `IoError::NotFound` | `store.u32 [slot] = 1` |
| `return wert` (Erfolg) | `store.u32 [ret] = 0`, danach der Wert nach `__val` |
| `return IoError::X` | `store.u32 [ret] = code` |
| `try a` | `cmp.ne u32 a.__err, 0` → `brcond` → Fehlerblock mit `ret`, sonst Adresse von `a.__val` |
| `a catch b` | `cmp.eq u32 a.__err, 0` → `brcond` → `a.__val` bzw. `b`, Zusammenführung über einen Slot |
| `e == IoError::X` | `cmp.eq u32 e.__err, code` |

Keine neue FIR-Instruktion, kein neuer Terminator — nur `load`, `store`, `cmp`,
`brcond` und `ret`.

## 5. Aufbau der Umsetzung

| Datei | Inhalt |
|---|---|
| `compiler/src/errors.rs` | Registrierung der Fehlermengen/Fehlerunionen, Parser-Erweiterungen (`error`, `E!T`, `try`, `catch`), Typprüfung, implizite Umwandlung |
| `compiler/src/lower_errors.rs` | Lowering nach FIR |
| `parser.rs`, `sema.rs`, `lower.rs`, `lexer.rs` | je eine Zeile `// HOOK fehlerunionen` an den vorgesehenen Stellen |

Die Aufteilung folgt dem Vorbild `sema_match.rs` / `lower_match.rs`.

## 6. Grenzen

Vollständig und nummeriert in `SPEC.md` §14.1.error_unions (F1–F10). Die
wichtigsten: keine abgeleitete Fehlermenge (`!T` ohne `E`), kein `defer`/
`errdefer`, `catch |e|` bindet an einen Ausdruck statt an einen Block, kein
`match` auf Fehlerwerten, keine Vereinigung von Fehlermengen, und als Feldtyp
eines Structs nur mit skalarem Erfolgstyp.

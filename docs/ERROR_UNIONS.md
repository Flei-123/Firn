# Error unions `E!T` in Firn

Reference: `SPEC.md` §5.1 (contract), `SPEC.md` §14.1.fehlerunionen
(deliberate restrictions of the implementation), `PLAN.md` round 3.
Implemented in `compiler/src/errors.rs` (syntax, registration, type
checking) and `compiler/src/lower_errors.rs` (lowering to FIR).
Tests: `tests/400_*.fi` … `tests/419_*.fi`, `tests/neg/err_*.fi`.

## 1. What for

A tokenizer, a parser, an allocator — everything that may fail without
the program crashing — needs a return path for expected errors.
Firn takes the way of `L7` for that: a two-valued union of *error code* and
*success value*, no unwinding, no landing pads, no hidden costs on
the success path.

## 2. Language surface

### 2.1 Error set

```firn
error IoError { NotFound, Permission, Closed }
```

Declaration at the top level. The codes are handed out in declaration order
from `1` on; `0` is reserved for „no error". A duplicate variant is
an error with a line and a column, and so is a doubly declared error set.

The name `IoError` is itself a **type**: the pure error value.

```firn
let e: IoError = IoError::Permission
if e == IoError::Permission { ... }      // == and != per error set
```

### 2.2 The error union as a type

```firn
fn read(x: i32) -> IoError!i32 { … }     // the return type
let r: IoError!i32 = read(3)             // the variable type
struct Halter { r: IoError!i32 }         // Feldtyp
fn nimm(r: IoError!i32) -> i32 { … }     // Parametertyp
```

### 2.3 `return` converts implicitly

```firn
fn read(x: i32) -> IoError!i32 {
    if x < 0 {
        return IoError::NotFound         // Fehler
    }
    return x + 7                         // Erfolg
}
```

No `ok(...)`, no `err(...)`. The same conversion applies with a `let` that
has a type annotation, with an assignment, with the field of a struct
literal and with the argument of a call.

### 2.4 `try` — pass the error upwards

```firn
fn chain(x: i32) -> IoError!i32 {
    let v = try read(x)                  // on an error: straight back, same code
    return v * 2
}
```

`try` is only permitted in a function that itself returns an error union of
**the same error set**. Otherwise there is an error with a line and a
column (`tests/neg/err_try_outside.fi`, `tests/neg/err_wrong_set.fi`).

`try` binds as tightly as a unary operator: `try f() + 1` is `(try f()) + 1`.

### 2.5 `catch` — substitute value

```firn
let v = read(x) catch 0                  // a substitute value on an error
let w = read(x) catch substitute()       // any expression
let z = read(x) catch |e| explain(e)     // with the error value bound
```

`catch` binds more weakly than any operator: `a catch b * 2` is
`a catch (b * 2)`. The substitute value must have the success type;
otherwise there is an error with a line and a column
(`tests/neg/err_catch_ty.fi`).

### 2.6 `!T` must not be discarded

```firn
fn main() -> i32 {
    read(1)                              // error: the result must not be
    return 0                             // discarded (#[must_consume])
}
```

The struct of an error union carries `must_consume = true`; the check is
the existing one in `sema::check_discard`.

## 3. Representation

```text
error IoError { NotFound, Permission, Closed }     // Codes 1, 2, 3

IoError        ->  struct { __err: u32 }                      4 Byte
IoError!i32    ->  struct { __err: u32, __val: i32 }           8 Byte
IoError!i64    ->  struct { __err: u32, __val: i64 }          16 Byte
IoError!Gross  ->  struct { __err: u32, __val: Gross }        40 Byte
```

`__val` lies at `round_up(4, align(T))`. `__err == 0` means success.

That is the whole trick: an error union is an **ordinary struct** in
`types::TypeCtx`. Because of that, the aggregate ABI (`abi.rs`), the
register allocation (`regalloc.rs`) and the codegen (`codegen_x86.rs`)
carry it without a single change — up to 8 bytes in `rax`, above that over
the hidden return pointer.
The side table `union_by_struct` in `errors.rs` plays the same role as
`enum_by_struct` for enums.

## 4. What the lowering produces

| Source text | FIR |
|---|---|
| `IoError::NotFound` | `store.u32 [slot] = 1` |
| `return wert` (success) | `store.u32 [ret] = 0`, then the value into `__val` |
| `return IoError::X` | `store.u32 [ret] = code` |
| `try a` | `cmp.ne u32 a.__err, 0` → `brcond` → error block with `ret`, otherwise the address of `a.__val` |
| `a catch b` | `cmp.eq u32 a.__err, 0` → `brcond` → `a.__val` resp. `b`, merged over a slot |
| `e == IoError::X` | `cmp.eq u32 e.__err, code` |

No new FIR instruction, no new terminator — only `load`, `store`, `cmp`,
`brcond` and `ret`.

## 5. Structure of the implementation

| File | Content |
|---|---|
| `compiler/src/errors.rs` | registration of the error sets/error unions, parser extensions (`error`, `E!T`, `try`, `catch`), type checking, implicit conversion |
| `compiler/src/lower_errors.rs` | lowering to FIR |
| `parser.rs`, `sema.rs`, `lower.rs`, `lexer.rs` | one line `// HOOK fehlerunionen` each at the intended places |

The split follows the model of `sema_match.rs` / `lower_match.rs`.

## 6. Limits

Complete and numbered in `SPEC.md` §14.1.fehlerunionen (F1–F10). The
most important ones: no inferred error set (`!T` without `E`), no `defer`/
`errdefer`, `catch |e|` binds to an expression instead of to a block, no
`match` on error values, no union of error sets, and as the field type of a
struct only with a scalar success type.

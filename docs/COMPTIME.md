# Compile-time evaluation (`const`, `const fn`, `comptime`)

## What runs at compile time (firnc0)

| Place | What is evaluated |
|---|---|
| `const X: T = expr` | integer/bool/float literals, operators, other consts, calls of functions (interpreted) |
| `static X: [T; N] = ...` | literals, `[v; n]`, text literals, struct literals, **calls of `const fn` returning an integer, bool or (nested) array** |
| `[v; N]` repeat count | same evaluator as `const` |
| `comptime { ... }` | interpreter, `emit_raw` / `emit_number`, `file_size` / `file_byte` |

## `const fn`

```firn
const fn crc_table() -> [u32; 256] {
    var t: [u32; 256] = [0 as u32; 256]
    var i: u32 = 0
    while i < 256 {
        var c: u32 = i
        var k: i32 = 0
        while k < 8 {
            if c & 1 == 1 { c = 0xEDB88320 ^ (c >> 1) } else { c = c >> 1 }
            k += 1
        }
        t[i as usize] = c
        i += 1
    }
    return t
}
static CRC: [u32; 256] = crc_table()
```

A `const fn` is an ordinary function (callable at run time as well) with a checked promise:

- body may call only other `const fn` (error at the call, with position)
- no pointers (`&`, `inout`, `*`), no floating point (error at the use, with position)
- evaluated by the interpreter in `compiler/src/comptime.rs`; plain `fn` stays callable from a `const` initializer as before (stability)

Interpreter scope: `let`/`var`, assignment, `+= -= ... ` and `++`/`--`, indexing and assignment of array elements (nested arrays too), `if`, `while`, `for`, `break`, `continue`, `return`, calls and recursion (depth 64, 2,000,000 statements, arrays up to 4,194,304 elements).

Typing rules: values are `i128` internally; every store into a typed local, parameter, array element or return value is cut to the declared integer type (like `release-fast` wrapping). `as T` converts using the written type. Untyped locals (`let x = ...`) are not cut.

Errors carry source position: index out of range, division by zero, endless loop, non-const call, pointer use.

## Not supported (yet)

- struct values, `str`, floats inside a `const fn`
- arrays as `const` (only `static`)
- array length expressions in types: `[u8; N]` needs a literal; `[v; N]` repeat counts may use consts and `const fn` calls
- const generics `[const N: usize]`
- firnc1: parses `const fn` but marks the file "not core" (exit 3, same as other unported features); calls in const contexts are not evaluated there (`ct_needed`)

## Tests

`tests/6765_const_fn_tables.fi`, `tests/6766_const_fn_crc_static.fi` (CRC32 table checked against the check value 0xCBF43926), `tests/neg/const_fn_*.fi`.

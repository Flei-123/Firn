# firn-bindgen: C header import (lib/cbind, tools/cbind)

Reads a C header and writes Firn declarations that match the C layout and the
existing FFI (`extern fn`, `#[link_lib]`, `#[link_name]`; docs/FFI.md).

```
firnc tools/cbind/firn_bindgen.fi -o firn-bindgen
firn-bindgen [-I dir] [-D NAME[=V]] [--system] [--lib NAME] [--wrap SPEC]... [--glue-prefix P]
             [--emit firn|layout|check-c|check-fi|glue] [-o FILE] header.h
```

| Option | Meaning |
|-|-|
| `--emit firn` (default) | consts, `type` aliases, structs, `extern fn` |
| `--lib NAME` | adds `#[link_lib(NAME)]` to every extern fn |
| `--system` | also searches `/usr/include` and `/usr/include/x86_64-linux-gnu` for `<...>` includes |
| `--emit layout` | `S name size` and `O name.field offset` lines predicted by the tool |
| `--emit check-c` | C program printing the same lines with `sizeof`/`offsetof` (gcc/clang reference) |
| `--emit check-fi` | Firn program printing the same lines with `size_of` and field addresses |
| `--wrap NAME` | wrap the `static inline` function NAME of the header in a C glue function |
| `--wrap NAME:ret(args)` | wrap the function-like macro NAME, e.g. `ADD3:int(int,int,int)` |
| `--emit glue` | the C glue file (`<prefix>NAME`, default prefix `fbg_`) |

With `--wrap`, `--emit firn` declares `#[link_name(fbg_NAME)] extern fn NAME(...)`:
compile the glue with `cc -c`, link it with the Firn object (see `tools/cbind/run.sh`).

## Modules (lib/cbind)

| File | Job |
|-|-|
| `cb_lex.fi` | tokenizer (comments, continuations, strings, numbers, punctuators) |
| `cb_expr.fi` | constant expression evaluator (`#if`, enums, array sizes, macro values) |
| `cb_pp.fi` | preprocessor: `#include`/`#define` (object + function-like, `#`, `##`, `__VA_ARGS__`), `#if/#ifdef/#elif/#else`, `#undef`, `#pragma once` |
| `cb_types.fi` | type table, records, typedefs, builtin typedefs (`size_t`, `uint32_t`, ...) |
| `cb_pscan.fi`, `cb_parse.fi` | declaration parser: specifiers, declarators (pointers, arrays, function pointers), struct/union/enum, attributes `packed`/`aligned` |
| `cb_layout.fi` | System V x86-64 sizeof/alignof/offsets incl. bit-fields, packed, aligned |
| `cb_names.fi`, `cb_emit.fi` | C type -> Firn type, Firn output, layout manifest |
| `cb_check.fi`, `cb_glue.fi` | check programs, C glue |

## Mapping

* `int` `u8`.. -> `i32` `u8`.., `char` -> `i8`, `_Bool` -> `u8`, `float`/`double` -> `f32`/`f64`.
* Pointers -> `*mut T` (`*mut u8` for `void *`/`char *`), function pointers -> `u64` (a Firn function value is a record address, not a C pointer).
* Structs -> `struct` with explicit `_padN` / `_tail` / `#[packed]` / `#[align(n)]` so size and offsets equal the C layout.
* Unions -> `struct { _data: [u8; size] }` with `#[align]`. Bit-field runs -> `_bitsN: [u8; k]` plus a warning.
* Enums -> `type E = u32` (`i32` with negative values) and `const` per enumerator. Object-like macros with constant integer value -> `const NAME: i32|i64`.
* Unknown typedef names (system headers not followed) -> opaque `struct X { _opaque: u8 }`.

## Limits (honest)

1. Target is Linux x86-64 System V (LP64). No Windows (LLP64) or 32-bit layouts.
2. Variadic functions, function-like macros and `static inline` bodies are not declared without `--wrap` (warning printed).
3. No accessors for bit-fields; `long double`/`__int128` become byte arrays (field alignment 16 is not expressed per field).
4. `sizeof` inside constant expressions, string/float macros, `_Complex`, K&R declarations and `extern` variables are not supported (warning or skipped).
5. glibc system headers are only followed with `--system` and parsed without `__GNUC__`; use typedef knowledge instead (`FILE` and friends are opaque).

## Proof

`bash tools/cbind/run.sh` (18 checks): tool-predicted layout == gcc `sizeof`/`offsetof` == Firn `size_of`/field addresses for
`cases/kernel_like.h` (list_head, pci_dev excerpt, packed, aligned, bit-fields, unions, nested), `/usr/include/zlib.h`,
`/usr/include/linux/input.h` (with `--system`) and `sqlite3.h` (downloaded by `apt-get download libsqlite3-dev`);
constants, `#ifdef`, pointer-to-pointer, warnings; all 278 sqlite3 extern fns compile; glue: `static inline` + macros callable from Firn.

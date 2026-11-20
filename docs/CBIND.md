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

## `firn bindgen` and automatic glue linking

```
firn bindgen HEADER.h [-I d] [-D N[=V]] [--system] [--lib L] [--wrap SPEC]... [--emit E] [-o F.fi]
firn build app.fi [--glue extra.c] [-o out]
```

* `firn bindgen` builds `firn-bindgen` into the cache on first use (rebuilt when `lib/cbind` changes) and passes the options through. Without `-o` the result goes to stdout.
* With `--wrap` and `-o F.fi`, the C glue is written to `F.glue.c` (it includes the header by absolute path).
* `firn build x.fi` compiles every `*.glue.c` next to `x.fi` (and each `--glue FILE.c`) with `cc`, assembles `x.fi` with `firnc -c`, and links all objects with `ld` against `libc.so.6`. No manual `cc`/`objcopy` step. Without glue files `firn build` behaves as before.
* Not done: an in-language `#[link_glue]` attribute inside firnc (the driver does the linking; `firnc` alone still needs `-c` plus a manual link).

## Bit-field accessors

Every named bit-field (width 1..32) of a struct gets `fn S_get_f(p: *mut S) -> i32|u32` and `fn S_set_f(p: *mut S, x: i32|u32)` next to the struct. They read/write the `_bitsN` byte run using the predicted System V bit offsets; the setter keeps all neighbouring bits. Wider fields and fields in unions get a warning instead. Proof: 20000 random reads and read-modify-write operations against gcc bit-fields (`cases/ffi_features_host.c`), byte-for-byte equal structs.

## sizeof, text constants, variadics

* `sizeof(type)` works in enums, array sizes, `#if` and macro values: builtin types, pointers, `struct/union/enum X`, typedef names. `sizeof(expr)` and `sizeof(T[n])` are rejected.
* `#define NAME "text"` (adjacent literals concatenate; escapes `\n \t \r \\ \" \0` only) -> `const NAME: str`. `#define X 3.14`, `-2.5e-3`, `.5f` -> `const X: f64`.
* Variadics: Firn has no variadic ABI. Declare each call shape as a glue wrapper: `--wrap "vsum3=vsum:int(int,int,int,int)"` emits `int fbg_vsum3(int,int,int,int) { return vsum(...); }` and `extern fn vsum3(...)` (`ALIAS=NAME:ret(args)`).

## aarch64

The layout rules are the same LP64 rules as x86-64 (`long`/pointers 8 bytes, same alignments; `long double` differs: 16 bytes on both but IEEE quad on aarch64, held as bytes). `tools/cbind/run.sh` compiles the `check-c` program with `aarch64-linux-gnu-gcc` and runs it under `qemu-aarch64`: output equals the prediction for `kernel_like.h` and `ffi_features.h`.

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
| `cb_bitfield.fi` | bit-field runs and get/set accessor generator |
| `cb_sizeof.fi` | registers `sizeof:<type>` values for the expression evaluator |
| `cb_textconst.fi` | string and float macros -> `const` lines |

## Mapping

* `int` `u8`.. -> `i32` `u8`.., `char` -> `i8`, `_Bool` -> `u8`, `float`/`double` -> `f32`/`f64`.
* Pointers -> `*mut T` (`*mut u8` for `void *`/`char *`), function pointers -> `u64` (a Firn function value is a record address, not a C pointer).
* Structs -> `struct` with explicit `_padN` / `_tail` / `#[packed]` / `#[align(n)]` so size and offsets equal the C layout.
* Unions -> `struct { _data: [u8; size] }` with `#[align]`. Bit-field runs -> `_bitsN: [u8; k]` plus get/set accessors.
* Enums -> `type E = u32` (`i32` with negative values) and `const` per enumerator. Object-like macros with constant integer value -> `const NAME: i32|i64`.
* Unknown typedef names (system headers not followed) -> opaque `struct X { _opaque: u8 }`.

## Limits (honest)

1. Linux x86-64 System V and aarch64 LP64 only. No Windows (LLP64) or 32-bit layouts.
2. Variadic functions and function-like macros need an explicit `--wrap` per call shape.
3. Bit-fields wider than 32 bits and bit-fields in unions have no accessors; `long double`/`__int128` become byte arrays (field alignment 16 is not expressed per field).
4. `sizeof(expr)`, `_Complex`, K&R declarations, `extern` variables, hex-float and non-simple-escape string macros are skipped.
5. glibc system headers are only followed with `--system` and parsed without `__GNUC__`; `FILE` and friends are opaque.
6. `firn build` links glue with `ld` and libc only; glue that needs other libraries must be linked manually.

## Proof

`bash tools/cbind/run.sh` (27+ checks): tool-predicted layout == gcc `sizeof`/`offsetof` == Firn `size_of`/field addresses for
`cases/kernel_like.h` (list_head, pci_dev excerpt, packed, aligned, bit-fields, unions, nested), `/usr/include/zlib.h`,
`/usr/include/linux/input.h` (with `--system`) and `sqlite3.h` (downloaded by `apt-get download libsqlite3-dev`);
constants, `#ifdef`, pointer-to-pointer, warnings; all 278 sqlite3 extern fns compile; glue: `static inline` + macros callable from Firn.
Also: `cases/ffi_features.h` (sizeof, string/float consts, bit-field accessors vs gcc, variadic wrappers, linked with gcc), `firn bindgen` + `firn build` glue auto-link, aarch64 layout under qemu.

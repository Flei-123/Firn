# Prelude

A few C#-style conveniences work without `import`. The prelude is a source-level
pass in both compilers (`compiler/src/prelude.rs`, `lib/firnc1/prelude.fi`); it runs
per file right after `derive` and before the lexer.

## What is implicit

| You write | The compiler adds |
|---|---|
| `x.to_str()` | module `std.tostr` is loaded (methods on numbers and `bool`) |
| `parse.to_i32(s) catch 0` | `import std.parse` |
| `format.format2(..)` | `import std.format` |
| `chars.count(s)` | `import std.chars` |
| `println(s)` / `print(s)` | `import std.io` and a file-local `fn println(s: str)` / `fn print(s: str)` |

Nothing else is implicit: `io.print_line(..)` still needs `import std.io`.

## Rules

* Loaded only when used. A file that does not use these names is not touched and
  compiles to the same assembly as before.
* Your own name wins. A module name is added only if every occurrence of the name in the
  file is `name.member` (no `let parse`, no field `format:`, no `fn chars`), and the file does
  not import it. `println`/`print` are added only if every occurrence is a call and the file
  declares no function of that name. `to_str` is loaded only if no file of the program
  declares a `to_str` method (`fn to_str(*self ..)`) and none imports `std.tostr`.
* `--no-prelude` switches the pass off.
* Files with `profile kernel` are never touched, so `--no-gc`, kernel and freestanding
  builds see no change.
* Imports are inserted in front of line 1 on the same line, wrappers are appended after the
  last line: line numbers never move.

## gc_init

The compiler already writes the call to `gc_init()` into the generated start code
(x86-64, aarch64, Windows, WASM) whenever the collector runtime is linked in. An explicit
`gc_init()` stays valid (idempotent). Proof: `tests/6823_gc_init_automatic.fi`,
`tests/6824_gc_init_explicit_still_valid.fi`.

## Tests

`tests/6820_prelude_basic.fi`, `tests/6821_prelude_own_names.fi`,
`tests/6822_prelude_method_to_str_own_impl.fi`, `tests/neg/6820_prelude_other_modules_stay_explicit.fi`.

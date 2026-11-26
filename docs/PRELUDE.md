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
| `format("{0} and {1}", x, y)` | `import std.format`, `import std.io`, each argument after the template goes through a file-local generic `__fmt_show[T]` (f-string formatting, so numbers, `bool`, `str` work directly) and the call becomes `format.format2(..)`; up to 8 arguments |
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

## Variadic `format`

`format(tpl, a, b, c)` is the typed variadic form for the common case: any number
(0 to 8) of arguments of any type that an f-string can print. It is a source rewrite
inside the prelude. The general `..T` parameter is a separate feature
(`docs/VARIADIC.md`); `format` stays as it is because its arguments have mixed types.
Nested calls work (`format("{0}", format("<{0}>", x))`). More than 8 arguments are left
untouched and fail as an unknown function. `format.format0` to `format.format8` stay available.

## gc_init

The compiler already writes the call to `gc_init()` into the generated start code
(x86-64, aarch64, Windows, WASM) whenever the collector runtime is linked in. An explicit
`gc_init()` stays valid (idempotent). Proof: `tests/6823_gc_init_automatic.fi`,
`tests/6824_gc_init_explicit_still_valid.fi`.

## Tests

`tests/6820_prelude_basic.fi`, `tests/6821_prelude_own_names.fi`,
`tests/6822_prelude_method_to_str_own_impl.fi`, `tests/6825_prelude_format_call.fi`, `tests/6826_prelude_own_format_wins.fi`, `tests/neg/6820_prelude_other_modules_stay_explicit.fi`.

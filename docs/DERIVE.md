# derive

`#[derive(Debug, Eq, Ord, Hash, Clone, Default, Json)]` in front of a `struct` or `enum` writes the `impl` block for you.

Implementation: a text pass in the lexer entry, firnc0 `compiler/src/derive/`, firnc1 `lib/firnc1/derive*.fi` (hook `lex_derive` in `lexer.fi`), same generated text. The attribute is blanked out, the generated `impl` is inserted on the SAME line right after the closing `}` (line numbers of the rest of the file stay), and `import std.derive;` is added when the file does not import it. No macro system: the output is ordinary source that the normal checker sees, so every error is an ordinary error on the line of the declaration.

| Derive | Generates | Needs from each field |
|---|---|---|
| `Debug` | `fn debug(*self) -> str`, text `Name { a: 1, b: "x" }`, enums `Var(1, 2)` | `debug()` (interface `Debug` in `std.derive`: ints, floats, bool; `str` handled directly) |
| `Eq` | `fn eq(*self, o: T) -> bool` (so `a == b` works on structs) | `==` |
| `Ord` | `fn cmp(*self, o: T) -> i64` (so `a < b` works on structs), enums compare variant index first | `<` `>` (str, bool handled directly) |
| `Hash` | `fn hash(*self) -> u64` | `hash()` (interface `Hash`: ints, bool; `str` directly; no floats) |
| `Clone` | `fn clone(*self) -> T` | `clone()` (interface `Clone`: ints, floats, bool; `str` copied) |
| `Json` | `fn to_json(*self) -> str`: structs `{"a":1,"b":"x"}`, enums `"Unit"` or `{"Var":[1,2]}` (firnc0 only) | `to_json()` (interface `Serialize` in `std.derive`: ints, floats, bool; `str` escaped directly) |
| `Default` | free function `default_Name() -> Name` (impl blocks have no associated functions) | ints, floats, bool, `str`, `Vec[X]`, or a type with its own `default_Type()` |

Rules and limits:
- A field without the method gives `type 'Inner' has no method 'debug'` (derive `Debug` on `Inner` too).
- Unknown name: `unknown derive 'Debog', known: ...`; wrong target: `derive attribute must stand before a struct or an enum`.
- Generic structs work (`#[derive(Debug, Eq, Clone)] struct Pair[A, B]`), every instance is checked as usual.
- Generic enums work (`#[derive(Debug, Json)] enum Opt[T]`, firnc0). `Default` works on non-generic structs only.
- The derive attribute must start its line and stand on one line.
- `Debug` allocates through the GC: call `gc_init()` first.
- Operators `<` `>` work on structs only (operators.rs); on enums call `a.cmp(b)`.
- firnc1 supports every derive on structs and `Debug`/`Hash`/`Clone` on enums. Enum `Eq`/`Ord` generate a nested `match`; firnc1 cannot resolve outer bindings inside a nested `match` (known firnc1 bug), so such files fail in firnc1 only.
- `Json` writes text through `std.derive`, it does not build a `std.json` tree; reading (`from_json`) is not generated. Not yet: firnc1 `Json`, `PartialEq` aliases, `Vec` fields.

Tests: `tests/6771_derive_core.fi` (firnc0, all derives), `tests/6772_derive_struct_enum_basic.fi` (both compilers), `tests/diff/derive_basic.fi`, `tests/diff/neg_derive_*.fi`, `tests/neg/derive_*.fi`.

# Type aliases -- `type Idx = u32`

Round TUPLES, 09.10.2026, `docs/GAPS.md` B14 (closed as A15).
Code: `compiler/src/alias.rs`, `compiler/src/modules.rs` (`expand_alias`),
`lib/firnc1/parser.fi` (`alias_*`), `bin/firnc1.fi` (module order).
Tests: `tests/2400_type_alias.fi`, `tests/2401_type_alias_module.fi`,
`tests/neg/2400..2407_alias_*.fi`, `tools/typealias/run.sh` (both compilers).

## What it is

```firn
type Idx = u32                  // a primitive
type Bytes = *mut Octet         // a pointer; Octet is declared further down
type Row = [Idx; 4]             // an array of an alias
type Cb = fn(Idx) -> Idx        // a function type
type Vi = Vec[i32]              // one instantiation of a generic struct
type Span2 = core.Span          // a type of another module
type P = Point                  // a struct: P { x: 1, y: 2 } builds a Point
```

An alias is **another name for the same type, not a new type**. `Idx` and
`u32` pass into each other without a cast, `Vec[Idx]` and `Vec[u32]` are ONE
instantiation, an error message names the target, and `size_of[Idx]()` is
`size_of[u32]()`. That is the whole point: it is what lets `std.core` and
`std.str` share one `Span` (GAPS B14) instead of two types of the same shape.

Rules:

* **Order does not matter.** `fn f(x: Idx)` may stand above `type Idx = u32`,
  and `type A = B` above `type B = u32`.
* **A use is replaced while the program is read.** An alias never reaches
  the type checker, the lowering or the code generator -- which is why it is
  free and why both compilers can share the rule.
* **A module can export an alias** (`export { Idx }`) and a user writes
  `m.Idx` in a type position or in front of `{`. The target is read from the
  point of view of the module that declared it: `type Pair = Item` in module
  `m` is `m__Item`, not an `Item` of the importing module.
* **`impl Idx { .. }` with `type Idx = Point`** means `impl Point` (an alias
  is the same type). An alias for a pointer, an array or a type of another
  module cannot carry methods and is refused.
* **`type` is not a keyword.** It is an identifier that can start an item
  and nothing else, so a variable named `type` keeps working.

What is refused, with line and column:

| program | message |
|---|---|
| `type A = B` + `type B = A` | `the type alias 'A' refers to itself` |
| `type Idx = u32` twice | `type alias 'Idx' is already declared` |
| `type V[T] = Vec[T]` | `a type alias cannot have type parameters` (write the alias for ONE instantiation) |
| `struct Point` + `type Point = u32` | `'Point' is the name of a struct and of a type alias` |
| `Idx[3]` | `the type alias 'Idx' takes no type arguments` |
| `impl Bytes { .. }` with `type Bytes = *mut u8` | `methods cannot be attached to the type alias 'Bytes'` |
| `Row { .. }` with `type Row = [i32; 2]` | `the type alias 'Row' does not name a struct` |
| `size_of[Row]()` with `type Row = [i32; 2]` | `'size_of' takes one plain type name, and 'Row' names a composite type` |

## Limits (stated, not hidden)

* **No generic aliases.** `type V[T] = Vec[T]` is refused; the alias names one
  instantiation (`type Vi = Vec[i32]`). A generic alias needs a substitution
  at the use site that the instantiation machinery (`mono`) would have to
  learn; nobody has needed it yet.
* **An alias is a name for a type position.** In an expression it is accepted
  as the name of a struct literal and nowhere else (`P { .. }`; not `P.new()`
  -- a method is called on a value).
* **Enum names and `gc class` names** are program-wide and not module
  renamed; an alias with the name of one of those is not diagnosed in
  stage 1 (stage 0 does not either).
* **Import cycles.** `firnc1` parses a module before the files that import it
  as soon as ANY file declares an alias. In a cycle of imports one module has
  to come first; an alias that the module parsed first takes from the other
  one is then unknown (`unknown type`), where `firnc0` -- which resolves the
  aliases of other modules after parsing -- would accept it. No program of the
  repository has such a cycle.
* **Error messages of `firnc1`** are the ones of its parser: it counts the
  error and says "the parser failed". `firnc0` points at the line.

## How the two compilers do it

|  | `firnc0` (Rust) | `firnc1` (Firn) |
|---|---|---|
| declarations of a file | `alias::hook_begin` finds them before the first item and parses every target | `alias_search` |
| a use in the same file | `alias::hook_use` in `parse_type_inner`: a copy of the target | `alias_lookup` in `ty`: the node of the target (shared, like every unchanged node in `mono.fi`) |
| a use of another module's alias | stays `m.Name`; `Renamer::expand_alias` puts in the target, renamed as its own module sees it | the registry in `mono.Gen` (`gen_alias_add/find`) holds the node; the module is parsed first |
| the generic identity `Vec[Idx]` = `Vec[u32]` | the argument is the target already when `Vec[..]` is parsed; across modules `Renamer::inst` re-keys the instantiation | the argument is the target's node when `gen_instance` sees it |
| struct literal / `impl` / `size_of` | `hook_struct_name`, `hook_impl_name`, `hook_plain_name` | `alias_struct_name`, `alias_plain_name` |

`tools/typealias/run.sh` runs the positive programs, the importing module in
front of the module it imports, a chain over three modules, and every refused
program through both compilers.

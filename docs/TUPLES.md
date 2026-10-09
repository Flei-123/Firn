# Tuples -- `(A, B)`, `(a, b)`, `t.0`, `let (a, b) = f()`

Round TUPLES, 09.10.2026, `docs/GAPS.md` B5 (closed as A16).
Code: `compiler/src/parser.rs`, `compiler/src/sema.rs` (`tuple_type`,
`tuple_lit`), `compiler/src/lexer.rs` (`tuple_index`), `compiler/src/lower.rs`
(`literal_reads_target`); `lib/firnc1/parser.fi`, `types.fi` (`tuple_ty`),
`sema.fi` (`tuple_literal`), `lexer.fi` (`lex_tuple_index`), `lower.fi`.
Tests: `tests/2410..2415_tuple_*.fi`, `tests/2413_tuple_swap_alias.fi`,
`tests/neg/2410..2419_tuple_*.fi`, `tools/tuples/run.sh` (both compilers).

## What it is

```firn
fn divmod(a: i32, b: i32) -> (i32, i32) {      // several return values
    return (a / b, a % b)
}

let (q, r) = divmod(17, 5)                     // taken apart into names
let t = (1, 2)                                 // a tuple literal; t: (i32, i32)
let u: (i64, u8) = (10, 3)                     // the context types the elements
let x = t.0 + t.1                              // an element by number
let ((a, b), (c, d)) = ((1, 2), (3, 4))        // nested, taken apart
let (_, second) = divmod(9, 4)                 // `_` skips an element
var (lo, hi) = (1, 2)                          // `var` makes the names writable
t = (t.1, t.0)                                 // the swap (needs `var t`)
type Pair = (i32, i32)                         // an alias is a tuple type like any other
```

A tuple is **no new kind of thing**: `(i32, u8)` is a struct named
`(i32, u8)` whose fields are called `0` and `1`. Everything a struct has, a
tuple has, with no extra rule:

* **layout** -- declaration order, natural alignment, size rounded up to the
  alignment (`tests/2411_tuple_abi.fi` reads the offsets);
* **passing and returning** -- as the struct with those fields: in registers
  up to 16 bytes (integer or SSE eightbytes), in memory beyond, a result
  through the hidden pointer from 9 bytes on (SPEC 14.1);
* **copy** -- a tuple is a value; `let y = x` copies;
* **ownership** -- a tuple with an element that has a `drop` is an owner
  (below);
* **identity** -- two tuple types are the same type exactly when their element
  types are the same (the name is made from them), whatever way they are
  spelled: `(i32, i32)`, `Pair`, `(int, int)`.

## Syntax

```ebnf
tuple_type = "(" type "," type { "," type } [ "," ] ")" ;
tuple_lit  = "(" expr "," expr { "," expr } [ "," ] ")" ;
elem       = postfix "." int_lit ;                       (* t.0, t.0.1 *)
let_tuple  = ( "let" | "var" ) tuple_pat [ ":" type ] "=" expr ;
tuple_pat  = "(" pat "," pat { "," pat } [ "," ] ")" ;
pat        = ident | "_" | tuple_pat ;
```

At least two elements: `(T)` and `(a)` are parentheses, `(a,)` and `()` are
errors. The lexer reads the digits right behind a `.` as an integer, never as
a float, so `t.0.1` is `t`, `.`, `0`, `.`, `1` (both lexers; the rule is in
`tools/lex_compare.sh`'s corpus).

`let (a, b) = e` is a **rewrite in the parser**: `let t = e` followed by
`let a = t.0` and `let b = t.1`, with a hidden name `__tup#<k>#<n>` (`k` counts
the hidden bindings of the file, `n` is the number of names). The type
checker refuses a tuple of another size there ("the pattern has 2 names, but
the tuple has 3 elements") and a value that is no tuple.

## Types of the elements

* **With a context** -- a `let` with a type, a parameter, a result, a field, an
  assignment target -- the tuple type of the context types the elements:
  `let u: (i64, u8) = (10, 3)`. An untyped literal takes the type it is given.
* **Without one** every element has the type it has on its own, as in
  `let x = 5` (an integer literal is `i32`, a float literal `f64`).
* There are no implicit conversions between tuple types (`(i32, i32)` to
  `(i64, i64)`); the elements are converted one by one.

## Generics

`fn swap[T](a: T, b: T) -> (T, T)`, `Box[(i32, i32)]`, `Vec[(i32, i32)]` and
`both[A, B](a, b) -> (A, B)` work (`tests/2415_tuple_generic.fi`). The naming
scheme of the instantiations is `tup2_i32_i32`. `size_of[T]()` inside a
template, with `T` a tuple (or a pointer, an array, a function type), passes
the type on through `sizeof.rs::stash_type`; before this round `Vec[*mut u8]`
did not work for that reason.

## Ownership and `drop`

A tuple with an element that has `fn drop(inout self)` is an owner: built by
a literal (the elements are moved in), returned, passed, assigned (the old
value is dropped first), and dropped at the end of its block with its elements
in order (`tests/2412_tuple_drop.fi`). **`let (a, b) = pair` is refused for such a
tuple** -- it would move the elements out one by one and partial moves do not
exist (SPEC 3.3): "cannot move a field out of its owner"
(`tests/neg/2415_tuple_partial_move.fi`). Pass the tuple on as a whole, or use
`&` / `inout`.

## A bug found on the way

`p = P { x: p.y, y: p.x }` -- and so `t = (t.1, t.0)` -- wrote the literal
field by field into the target, so the second field read what the first had
overwritten (`x == y`). That was in `main` for structs and arrays long before
tuples. Now a literal that reads the variable at the root of the assignment
target (or, through a pointer, any memory) is built in a place of its own and
copied (`lower.rs::literal_reads_target`, `lower.fi::literal_reads_target`);
a literal that reads nothing of the target is written in place as before, so
no frame grows (`tools/result_location`). `tests/2413_tuple_swap_alias.fi`.

## Limits (stated, not hidden)

* **No tuple patterns outside `let`/`var`**: not in `match` arms, not as
  `(a, b) = f()` on an existing pair of variables (a statement may begin with
  `(`, so it could not be told from an expression), not in a `for` or a
  parameter. Write `let (a, b) = f()` or `t = f()` and read `t.0`.
* **No comparison** of tuples with `==` (structs have none either).
* **`Self` in a tuple of an interface method** is resolved like a pointer or
  an array (`(Self, i32)` works).
* **A tuple inside an `enum` payload** is laid out when the enum is: if its
  elements are enums or structs that are laid out later than the payload, the
  size can be wrong. A tuple in a struct field and in a signature is exact (it
  takes part in the struct layout order); use a named struct in the payload
  when in doubt.
* **The layout yardstick** (`--emit=layout`, `bin/layoutdump.fi`) resolves the
  declarations of the root file and prints `?` for a tuple, in both
  implementations; `tests/2410_tuple_basic.fi` is compared that way.
* **Error messages of `firnc1`** are the ones of its parser and type checker:
  it counts the error and says which stage failed. `firnc0` points at the line.

## How the two compilers do it

|  | `firnc0` (Rust) | `firnc1` (Firn) |
|---|---|---|
| tuple type | `TypeExpr::Tuple`; `resolve_ty` makes the struct; during `collect_structs` the layout waits and joins the topological order (`tuple_pending`) | node kind `T_TUP` (children = elements); `tuple_ty` lays out the structs of the elements first while the field types are resolved (`lay`/`lst`) |
| tuple literal | struct literal named `(tuple)` with fields `0`, `1`, ..; `tuple_lit` | the same node (`E_SLIT`), `tuple_literal` |
| `t.0` | `Field(t, "0")`, the lexer reads the digits | `E_FIELD` with the name `0`; `lex_tuple_index` |
| `let (a, b)` | `let_tuple`, statements queued in `after` | `let_tuple`, `tuple_bindings`, queue `after` |
| generics | `subst_ty`, `type_tag` (`tup2_i32_i32`) | `subst_ty`, `tag_build` |
| the swap fix | `lower.rs::literal_reads_target` | `lower.fi::literal_reads_target` |

`tools/tuples/run.sh` runs every positive program through both compilers,
every refused program through both (they must both say no, with a real
error), and compares the syntax trees (`--emit=ast-canon` against
`bin/astdump.fi`).

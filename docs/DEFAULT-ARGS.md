# Default and named arguments (r664, IMPLEMENTED)

Gap list item (vspy-lang): `fn f(a: i32, b: i32 = 2)` and `f(b: 1, a: 4)`.
Python: `def f(a, b=2)`, `f(b=1, a=4)`. Status: IMPLEMENTED in firnc0
(`compiler/src/dargs.rs`) and firnc1 (`lib/firnc1/mono.fi`, `dargs_run`),
parser + canonical printer in both. Differences from the design below:
the pass runs after the module merge AND the monomorphization (callees have
final names, generic instances are plain functions); it is a node-table scan
in firnc1. Named arguments exist as `ExprKind::Named` / `E_NAMED`
(parser output only). Arguments are evaluated in parameter
order, not in written order. **r704 (methods + templates):** `x.m(a, b: 1)` and
`f[T](a, b: 1)` take defaults and named args. Generic instances are plain
functions when the pass runs; the default is copied into each instance with
`T` substituted (`mono.rs` / `fn_instantiate`). Methods are NOT resolved by
the receiver type (sema runs later): the pass matches the bare method name
against every function whose first parameter is `self` (`pick_method` in
`dargs.rs`, `dargs_pick` in `mono.fi`). One candidate: it is used. Several
(same method name on different types): the call is left alone when a candidate
takes it as written; accepted when all have the same parameter names and no
defaults (named-only); else error "needs default or named arguments but
exists on several types". `impl I for T` methods take no defaults (the
signature must equal the interface). firnc1 also checks "default must be a
constant" now (error counted, no text). Consts inside defaults of a module
function are renamed with the module (`modules.rs`). Also open: exact
receiver-type resolution (second sema pass) if the name rule gets in the way.
Tests: `tests/3910_default_args.fi`, `tests/3950_default_args_methods.fi`, `tests/3951_std_defaults.fi`, `tests/neg/default_args_*.fi`.

## Syntax

```firn
fn connect(host: str, port: i32 = 80, tls: bool = false) -> i32 { ... }

connect("a")                       // port = 80, tls = false
connect("a", 8080)                 // tls = false
connect("a", tls: true)            // named, skips port
connect(port: 1, host: "a")        // all named, any order
```

* A parameter with `= <const-expr>` is optional. Once one parameter has a
  default, ALL following parameters need one ("a required parameter cannot
  follow an optional one").
* Default = constant expression only: literals, `const` names, unary minus,
  casts, `size_of[..]`, arithmetic on those (the evaluator that already
  exists for array lengths and `const`, `comptime.rs`). No calls, no names of
  locals, no other parameters. Strings: text literals only (a `str` view of
  static data).
* Call arguments: positional first, then `name: expr`. After a named
  argument no positional one ("positional argument after a named one").
  Errors: unknown parameter name (with `did you mean`), parameter given twice
  (positional + named or named twice), missing required parameter (names it).
* Not allowed on: `extern fn`, function values / `fn(..)` types (no defaults
  in a type), interface methods in `impl ... for` (the signature must equal
  the interface; defaults stay out of that comparison), closures.
  Methods (`x.m(..)`) and generic functions (`f[T](..)`) are allowed; the
  type arguments precede the value arguments as today.

## Where it is resolved

Everything at compile time, before type checking sees the call, so sema,
lowering, the optimiser and the backends are unchanged:

1. **Parser** (`parser.rs` / `lib/firnc1/parser.fi`): parameter list accepts
   `= expr` and records it in `Param.default: Option<Expr>`; argument list
   accepts `IDENT ':' expr` and records `Arg.name: Option<String>`. Today the
   AST holds `Vec<Expr>` for arguments; the cheapest change keeps that and
   adds a parallel `Vec<Option<(String, Span)>>` on `ExprKind::Call` (named
   slots) -- or wraps a named argument as `ExprKind::Named(name, expr)`
   (one new node kind, parser-only; sema never sees it, see 2).
2. **Call-site rewrite pass** (new, between parse and sema, like
   `refparam.rs`): needs the callee's signature, so it runs after all modules
   are merged and generic names are mangled. For each call to a known
   function: reorder named arguments into parameter order, append the
   default expressions (cloned with fresh ExprIds, `reid`-style as in
   `foreach.rs`) for the missing tail. Result: a normal positional call.
   Method calls and `Type::method` paths use the same table.
3. **Sema/lowering/moves/backends**: unchanged (they see positional calls).
4. **Mangled generic instances**: the default is stored on the template
   (`FnTemplate.decl`) and cloned into the instance like any parameter.
5. **Diagnostics**: spans point at the call; "missing argument 'port'
   (declared with no default at file:line)".

## Parity cost (why it is not done in this round)

* `ast` gets a field in both compilers (`lib/firnc1/ast.fi` has a flat node
  table: the default needs a new child slot in `fn` params; the named-arg
  node needs a kind constant + `astdump`/`semadump`/`firdump` support).
* New pass in both compilers; `firnc1` has a flat symbol table of function
  signatures that must keep the defaults per parameter.
* `--emit=ast` output changes -> `parser_compare` text must match char by
  char in both compilers; the canonical printer (`ast_canon.rs` /
  `print.fi`) needs the `= expr` and `name:` forms.
* ~600 lines Rust, ~900 lines Firn, tests 3770.. in all four levels plus
  neg tests (order, duplicate, unknown name, non-constant default).

## Order of work

1. AST + parser + printer in both compilers (parser_compare green).
2. Rewrite pass in firnc0, then firnc1 (sema_compare / fir_compare green).
3. Library: use defaults in `std.args`, `std.log` setup helpers.
4. Update SPEC + `docs/FIRN-VS-PYTHON.md` gap rows.

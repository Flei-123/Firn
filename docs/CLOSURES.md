# Closures in Firn -- state, gaps, recommendation (13.10.2026)

Measured, not assumed: `tests/871`-`875` (closure plain / capture / gc root / callback / vec_sort_by) pass on
main (bae77570d + this branch). Design reasoning is in `docs/ROUND58.md`; this page is the short view.

## What works today
* `fn(A) -> R` is a value: ONE word, a pointer to a record whose first word is the code address.
  Fields, parameters, array elements, results: all fine. `==` / `!=` compare records.
* `fn(..) {..}` captures nothing (no GC record). `gc fn(..) {..}` captures: type `AllocError!fn(..)`, needs `try`.
* Capture is BY VALUE (copy at creation), at most ONE word per captured name (an aggregate goes in as `*mut T`
  or `Gc[T]`). Writing to a captured name is a compile error. The record is a normal GC object of a synthesised
  class, so `Gc[T]` captures are traced transitively (tests/873, with counter-check).
* Library users: `vec_sort_by/min_by/index_where/all/any`, `std.pool`, `async.post`.

## Gaps (found by reading + running)
1. `async.loop`, `post`, `pool` keep callbacks as raw addresses in memory the GC does not scan (loop.fi A2):
   a capturing closure given to a timer / watch is swept. -> fixed WITHOUT compiler change by `async.fnpin`
   (root table, `pin`/`unpin`). Test `tests/2869_async_closure_pin.fi`, mutation `tools/net/pin_mutation.sh`
   (pin removed -> segfault, exit 139). Manual unpin: leaks if forgotten.
2. ~~Closure literal inside a generic function is refused~~ -- FIXED r716 (firnc0): every instance gets its own
   closure ids (`mono.rs`, test 4042). firnc1 mirror still open.
3. ~~`Vec[fn(..)]`~~ already works on main (test 4043).
4. `s.f(1)` on a struct FIELD is a call since round 68 (tests/1238); `v[i](1)` on an array/Vec element still needs a local first (OPEN: needs a call node with a callee EXPRESSION in sema + lower, both compilers).
5. Captures larger than one word; mutation of captured state needs an explicit `*mut` / `Gc[Cell]`.
6. No self-recursion of a literal -- by design: a literal has no name; recurse through a named `fn` (or pass the function itself as a parameter).

## Recommendation
* Keep capture BY VALUE (no lifetimes in the language; by-ref would dangle). Do NOT add capture by reference.
* Callbacks that must carry state: `gc fn` + `fnpin` today. Next step (library): `loop.watch_add_fn` variants that
  pin and unpin automatically on `watch_del` / timer end (small, no compiler change).
* Alternative that needs nothing at all: plain `fn(ctx: *mut Ctx, ..)` plus an explicit context record
  (what `ud: u64` already is) -- fastest, zero allocation, but the caller keeps `ctx` alive.
* Compiler work, ordered by value / cost:
  (a) fn type as template argument (fix `sizeof.rs` substitution) -- M,
  (b) closure literal in generics (one capture class per instance) -- M/L,
  (c) multi-word capture by copying aggregates via descriptor (record gets variable size, offsets list per
      class already exists) -- L, only if (a)/(b) are not enough,
  (d) escape analysis to put non-escaping closures on the stack (`fn` literal that is only passed down) -- L,
      pure optimisation; measure first (a record is one 8+8n bytes GC allocation).
* No prototype of (a)-(d) now: each touches sema/sizeof/gc.rs and the twin compiler (firnc1); roadmap items instead.

## Addendum 18.10.2026: lambda sugar and stack closures (design, not built)
Checked on main: closures with capture exist (`gc fn(..) {..}`, tests 871-875, `src fnval.rs`, twin in firnc1); the
older note "Firn has no closures" is wrong. What is missing is the cheap, non-escaping form:
* Syntax: `|x, y| expr` / `|x| { .. }` as sugar for `fn(x: T, y: U) -> R { return expr }`; parameter types and
  result come from the expected `fn(..)` type (only allowed where one is known: argument of a fn-typed parameter,
  `let f: fn(..) = ..`). `|` is also `catch | e |` and bit-or, so the parser may only try it at expression START and
  only when the expected type is a fn type.
* Lowering: identical to `fn(..) {..}` (code fn with the record as LAST parameter, captured by value).
* Non-escaping form `local fn(..) {..}` / lambda passed down: record is an `alloca` of 8+8n bytes in the caller frame
  (no GC, no `try`, type plain `fn(..)`). Safety: reuse `escape.rs` (the rule that refuses a pointer to a local
  leaving the function): the closure value counts as `&local`; storing it in a struct/static/returning it is an error
  unless `#[allow_escape]`. Callee must not keep it (same contract as `ud: *mut Ctx` callbacks today).
* Cost: sema (expected-type lookup), lower (alloca instead of `gc_alloc`), escape.rs, both compilers + parser_compare
  / sema_compare / fir_compare cases. Roadmap items r-lambda / r-localfn.

## Lambda sugar (r715, built 18.10.2026)
```
sort_stable_by[Row](&rows, |a, b| (*a).amount < (*b).amount)   // expression body = `return expr`
each(5, &total, |acc, i| { *acc = *acc + i })                  // block body
let f: fn(i32) -> i32 = |x| x - 1                              // expected type from the let
let g = |x: i32, y: i32| -> i32 x + y                          // fully typed: no context needed
let h: fn() -> i32 = || 42
let c: fn(i32) -> i32 = try gc |x| x + n                       // capturing form = heap record, like `gc fn`
```
* Parser: a `|` (or `||`) at the START of an operand is a lambda (bit-or is infix, `catch |e|` follows an operand).
  Untyped parameters carry the marker type `__infer`, a missing result the marker `__lam_expr` / `__lam_blk`.
* `compiler/src/lambda.rs` (pass after mono + dargs, BEFORE sema) replaces the markers from the expected
  `fn(..) -> R` type: parameter of a plain call or method call (all candidates must agree), `let x: T`, `return`
  (fn result type), struct literal field. `E!fn(..)` (the `gc` form) is looked through. Without a usable expected
  type: error "cannot infer the types of this lambda here". Afterwards the lambda is exactly `fn(x: T) -> R { .. }`
  -- same lowering as before, capture by value, no stack closures yet.
* firnc1 (`lib/firnc1/lamsugar.fi` + `lambda_sugar` in parser.fi) mirrors: arguments of plain calls (also behind `try`)
  and `let`/`var` with a declared type. Not mirrored (stage 0 only): method calls, struct fields, `return`.
* Stack/non-escaping closures (`local fn`) are still only a design (section above).

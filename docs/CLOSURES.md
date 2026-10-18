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
   closure ids (`mono.rs`, test 4042). firnc1 mirror: `gen_closure_template`/`copy_lambda` (done).
3. ~~`Vec[fn(..)]`~~ already works on main (test 4043).
4. ~~`v[i](1)` on an array/Vec element~~ -- FIXED r728 (`Call("method __call", [callee, args])`, both compilers; tests 4150-4154).
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
  and `let`/`var` with a declared type, `return` (the parser stores the result type in the `S_RET` statement) and
  struct literal fields, and (r732) method-call arguments (all methods of that name and arity must agree on the `fn` type).
* A closure inside a template body: firnc1 parks the closure function in `Gen` (`gen_closure_template`) instead
  of the function table; every instance gets its own copy (`copy_lambda` in mono.fi, fresh `__closure#N`).
  `-> AllocError!T` on a generic fn is a separate firnc1 gap (not closure related).
* firnfmt: `|x, y|` stays tight (`lambda_open`/`lambda_close` in tools/fmt/fmt.fi); binary `a | b` is unchanged.
* Stack/non-escaping closures (`local fn`) are still only a design (section above).

## `?` on Option (r725)
`let x = f()?` / `a = f()? + g()?` / `return h(f()?)` / `if f()? > 0 {..}`: `f()` runs into a hidden `let __q#L_C`
in front of the statement; `!__q#L_C.has` does `return opt_none[U]()` (the enclosing fn must return `Option[U]`,
`import std.option`); the value is `__q#L_C.val`. Not allowed (error): right side of `&&`/`||`, `while` condition,
lambda expression body, `defer`, `else if` condition, a fn that does not return an Option. Operands of all `?` in
a statement run in source order before the rest of the statement. firnc0 `parser.rs::question_op`, firnc1
`parser.fi::question_op` + `mono.option_none_name`.

## Generic functions as values (r718)
```
let f = id[i32]                          // instance of a generic fn = ordinary fn value
apply(id[i32], 3)                        // as an argument
let o = Ops { f: dbl[i32] }              // struct field
fn via[T](v: T) -> T { return apply_g[T](id[T], v) }   // inside a template: T is substituted per instance
let s = option.opt_some[i32]             // module qualified
```
* `name[Targs]` NOT followed by `(` is the mangled instance name as a plain identifier (`sema_generic.rs::hook_generic_call`,
  firnc1 `parser.fi` postfix loop). The instance is queued like for a call, so the value is a normal named function:
  one-word static record, `==` works, no allocation.
* Inside a template body `mono.rs::subst_expr` (firnc1: `copy_expr_inner` -> `inst_rewrite`) substitutes the type
  parameters of such an identifier.
* Type arguments may be fn types: `pass[fn(i32) -> i32](inc)`, `Holder[fn(i32) -> i32]{..}`.
* r742: a generic function WITHOUT type arguments takes them from the expected `fn(..)` type:
  `apply(id, 4)`, `let f: fn(i32) -> i32 = dbl`, `g = dbl`, struct field `Ops { f: dbl }`, `return dbl`,
  `fn(*mut Vec[i32]) -> i32` against `count[T](v: *mut Vec[T])` (generic struct instances unify by their arguments).
  Without an expected fn type (`let f = id`) it stays an error (`unknown name`, hint). A type parameter the
  fn type does not fix is an error (`cannot infer the type parameter 'U' of 'conv' ...`).
  firnc0: `lambda.rs::value_rewrite` + `sema_generic::infer_fn_value` + `mono::expand_late` (instances recorded after
  `mono::expand`; the lambda pass runs again over the new bodies). firnc1: `lamsugar.fi` calls the hook
  `mono.gen_infer_value` (`unify_ty`), `gen_run` expands the new instances and runs the pass again.
  Limits: firnc1 treats a fn-typed (or untyped) variable of that name ANYWHERE in the program as the variable (flat tree, over-cautious).
* r742: methods as values: `Type.m` (no call) is the plain function `Type__m`, receiver pointer = first parameter:
  `let f = Acc.bump; f(&a, 4)`, `apply(Acc.bump, ..)`, struct field. A BOUND receiver (`x.m` without a call) is NOT supported:
  firnc0 reports `method 'm' of 'T' used as a value without a call` (hint: `Type.m` or `|a| x.m(a)`); firnc1 leaves it to the type checker.
* r742: lambda assignment targets (`x.f = |a| ..`) find the type of the target through method-call results
  (`let x = obj.method()`), `try f()`, cast, address-of, module statics/consts (firnc0) and are block scoped
  (a `let` in an inner block that shadows with another type ends with the block; `for` variables shadow). Tests 4300-4304.

## r742b (18.10.2026): the rest of generic fn values
* `apply_g(id, 4)` -- a generic CALLEE without type arguments is allowed when an argument is the NAME of a generic fn
  (`sema_generic::infer_call`, firnc1 `gen_infer_call`): `T` comes from the other arguments (typed locals, casts, `&x`,
  literals as a weak default `4` -> i32, `2.5` -> f64; a plain `fn` name gives its signature in firnc0). Then the fn-value
  argument is inferred from the now concrete parameter type. Other bare generic calls stay an error (tests/neg/generic_without_ty_args).
  Not inferable -> `cannot infer the type parameter 'T' of 'f' from the arguments` (tests/neg/4305).
  firnc1 gets the argument types from its statement walk (`calls_in_expr`, same env as the lambda targets); a plain `fn` name as
  argument gives no type there (write `apply_g[i32](inc, 4)`).
* `mod.f` (qualified, no `[T]`) is a generic fn value in both compilers (`option.opt_some`): modules.rs resolves it to the global template name.
* firnc1: a fn-typed/untyped variable named like a generic fn hides it; `ty_of` knows casts, `&x`, module statics/consts.
* Bound method value (r750, firnc0 only): `try x.m` (x = a plain variable of type `*T` / `*mut T`, `m` a method, no call) is
  `try gc |a0, ..| x.m(a0, ..)` -- the record captures the receiver pointer (one word, by value). Parameter and result types
  come from the METHOD, so no expected type is needed (`let g = try p.add`). Without `try` -> error "needs `try`"
  (tests/neg/4352); a struct VALUE receiver -> error "receiver is not a pointer" (tests/neg/4303); `Type.m` still passes the
  receiver as first argument. The program needs the GC runtime (some `gc` token or `import std.vec`) so that `AllocError` exists.
  Test 4351 (`firnc1_skip`): firnc1 builds closures inside the parser (capture analysis while parsing), so the twin needs types in the parser.

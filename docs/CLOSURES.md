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
2. Closure literal inside a generic function is refused (copy per instantiation).
3. `Vec[fn(..)]` / fn type as template argument does not compile (`size_of$T` substitution). Wrapper struct works.
4. `s.f(1)` / `v[i](1)` is not a call on a field / element; bind first.
5. Captures larger than one word; mutation of captured state needs an explicit `*mut` / `Gc[Cell]`.
6. No self-recursion of a literal.

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

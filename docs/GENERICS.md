# Generics: what works, what does not (probed 2026-11-12, firnc0 main abe6d3aef)

Every row below was run against `firnc`; the test column names a file in `tests/` that pins the behaviour.

| Feature | State | Evidence |
|---|---|---|
| Generic functions `fn f[T](x: T) -> T`, monomorphized | works | `tests/210_generic_fn.fi`, `730_generics_core.fi` |
| Type argument inferred from an expected `fn` type or call | works | `4300_generic_fn_infer.fi`, `4305_generic_call_infer.fi` |
| Generic function as a value | works | `4280_generic_fn_value.fi`, `4282_generic_fn_values_mixed.fi` |
| Generic structs `struct Box[T]` and `Vec[T]`, `Map[K, V]` | works | `211_generic_struct.fi`, `212_generic_map.fi` |
| Generics across modules, qualified calls and types | works | `3970_qualified_generic_call.fi`, `3991_qualified_generic_type.fi`, `1707_module_generic_local.fi` |
| Generics with closures, error unions, tuples | works | `4042_closure_in_generic.fi`, `4170_errunion_generic_fn.fi`, `2415_tuple_generic.fi` |
| Interface bound `fn f[T: Area](x: *T)` | works, checked at the call: `type 'i32' does not implement the interface 'Area' — bound on the type parameter 'T' of 'f'` | probe, `docs/ROUND46.md` |
| Built-in bounds `T: Int`, `T: Scalar` | works | `lib/std/testkit.fi` (`assert_eq[T: Int]`), `lib/rt/vec.fi` |
| Methods (`impl Typ { fn m(*self) }`) on a non-generic struct | works | `docs/ROUND45.md` |
| Dynamic dispatch `dyn Interface` | works | `docs/ROUND46.md` |
| Methods on a generic type `impl Box[T] { .. }` | **missing**: `expected 'fn', 'struct', ... at top level, found 'impl'` | `docs/ROUND45.md` section 10 |
| Operator overloading (`impl Add for V2`, `a + b`) | **missing**: `unknown interface 'Add'` | `docs/ROUND45.md` section 10 |
| Default methods in an interface | missing | `docs/ROUND46.md` section 9 |
| Interface inheritance (`interface A: B`) | missing | `docs/ROUND46.md` section 9 |
| Associated functions `Typ::new()` | missing (`::` is no token) | `docs/ROUND45.md` section 10 |
| Type parameters on aliases `type V[T] = ..` | refused | `SPEC.md` (type aliases section) |

Workaround today: free generic functions over a generic struct (`fn box_get[T](b: *Box[T]) -> T`, as `std.vec` does).

## Plan for the missing parts (not started: each needs both compilers and tests/diff)

1. **Methods on generic types.** Parse `impl Box[T]`; when `Box[i32]` is instantiated, instantiate the methods with `T = i32` under the instance name. Reuse the monomorphizer (`compiler/src/mono.rs`) for the bodies and `impls.rs` for the method table (`Typ__m` naming). `lib/firnc1` needs the same in parser, sema and lower.
2. **Operator overloading** as sugar over fixed interfaces: `a + b` on a struct becomes `Add.add(&a, b)`; fixed set `Add Sub Mul Div Neg Eq Ord Index`. Needs 1 only for generic operands; do it after methods exist for plain structs (already true).
3. **Default methods and interface inheritance**: lowering rule for a default body (one copy per implementing type) and a method-table prefix rule.

Roadmap items r-generic-1..3 carry these.

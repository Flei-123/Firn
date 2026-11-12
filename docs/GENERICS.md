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
| Methods on a generic type `impl Box[T] { .. }` | works in `firnc0` and `firnc1` (2026-11-12) | `tests/6720_impl_generic_core.fi`, `tests/neg/impl_generic_*.fi`, `tests/diff/impl_generic_*.fi` |
| Operator overloading (`impl V2 { fn add(*self, o: V2) -> V2 }`, `a + b`) | works: `a + b` calls `add` on a struct operand (`add sub mul div neg eq cmp`) | `docs/OPERATORS.md` |
| Default methods in an interface | works: `fn m(*self) -> T { body }` inside `interface`; copied as `X__m` at the end of every `impl I for X` that does not define it | `tests/6730_iface_default_core.fi` |
| Interface inheritance (`interface A: B + C`) | works: the methods of the parents come first in `A` (and its method table), `impl A for T` needs `impl B for T` too, `dyn A` and bounds `[T: A]` call the parent methods | `tests/diff/iface_inherit_basic.fi` |
| Associated functions `Typ::new()` | missing (`::` is no token) | `docs/ROUND45.md` section 10 |
| Type parameters on aliases `type V[T] = ..` | refused | `SPEC.md` (type aliases section) |

Methods on generic types, how it works: `impl Box[T] { fn m(*self) .. }` is parsed as a template per method, stored under `Box__m` next to the function templates (the type parameters of the `impl` header are the template parameters, their names are free: `impl Box[U]` is fine). When the struct instance `Box[i32]` (`Box__i32`) is expanded, every method template of `Box` is expanded with it as `Box__i32__m`. After that the ordinary method resolution of round 45 finds `Box__i32__m` by the receiver type; nothing downstream changed.

Default methods: the body is parsed once with `Self` as the receiver type and copied per implementing type (type parameter `Self` in firnc1, `replace_self` in firnc0); `Self` is usable in the signature only, the body may call methods the interface declares and free functions visible in the implementing module; a `gc class` receiver works in firnc0 only (firnc1 cannot call a class method from another method of the same class, tracked separately).

Limits: a method cannot be generic itself; `impl Iface for Box[T]` is not supported; `impl Box[i32]` (a specialised block) is not supported, the header lists type parameters only; every method of every used instance is type-checked even when never called.

## Plan for the missing parts (each needs both compilers and tests/diff)

1. **Methods on generic types.** Done, see above (`compiler/src/impl_generic.rs`, `mono.rs::expand_methods`, `lib/firnc1/mono.fi::methods_instantiate`).
2. **Default methods in interfaces** (r1472, done in firnc0 `iface_default.rs` and firnc1 `parser.fi interface_default_body/impl_defaults_add` + `mono.fi gen_defaults_instantiate`). `interface I { fn m(*self) -> T { body } }`: the parser keeps the body (`iface.rs` and `lib/firnc1/iface.fi` store signatures only today, so both registries need a body slot, a template like the method templates above). At the end of every `impl I for X` block each default method that the block does not define is copied as `X__m` with `self` typed `*X`, through the same expansion used for generic methods. Dynamic dispatch through `dyn I` needs no change: the method table is filled from the `X__m` names. Error to add: a default body that calls a method the interface does not declare.
3. **Operator overloading** (r1473, done in firnc0 `operators.rs` and firnc1 `opsugar.fi` + `sema.fi op_method`). A binary operator on a struct operand calls the method of the left operand: `+ - * /` -> `add sub mul div`, `== !=` -> `eq` (returns bool, `!=` negates it), `< <= > >=` -> `cmp` (returns i64, compared with 0), unary `-` -> `neg`. Any method with that name on the struct counts (inherent `impl V2` or `impl Iface for V2`); the receiver rules of ordinary method calls apply (`*self` needs an addressable operand). `Index` is not part of this step. firnc1 rewrites the tree node in place into a method call, firnc0 types the same synthetic call and lowers it through `lower_call`.
4. **Interface inheritance** (`interface A: B`): method-table prefix rule, after 2.

Roadmap items r-generic-1..3 carry these.

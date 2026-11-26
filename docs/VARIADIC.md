# Variadic parameters `..T` (r1645)

```firn
fn sum(xs: ..i32) -> i32 {
    var total: i32 = 0
    for x in xs { total += x }
    return total
}
sum()  sum(5)  sum(1, 2, 3)
```

* The LAST parameter may be `name: ..T`. Inside the function it is a `Vec[T]` (`for x in xs`, `xs.len()`, `vec_at`).
* Source pass before the lexer, in both compilers (`compiler/src/variadic.rs`, `lib/firnc1/variadic.fi`), after `derive`, before the prelude. Line numbers stay stable; columns on a rewritten line shift.
* The declaration becomes `xs: Vec[T]`. Each call `f(a, x, y)` of a function declared in the SAME file becomes `f(a, vec.vec_with[T](vec.vec_with[T](vec.vec_new[T](), x), y))`; `import std.vec` is added when missing. Calls inside `f"..{f(1, 2)}.."` are rewritten too. No limit on the argument count.
* Methods (`fn m(*self, xs: ..T)`) get the `Vec[T]` parameter but no call rewrite. Generic element types (`..T` with a type parameter) are not packed either.
* Calls from another module are not rewritten: build the `Vec[T]` with `vec.vec_with`.
* `profile kernel` files are not touched.
* Added to `std.vec`: `vec_with(v, value)` (returns the Vec with `value` appended) and the method `xs.len()` on `Vec[T]`.
* Errors name the helper: `argument 2 of 'vec_with__i32' has type str, expected i32`.
* Tests: `tests/6830_variadic_params.fi`, `tests/neg/variadic_wrong_type.fi`.

# Type-name function calls (r1642, r1643)

* `Vec[i32].new()`, `Map[u32, i32].new()`, `Set[u32].new()`: the parser turns `Name[Args].f(..)` on a generic struct into a call of the instance function `Name__Args__f` (a function of `impl Name[T] { fn f() .. }`). `new` is declared in `lib/rt/vec.fi` and `lib/rt/map.fi`. A missing function ends as `unknown function 'Vec__i32__mew'`.
* `mod.Type.f()` (module-qualified plain types) already worked after K1.
* `T.missing()` for a struct, enum, alias or primitive type with no such function: `type 'T' has no function 'missing'` at the type name, help `did you mean 'x'?` from the functions of that type (`compiler/src/typecall.rs missing`, `lib/firnc1/sema.fi type_missing`).
* Tests: `tests/6831_type_function_calls.fi`, `tests/neg/type_missing_function.fi`, `tests/neg/type_missing_function_prim.fi`.

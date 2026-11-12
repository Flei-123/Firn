# Operator overloading

Operators on a struct operand call a method of the left operand. There is no interface to implement: a plain `impl T { fn add(*self, o: T) -> R }` is enough. Both compilers (firnc0 `compiler/src/operators.rs`, firnc1 `lib/firnc1/opsugar.fi`) apply the same rules.

| Source | Method | Result |
|---|---|---|
| `a + b` `a - b` `a * b` `a / b` | `add` `sub` `mul` `div` | result type of the method |
| `-a` | `neg` | result type of the method |
| `a == b` / `a != b` | `eq` (`!=` negates it) | `bool` |
| `a < b` `a <= b` `a > b` `a >= b` | `cmp` | method returns `i64` (negative, 0, positive), compared with 0 |

Rules:
- Only struct operands (not `str`). Integer, float and pointer operands keep the built-in meaning.
- The right operand has whatever type the method declares (`fn mul(*self, k: i64) -> V2` gives `v * 3`).
- A struct without the method keeps the old error; a wrong operand type is an ordinary argument error.

Trap: methods take `*self`, so the left operand must be addressable. `(a + b) * c` is rejected ("needs an address"); write `let t = a + b` and then `t * c`.

Tests: `tests/diff/op_overload_*.fi`, `tests/diff/neg_op_*.fi`.

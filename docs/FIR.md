# FIR — the intermediate language of `firnc0`

FIR ("Frontend Intermediate Representation") is the compiler's own, typed
intermediate language. It sits between the type checker and the
x86_64 backend:

```
Quelle -> Lexer -> Parser -> AST -> Typprüfer -> [ FIR ] -> Optimierer -> x86_64
```

FIR is **not** a renamed AST: expressions are decomposed into individual
instructions with value ids of their own, control flow exists only as basic
blocks with terminators, variables are memory slots (`alloca`) and every
instruction has an explicit machine type. Conversely, FIR contains nothing
machine-specific: registers, stack frames, calling convention and
instruction selection only come into being in the backend.

Printing the textual form:

```
firnc --emit=fir-raw file       # FIR straight after the lowering
firnc --emit=fir-opt file       # FIR after constant folding + dead code removal
firnc --emit=fir     file       # the same as --emit=fir-opt
```

Implementation: `compiler/src/fir.rs` (data structure + textual form),
`compiler/src/lower.rs` (AST -> FIR), `compiler/src/opt.rs` (optimizer).

---

## 1. Structure

A **module** is a list of **functions**. A function has a name,
a list of parameter types, a return type and a list of
**basic blocks**. The first block (`bb0`) is the entry block. A block is
a sequence of **instructions** and exactly **one terminator** at the end.

Every instruction defines at most one value `%n`. Values are numbered
consecutively and are defined exactly once (SSA-like). The parameters of a
function with *n* parameters occupy the values `%0 … %(n-1)`.

Textual form (every line is an instruction, block labels stand in column 1):

```
; FIR v0
fn @name(%0: i32, %1: ptr) -> i32 {
bb0:
  %2 = const.i32 7
  ret %2
}
```

The first line `; FIR v0` is the format tag; `;` does not start a comment
in the instruction syntax, it only occurs in this header line.

---

## 2. Types

| FIR type | Meaning | Width |
|---|---|---|
| `i8 i16 i32 i64` | signed integer | 8/16/32/64 bits |
| `u8 u16 u32 u64` | unsigned integer | 8/16/32/64 bits |
| `bool` | truth value, exclusively 0 or 1 | 8 bits |
| `ptr` | address, untyped in FIR | 64 bits |
| `void` | no value (e.g. `store`, `copymem`, a call without a return) | – |

Mapping of the source types (identical in the type checker, the lowering
and the backend):

* `i8…i64` -> `i8…i64`, `u8…u64` -> `u8…u64`
* `usize` -> `u64`, `isize` -> `i64`
* `bool` -> `bool` (1 byte, only 0/1)
* `*T`, `*mut T` -> `ptr` (the target type disappears; element sizes are
  already resolved into constant byte offsets during lowering)

**Aggregates (structs, arrays) are not FIR values.** They exist only as an
address: `alloca` creates the space, `ptradd` computes field and element
addresses, `load`/`store` access scalar fields, `copymem` copies whole
aggregates. That is why there is no aggregate type and no
aggregate arguments in FIR.

Signedness is a property of the **type**, not of the instruction: `div.i32`
is signed (`idiv`), `div.u32` is not (`div`); `shr.i64` is arithmetic
(`sar`), `shr.u64` logical (`shr`).

---

## 3. Instructions

Notation below: `%d = ` stands for the defined value (absent for
`void` instructions), `T` is the instruction type.

| Textual form | Meaning |
|---|---|
| `%d = const.T c` | constant `c`, truncated to `T` |
| `%d = add.T %a, %b` | addition (wrapping) |
| `%d = sub.T %a, %b` | subtraction |
| `%d = mul.T %a, %b` | multiplication |
| `%d = div.T %a, %b` | division; signedness from `T` |
| `%d = rem.T %a, %b` | remainder; signedness from `T` |
| `%d = and.T %a, %b` | bitwise and |
| `%d = or.T %a, %b` | bitwise or |
| `%d = xor.T %a, %b` | bitwise exclusive or |
| `%d = shl.T %a, %b` | left shift |
| `%d = shr.T %a, %b` | right shift; arithmetic for a signed `T` |
| `%d = cmp.OP.T %a, %b` | comparison, `OP` ∈ `eq ne lt le gt ge`; `T` is the **operand type**, the result is always `bool` |
| `%d = neg.T %a` | arithmetic negation |
| `%d = not.T %a` | bitwise not; with `T = bool` a logical not (0↔1) |
| `%d = cast.FROM.TO %a` | conversion: `FROM` is the source type, `TO` the instruction type. Widening extends according to the signedness of `FROM` (signed: sign-extended, otherwise zero-extended), narrowing truncates. `ptr` counts as a 64-bit value. |
| `%d = alloca.ptr size=N align=A` | `N` bytes of stack memory, aligned to `A` bytes; the result is the address. **Only in the entry block.** |
| `%d = load.T %a` | loads a `T` from the address `%a` |
| `store.T %v, %a` | stores the value `%v` of type `T` at the address `%a` (no result) |
| `%d = ptradd.ptr %b, %o` | address `%b` + `%o` **bytes** (`%o` is `i64` or `u64`) |
| `%d = call.T @f(%a, …)` | call; `T` is the return type, `void` for a call without a return value (then without `%d = `) |
| `%d = syscall.i64 %nr, %a1, …` | Linux syscall: the first argument is the number, then up to 6 arguments, all `i64`; the result is `i64` |
| `copymem %dst, %src, size=N` | copies `N` bytes from `%src` to `%dst` (no result, no overlap) |

**Pure/impure:** `const`, `add`…`shr`, `cmp`, `neg`, `not`, `cast`,
`ptradd`, `load` and `alloca` are pure — the optimizer may remove them if
their result is unused. `store`, `call`, `syscall` and `copymem` have
side effects and always stay (`Op::is_pure` in `fir.rs`).

## 4. Terminators

| Textual form | Meaning |
|---|---|
| `br bbN` | unconditional jump |
| `brcond %c, bbT, bbF` | jump to `bbT` if `%c` (type `bool`) is not zero, otherwise to `bbF` |
| `ret %v` | return with a value |
| `ret` | return without a value |
| `<unset>` | **must not occur after the lowering** — build state only |

---

## 5. Invariants

These are the promises the lowering makes to the optimizer and the backend;
the optimizer preserves them. `lower.rs` checks (1) and (2) itself at the
end and reports a compiler error instead of silently passing on something
broken.

1. **Exactly one terminator per block**, always at the end. No
   `Term::Unset`. Unreachable code after a `return` ends up in a new block
   that is properly terminated as well (`ret` with `const 0` resp. `ret`
   for `void`).
2. **All `alloca` stand in the entry block `bb0`**, before the first
   non-`alloca` instruction. That makes the stack frame statically
   computable in the backend; `alloca`s from deep blocks (e.g. the result
   slot of an `&&`) migrate there as well. That is why the value ids in the
   entry block are not necessarily ascending.
3. **Every value is defined exactly once** and before every use — with
   the exception of back edges, where only values from dominating blocks
   are used. Values never merge anew across a block boundary (see 4).
4. **Phi nodes — since ROUND 92, and only in the middle.** The LOWERING is
   unchanged: every local variable and every parameter still gets an
   `alloca` slot of its own, reading is a `load`, writing is a `store`. So
   `--emit=fir-raw` is phi-free, the lowering needs neither dominance
   computation nor SSA construction, and `tools/fir_compare.sh` can go on
   comparing that text octet for octet against the compiler written in Firn.

   What changed is what happens afterwards. `mem2reg.rs` now does the real
   SSA construction (dominator tree, dominance frontiers, renaming — Cytron
   et al.) and writes `Op::Phi` where two paths bring two different values
   to the same block. That is what a variable written SEVERAL times needs;
   before round 92 only cells written exactly once could be resolved, so
   every loop counter stayed in memory and nothing above the backend could
   reason about it.

   `Op::Phi` never reaches a code generator: `phi.rs` turns every phi back
   into copies at the ends of its predecessors (`Op::Copy`) as the last step
   before code generation. The invariants — phis at the front of their
   block, one entry per distinct predecessor, sorted by block number — are
   checked by `Func::verify_phis` in every build.

   See `docs/ROUND92.md`.
5. **Type fidelity:** both operands of a binary operation have the type of
   the instruction (exception: the shift amount is brought to the type of
   the left operand during lowering); both operands of a `cmp` have the
   operand type named in the instruction; `store.T`/`load.T` match the
   width of the stored value; addresses are always `ptr`; a `bool` contains
   only 0 or 1.
6. **Block numbers are indices**: `bbN` is the `N`-th block of the
   function. Jump targets are always valid blocks of the same function.

---

## 6. How the AST is mapped to FIR

* **Variables/parameters:** an `alloca` in the entry block; parameters are
  stored there from their value ids (`%0…`) into their slot. Every access
  is a `load`/`store`.
* **lvalues** become address computations:
  * an identifier -> the slot address,
  * `a.f` -> `ptradd base, const OFFSET` (the offset from the struct
    layout; offset 0 produces no `ptradd`),
  * `a[i]` -> cast the index to `u64`, `mul.u64` with the element size,
    `ptradd`,
  * `*p` -> the pointer value itself.
* **`if`/`else`, `while`** become basic blocks with `brcond`/`br`.
* **`&&`, `||`** are resolved with **short-circuiting**: an `alloca` slot of
  type `bool` takes the result, the left operand lands in the slot and
  controls a `brcond`; only in the "still has to be checked" branch is the
  right operand evaluated and does it overwrite the slot. **No** `and.bool`
  or `or.bool` instruction comes into being.
* **Struct/array literals** are written field by field resp. element by
  element into their target slot; the assignment of a whole aggregate
  (`let p2: Point = p1;`) becomes a `copymem`.
* **`as`** becomes a `cast`. Special case: `x as bool` is lowered as a
  `cmp.ne` against 0, so that a `bool` is guaranteed to contain only 0/1.
  `bool as iN` is a `cast` (zero-extended).
* **`const` declarations** are evaluated at compile time and appear
  as a `const` instruction at the place of use.
* **`syscall(...)`** becomes a `syscall.i64`; every argument is extended to
  `i64` beforehand (a signed source: sign-extended, otherwise
  zero-extended).

---

## 7. A complete example

Source program:

```
struct Point {
    x: i32,
    y: i32,
}

const LIMIT: i32 = 10;

fn summe(n: i32) -> i32 {
    var s: i32 = 0;
    var i: i32 = 1;
    while i <= n {
        s = s + i;
        i = i + 1;
    }
    return s;
}

fn main() -> i32 {
    var p: Point = Point{ x: 3, y: 4, };
    p.y = summe(LIMIT);
    if p.x > 0 && p.y > 0 {
        return p.x + p.y;
    }
    return 0;
}
```

The corresponding output of `--emit=fir-raw` (unoptimized, copied
verbatim; the test `lower::tests::doc_example_matches` compares this block
with what the lowering really produces). `+` shows up as `checked_add`, not
plain `add` (round 72, SPEC section 13, `L9`) -- the default build level
(`dev-fast`) checks integer arithmetic, and `Op::CheckedBin` carries its own
panic message text right in the FIR, ready for the backend to jump to on
overflow without asking `lower.rs` again:

```firdump
; FIR v0
fn @sum(%0: i32) -> i32 {
bb0:
  %1 = alloca.ptr size=4 align=4
  %2 = alloca.ptr size=4 align=4
  %4 = alloca.ptr size=4 align=4
  store.i32 %0, %1
  %3 = const.i32 0
  store.i32 %3, %2
  %5 = const.i32 1
  store.i32 %5, %4
  br bb1
bb1:
  %6 = load.i32 %4
  %7 = load.i32 %1
  %8 = cmp.le.i32 %6, %7
  brcond %8, bb2, bb3
bb2:
  %9 = load.i32 %2
  %10 = load.i32 %4
  %11 = checked_add.i32 %9, %10 "panic: integer overflow in 'i32 + i32' at test:1:1"
  store.i32 %11, %2
  %12 = load.i32 %4
  %13 = const.i32 1
  %14 = checked_add.i32 %12, %13 "panic: integer overflow in 'i32 + i32' at test:1:1"
  store.i32 %14, %4
  br bb1
bb3:
  %15 = load.i32 %2
  ret %15
bb4:
  %16 = const.i32 0
  ret %16
}
fn @main() -> i32 {
bb0:
  %0 = alloca.ptr size=8 align=4
  %9 = alloca.ptr size=1 align=1
  %1 = const.i32 3
  store.i32 %1, %0
  %2 = const.i64 4
  %3 = ptradd.ptr %0, %2
  %4 = const.i32 4
  store.i32 %4, %3
  %5 = const.i64 4
  %6 = ptradd.ptr %0, %5
  %7 = const.i32 10
  %8 = call.i32 @sum(%7)
  store.i32 %8, %6
  %10 = load.i32 %0
  %11 = const.i32 0
  %12 = cmp.gt.i32 %10, %11
  store.bool %12, %9
  brcond %12, bb1, bb2
bb1:
  %13 = const.i64 4
  %14 = ptradd.ptr %0, %13
  %15 = load.i32 %14
  %16 = const.i32 0
  %17 = cmp.gt.i32 %15, %16
  store.bool %17, %9
  br bb2
bb2:
  %18 = load.bool %9
  brcond %18, bb3, bb4
bb3:
  %19 = load.i32 %0
  %20 = const.i64 4
  %21 = ptradd.ptr %0, %20
  %22 = load.i32 %21
  %23 = checked_add.i32 %19, %22 "panic: integer overflow in 'i32 + i32' at test:1:1"
  ret %23
bb4:
  br bb5
bb5:
  %24 = const.i32 0
  ret %24
bb6:
  br bb5
bb7:
  %25 = const.i32 0
  ret %25
}
```

Line by line, the most important points:

* `@sum` has one parameter: `%0` is the passed value, `%1` its
  stack slot. The three `alloca` (`%1` the parameter, `%2` = `s`, `%4` =
  `i`) stand at the front of the entry block as prescribed — that `%3`
  (the constant `0`) has a smaller id than `%4` is a consequence of
  invariant 2.
* The `while` loop has three blocks: `bb1` the condition, `bb2` the body
  (which jumps back with `br bb1`), `bb3` afterwards. `bb4` is the
  unreachable block behind the `return`; it is terminated anyway
  (invariant 1) and is removed by the optimizer.
* In `@main`, `%0` is the 8-byte `Point`; `x` lies at offset 0 (hence
  `store.i32 %1, %0` without a `ptradd`), `y` at offset 4.
* `%9` is the result slot of the `&&`. `bb1` evaluates the right operand
  only if the left one was true — a real short circuit.
* `LIMIT` appears as `%7 = const.i32 10`: constants are substituted during
  lowering.
* `bb4`, `bb6` and `bb7` are unreachable resp. empty. Exactly such blocks
  are cleared away by dead code elimination — easy to see when comparing
  `--emit=fir-raw` with `--emit=fir-opt`.

---

## 8. What FIR does not have (yet)

Deliberately left out in stage 0 (phi nodes and SSA construction came in
round 92 and are described in section 1 item 4):
aggregates as values, function pointers/indirect calls, global variables
(only `const`, and those are substituted), alias information, loop
information (dominator tree/loop detection), debug metadata, call
attributes, floating point types and vector types. The instruction set is
chosen so that it leaves room for these extensions without existing parts
having to be rewritten.

---

## 9. From FIR to x86_64 code

The backend (`compiler/src/codegen_x86.rs`) translates FIR directly into
GNU assembler text (Intel syntax) — without LLVM, without Cranelift,
without C. The mapping is deliberately simple and thereby checkable:

* Every FIR value `%n` gets an 8-byte stack slot of its own; computation
  happens in `rax`/`rcx` (`rdx` for division/remainder, `rdi`/`rsi`/`rcx`
  for `copymem`). Because of that, no value ever lives in a register across
  a `call` and the callee-saved registers `rbx`, `r12`-`r15` are never
  touched.
* An `alloca` becomes an area in the stack frame; the instruction itself is
  a `lea` of the address into the slot. That is why all `alloca` have to
  stand in the entry block (invariant 2) — the frame is thereby statically
  sized.
* A basic block `bbN` of `@f` becomes the label `.Lf__bbN`; `br` is a
  `jmp`, `brcond` is `test al, al` + `jnz`/`jmp`, `ret` is the epilogue +
  `ret`.
* The frame is always a multiple of 16 bytes. On entry
  `rsp % 16 == 8` holds, and `push rbp` evens that out — so the stack is
  16-aligned at every call site (System V AMD64).
* Signedness comes from the FIR type: `div.iN` -> `idiv`, `div.uN` ->
  `div`, `shr.iN` -> `sar`, `shr.uN` -> `shr`, `cmp.lt.iN` -> `setl`,
  `cmp.lt.uN` -> `setb`. A `cast` becomes `movsx`/`movsxd`/`movzx`/`mov`.
* `syscall.i64` puts argument 0 into `rax` and the remaining ones into
  `rdi, rsi, rdx, r10, r8, r9` — the Linux ABI, not the call ABI.
* `_start` calls `main` and passes `eax` to `exit` (freestanding, without
  libc).

To be seen with `firnc --emit=asm file.fi -o file.s` resp. `--keep-asm`.

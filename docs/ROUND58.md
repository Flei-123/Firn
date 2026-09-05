# Round 58 — functions become values

**Branch:** `r58-closures` · **Base:** `04bb780` (main after round 57)
**Reserved for this round:** FIR opcodes 50–59 (53 used), state block slots
2200–2299 (**none used** — see §7), test numbers 870–889.

Until round 57 `compiler/src/types.rs` had no `Type::Fn`. A function could be
called, and that was all it could do. That one gap cost more than it looks:
no callbacks, no event handlers, no sorting with an ordering of your own, no
iterator with a step of your own. The two places where the compiler itself
ran into it are still in the source and say so plainly — the finalizer
dispatcher (`compiler/src/gc.rs`, round 47: *"stage 0 has no function
pointers; a dispatcher function with a tag is the honest equivalent"*) and
the thread dispatcher (round 49, same sentence). `lib/firnc1/types.fi` says
it too: *"A function pointer instead of an import in the other direction
would be cleaner, but Firn has none."*

This round closes the gap in both compilers.

---

## 1. The representation

A value of type `fn(A, B) -> R` is **one machine word**: the address of a
**function record**.

```text
record:  [0]      address of the machine code
         [8+8*k]  the k-th captured value of a closure
```

Three kinds of record, one shape:

| what | record lies in | costs |
|---|---|---|
| named function taken as a value | `.rodata`, one word | nothing, no allocation |
| closure without captures | `.rodata`, one word | nothing, no allocation |
| closure with captures | GC heap, `8 + 8·n` bytes | one allocation, may fail |

The label of a static record is `.L__fnv.<symbol>`; it is file local and
never appears in the symbol table.

### Why one word and not two

The obvious alternative is the shape of `dyn I` (round 46): two words,
`{code, environment}`. It was rejected. Two words make the fn value an
**aggregate**: it no longer fits in one register, it needs `sret` on return
(SPEC §14.1 returns everything above 8 bytes through the hidden pointer),
and every struct that holds one grows by 8 bytes. One word costs one extra
load per indirect call — and that load hits `.rodata` or a heap block that
the caller has just touched anyway.

### The call

```text
%c = load.ptr [%f]              ; the code address out of word 0
%r = calli.R  %c(a, b, %f)      ; the record goes in as the LAST argument
```

The record travels as the **last** argument. That is the point on which the
whole design hangs: a named function needs no shim, because System V lets
the caller pass one argument too many and a callee that does not know about
it never reads it. A closure body is translated as an ordinary function
whose last parameter is the record — that is where it reads its captured
values from.

**A direct call stays direct.** `add(1, 2)` is still `Op::Call` and thus
`call add`. The indirection arises only where the target really sits in a
value. That is not a claim, it is measured: `tools/fnval/run.sh` reads the
emitted assembly of **both** compilers and checks the call sites, with two
counter-checks that have to strike.

---

## 2. The syntax

```firn
fn(i32, i32) -> i32        // the type
fn(i32)                    // ... without a result
fn(a: i32) -> i32 { … }    // the closure literal, captures nothing
gc fn(a: i32) -> i32 { … } // ... captures, record in the GC heap
```

`fn` in a type position can mean nothing else; a declaration always carries
a name after `fn`. So no new keyword and no ambiguity.

`gc` in front of the literal is not decoration. It is the same `gc` as in
`gc C{ … }` and it says the same thing: **here a GC object comes into
being.** A capturing closure therefore has the type
`AllocError!fn(A) -> R` and needs a `try`, exactly like `gc C{ … }` has the
type `AllocError!Gc[C]`. Whoever allocates sees it at the place where it
happens (SPEC §1, guiding principle 1).

Both directions are errors, and both name the way out:

```text
this closure captures 'n' and therefore needs storage
  note: write 'gc fn(…)' — the record then lies in the GC heap and the
        result is an AllocError!fn(…)

this closure captures nothing and needs no GC record
  note: write it without the 'gc' in front
```

---

## 3. The capture decision: **by value**, and why

A closure captures a **copy** of the value, taken at the moment the closure
is created. The alternative — a pointer to the variable in the enclosing
frame — was rejected, and not on taste:

1. **Firn has no lifetimes and no borrow checker.** A closure that outlives
   its creating frame is the normal case (`fn adder(n: i32) -> …` returns
   one). A captured reference into that frame would be a dangling pointer,
   and nothing in the language would catch it. Capture by value makes that
   class of bug impossible by construction.
2. **The collector has to reach the captured values.** A copy in a GC object
   is traced through the ordinary type table (§4). A pointer into a dead
   frame is not traceable at all.
3. **The cost is visible.** The record size is a compile-time constant,
   `8 + 8·n`. With capture by reference the cost would depend on what the
   enclosing frame does afterwards.

The consequences are stated rather than hidden:

* Writing to a captured value inside the closure is an **error**, not a
  silent write to a copy:
  `'n' is captured by value and cannot be assigned inside the closure`,
  with the note *"capture a pointer or a Gc[T] if the change is meant to be
  visible outside"*. That is the escape hatch: a captured `*mut i32` or a
  captured `Gc[Cell]` gives shared mutation, and it says so in the type.
* Captured values are **at most one word**. An aggregate would make the
  record a variable-sized thing that the collector could no longer describe
  by a fixed offset list. `a closure cannot capture 'p' of type Point
  (8 octets)` names the variable, the type and the size.
* Which values are captured is worked out by the **type checker**, not by
  the parser: only there is it known which names belong to the enclosing
  function. Every use of a name inside a closure body runs through
  `Checker::note_use` (`compiler/src/sema.rs`), and every name found below
  the closure's own scopes is captured once, in order of first use.

---

## 4. The dangerous place: the captured value as a GC root

This is where the round could have gone quietly wrong. A closure that
captures a `Gc[T]` holds the only reference to that object once its creating
frame has been left. If the collector does not see the captured value, the
object is swept while the closure is still using it — and the damage shows
up much later, somewhere else.

**How it is prevented.** The capture record **is** an ordinary GC object of a
synthesised class (`gc.rs::declare_capture_class`, in Firn
`gc.fi::gc_capture_class`):

```text
gc class __capture#N {
    __code: u64,     // the code address — NOT traced, it points into .text
    __c0: T0,        // the captured values
    __c1: T1,
    …
}
```

The class is registered with layout, type tag and traced offsets, and lands
in the type table of the collector like any other. So the chain

```text
stack (the fn value) ──► capture record ──► Gc[Cell] ──► …
```

is traced **precisely**, with no special path, no pinning and no external
root. The record pointer itself is one plain word on the stack and is found
by the conservative stack scan (SPEC §3.5.3).

Why the class has to be built during the **type check** and not with the
other classes: the ordinary path (`declare_classes`/`layout_classes`) runs
over the items and is long finished by the time a closure literal is
reached. Only there are the types of the captured values known.

### The proof, and the counter-check

`tests/873_closure_gc_root.fi` creates the object in a frame that is then
left, **scrubs** that frame (so the conservative stack scan cannot keep the
object alive by accident), collects repeatedly and allocates 300 objects
between collections — a freed block lands on the free list and is handed out
again straight away, so a dead captured object would have its content
overwritten. Only then does it read through the closure. A second closure
captures the head of a ten-element chain, so the tracing has to be
transitive.

Measured, on this round's build:

| build | `tests/873_closure_gc_root.fi` |
|---|---|
| with the traced offsets | exit **0** |
| counter-check: `strong_offs` deliberately left empty | exit **2** |

Exit 2 is the read through the closure returning something other than 4711.
The test therefore really measures the tracing and not an accident.

---

## 5. What it is used for

`lib/std/vec.fi` (= `lib/rt/vec.fi`) gets the ordering and the predicate as a
value:

```firn
vec_sort_by[T](v, less: fn(*T, *T) -> bool)
vec_is_sorted_by[T](v, less: fn(*T, *T) -> bool) -> bool
vec_min_by[T](v, less) -> usize
vec_max_by[T](v, less) -> usize
vec_index_where[T](v, pred: fn(*T) -> bool) -> usize
vec_count_where[T](v, pred) -> usize
vec_all[T](v, pred) -> bool
vec_any[T](v, pred) -> bool
```

`vec_sort[T: Ord]` from round 50 takes its ordering off the **type**. That is
right where a type has exactly one natural order and wrong as soon as the
same data has to be sorted twice differently — and the only way out was to
copy the sorter. `tests/874_vec_sort_by.fi` sorts the same `Vec[Point]` by
`x`, by `y` and descending by `x` with **one** sorter, and once with an
ordering written on the spot as an anonymous function.

The price is stated where it is paid: the comparison inside the sorting loop
is one indirect call, one more than with `T: Ord`, which resolves statically
(`tools/bounds/run.sh` measures that separately).

---

## 6. Both compilers

The fixpoint only holds if `lib/firnc1` can do everything `compiler/src` can.
The port is complete:

| firnc0 (Rust) | firnc1 (Firn) |
|---|---|
| `Type::Fn { params, ret }` | `K_FN` (17) with its own parameter table `ft_off/ft_len/ft_par` in `types.fi` |
| `TypeExpr::Fn` | `T_FN` (3), parameters in `children`, packed as `(off << 32) \| count` |
| `ExprKind::Lambda(LambdaDecl)` | `E_LAMBDA` (14) plus the generated tree function `__closure#N` |
| `Op::FnRef { name }` | `O_FNREF` (**53**) |
| `fnval.rs` (registry, parser hook, type check, lowering) | spread over `parser.fi`, `sema.fi`, `lower.fi`, `codegen.fi` |
| `gc.rs::declare_capture_class` | `gc.fi::gc_capture_class` |

Two structural differences, both deliberate:

* **firnc0** keeps the closure body inside the expression and pulls it out
  into a function of its own only at lowering time. **firnc1** builds that
  function in the parser straight away and the type checker skips it in the
  top-level loop — its body is checked at the literal, where the captured
  names are still in scope. Both produce the same canonical AST:
  `tools/parser_compare.sh` compares `(closure plain ((param a i32)) i32
  (blk …))` and finds no difference. The serial number of the generated
  function is deliberately **not** in the rendering: the two compilers number
  differently, and the number says nothing about the tree.
* The generated functions carry a `#` in their name, so no source text can
  ever write them. The assembler does not accept the character, so it becomes
  a dot in the symbol (`_F0.__closure.0`). No other name in either compiler
  contains a `#` at a place where a symbol arises from it.

---

## 7. Reserved numbers

* **FIR opcodes 50–59:** 50, 51, 52 were already taken (`O_ASM`, `O_MMIOLD`,
  `O_MMIOST`, round 52). This round takes **53** (`O_FNREF`) and nothing
  else; 54–59 stay free.
* **State block slots 2200–2299:** **not needed.** The capture record is an
  ordinary GC object and needs no new state in the collector — the last slot
  in use is still `S_ZBUDGET` at 2136.
* **Test numbers 870–889:** 870–875 positive, 876–889 negative.

---

## 8. What does NOT work (honestly)

1. **A function type as a type ARGUMENT of a template.**
   `Vec[fn(i32) -> i32]` does not compile: `size_of[T]()` inside the template
   carries the type parameter in the call name (`size_of$T`, see
   `sizeof.rs`), and that substitution only knows named types. The workaround
   is a wrapper struct and it is in `tests/870_fnval_core.fi`:
   `struct Slot { f: fn(i32, i32) -> i32 }`, then `Vec[Slot]`. A function
   type as the type of a **field**, of a **parameter**, of an **array
   element** and as a **result** works everywhere.
2. **A closure literal inside a generic function.** Refused with a clear
   message (`tests/neg/876_closure_in_template.fi`), because the literal
   would be copied once per instantiation and would need one capture record
   and one generated function per copy. Function **pointers** work in a
   template without any restriction — `vec_sort_by[T]` is exactly that.
3. **Calling a function value directly out of a field or an element.**
   `s.f(1, 2)` is a method call, `v[i](1, 2)` is not parsed as a call.
   Bind it first: `let f = s.f`, then `f(1, 2)`. The tests do it that way.
4. **Capturing more than one word.** See §3. An aggregate goes in through a
   pointer or a `Gc[T]`.
5. **Ordering comparisons on function values.** `==` and `!=` work (a pointer
   comparison, "the same function record"); `<` and friends are refused —
   addresses have no meaningful order.
6. **`vec_sort_by` is not stable.** Same as `vec_sort`: it is the same
   heapsort.
7. **A closure literal cannot recurse into itself.** It has no name at the
   place where it is written. Named functions can.

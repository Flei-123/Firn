# Round 45: methods — `impl`

**Base: `f48e51c` (main after round 42).** Branch `r45-impl`.

Up to this round Firn had free functions only. The
standard library from round 42 reads accordingly:

```firn
bytes_push(&b, 65 as u8)
let n: usize = length(trim(source))
str16_set(&s, 0, 73 as u16)
```

The prefix is the type — only written down by hand and not checked by the
compiler. This round turns that into:

```firn
b.add(65 as u8)
let n: usize = source.trim().length()
s.set_at(0, 73 as u16)
```

---

## 1. The one design decision: `impl` is a writing aid

A method is **not a new kind of thing** in Firn. `impl` has exactly two
effects:

1. `impl T { fn m(*mut self, x: i32) … }` creates the **ordinary function**
   `T__m(self: *mut T, x: i32)`. Afterwards it stands in `Program::funcs`
   like any other.
2. `x.m(a)` becomes a call to this function. Which one is meant is decided
   **solely by the static type of the receiver**.

No dynamic dispatch, no vtable, no lookup at runtime. If the type is fixed,
the jump instruction is fixed. (Interfaces and dynamic dispatch are
explicitly **not** part of this round.)

The gain of this construction is not convenience but **reach at
zero risk for everything downstream**: monomorphization, `#[must_consume]`
via the type, calling convention, register allocation, optimizer, debug
info and the symbol plan have a perfectly ordinary function in front of
them after parsing and **did not have to be touched**. The entire language
change sits in the parser, the type checker and the one argument in the
lowering.

---

## 2. The receiver: `*self`, not `&self`

The assignment suggested `self` / `&self` / `&mut self`. That does not fit
this language, and for one reason:

> **Firn has no references.** It has pointers (`*T`, `*mut T`) and the
> address operator `&x`. A `&self` would be a concept that does not occur
> anywhere else in the language — `&` here means „address of", not
> „reference to".

The receiver is therefore written the way every other parameter is written
in Firn:

| Notation | type of `self` | intended for |
|---|---|---|
| `fn m(self)` | `T` | small values, views, pure computations |
| `fn m(*self)` | `*T` | reading methods on owning buffers |
| `fn m(*mut self)` | `*mut T` | modifying methods |

At the call site there is exactly **one** adjustment — the one you would
otherwise write by hand:

* the method wants a pointer, the receiver is present as a **value** → the
  compiler takes its address (`&x`).
* the method wants a pointer, the receiver **already is a pointer** → it is
  passed through.

There is no more automatism than that:

* **No automatic dereferencing.** If the method wants a copy and a pointer
  is present, that is an error with a suggestion: `(*z).erstes()`.
  (`tests/neg/impl_receiver_is_ptr.fi`)
* **No silent intermediate.** `p.moved(1).sum()` is an error
  if `sum` wants a pointer: the result of a call has no
  address. Bind first, then call. A silent intermediate value would be the
  more dangerous choice — changes to it would arrive nowhere.
  (`tests/neg/impl_receiver_without_address.fi`)
* **No chain over several levels**, no `**p`, no way through fields.

### Honestly named: `*self` and `*mut self` check the same thing today

`sema::compatible` compares pointers **without** mutability — `*T` and
`*mut T` are interchangeable in this language (that holds for every
parameter, not only for the receiver). `*mut self` therefore says exactly
as much today as `*mut T` on an ordinary parameter: it is **intent,
recorded in the source text**, not a constraint enforced by the compiler.
If the mutability of pointers is checked one day, the check applies to the
receiver as well without another line in this round.

The address operator itself was likewise taken over unchanged: `&x` demands
a **place** (variable, field, array element, `*p`) and yields `*mut`. The
receiver check uses exactly the same set — there is no second, deviating
rule for methods.

---

## 3. How a method call runs through both compilers

Until the type check, the call carries the name `"methode m"`. The space
makes it a name that cannot arise from any identifier of the source text —
the same construction as `"gc C"` in `gc.rs`.

| Phase | what happens | `firnc0` | `firnc1` |
|---|---|---|---|
| parser, item | `impl T { … }` → functions `T__m` | `impls.rs::hook_item` | `parser.fi::impl_deklaration` |
| parser, expression | `x.m(a)` → `Aufruf("methode m", [x, a])` | `impls.rs::hook_methodenaufruf` | `parser.fi::nach_ausdruck` |
| type checker | receiver type → struct name → `T__m`, adjust the receiver, check the arguments | `impls.rs::hook_call` | `sema.fi::methoden_ruf` |
| type checker, `probe` | return type without reporting (otherwise `p.sum() != 42` gets no literal type) | `sema.rs::probe_d` | `sema.fi::probe_t` |
| lowering | derive the target **anew**, receiver as an address if needed | `lower.rs::lower_call` | `lower.fi::ruf_voll` |

**Why no side table between the type checker and the lowering?** Because
there is nothing to transfer: the receiver type and the method name are
available in both phases, and the derivation is pure name arithmetic. A
table would have to be carried along in `firnc1` and kept valid across
monomorphization — effort for a piece of information the type already
gives. In `firnc0` the derivation therefore sits in **one** place
(`impls::ziel_von`), used jointly by `probe`, `call` and the lowering; they
cannot drift apart.

---

## 4. Name resolution: there is nothing to shadow

Methods and free functions lie in **one** namespace, but under
**different names**: the method `m` of `T` is called `T__m`.

* `m(x)` never finds a method.
* `x.m()` never finds a free function — not even when its first
  parameter is exactly the matching pointer
  (`tests/neg/impl_free_func_is_no_method.fi`).
* They may therefore have the same name. `tests/810` and
  `tests/modules/geo.fi` demonstrate it: `sum(a, b)` and `Dot.sum()`,
  `unit()` and `Rect.unit()` stand next to each other. In
  `lib/str/std_impl.fi` exactly that happens in earnest: the free function
  `length(s)` and the method `Span.length()` are both reachable and point
  at the same code.

**Modules.** In the module `geo`, `Rect` becomes `geo__Rect` when
merging, and `Rect__area` accordingly becomes
`geo__Rect__area`. The resolution "struct name ++ `__` ++ method"
still holds afterwards, because it always computes with the name the type
carries at that point in time. The call `r.area()` therefore needs
**no** module prefix: the method belongs to the type, and the type already
carries its module in its name.

**Visibility.** A method follows the type, not the `export` list: whoever
sees the type sees its methods. That is deliberate — a method that hangs on
the type but may not be called would be a second visibility rule next to
the existing one.

**Two `impl` blocks for the same type are allowed** (`tests/modules/geo.fi`
does it): `impl` creates functions, not a closed table. Two methods of the
same name on the same type are the same error as two functions of the same
name — „funktion 'T__m' ist bereits deklariert".

---

## 5. What was changed in `firnc0`

| File | Lines | what |
|---|---|---|
| `compiler/src/impls.rs` | **490 new** | everything specific to this round: parser hooks, receiver forms, resolution, error messages, `ziel_von` |
| `compiler/src/parser.rs` | +15/-1 | two hooks (item level, postfix); `cont` becomes `pub(crate)` |
| `compiler/src/sema.rs` | +57/-24 | hook in `call`, branch in `probe_d`, `pruefe_argument` extracted |
| `compiler/src/lower.rs` | +32/-2 | derive the target anew, receiver as an address |
| `compiler/src/nogc.rs` | +17 | a method call in the `#[no_gc]` tree is rejected |
| `compiler/src/main.rs` | +1 | `mod impls;` |

`pruefe_argument` is **literally** the previous loop body from `call` —
the messages of ordinary calls are unchanged, only the number in the text
counts without the receiver for methods (`v.push(x)` has **one** argument).

## 6. What was changed in `firnc1`

| File | Lines | what |
|---|---|---|
| `lib/firnc1/parser.fi` | +225/-25 | `impl` block, receiver, `ruf_argumente_in`, `methodenrufname`, pre-scan |
| `lib/firnc1/sema.fi` | +159 | `methoden_ruf`, `empf_struct`, `methodenfunktion`, `ist_platz`, branch in `probe_t` |
| `lib/firnc1/lower.fi` | +63 | the same derivation, receiver as an address |
| `lib/firnc1/nogc.fi` | +13 | the same rejection as in `nogc.rs` |
| `lib/rt/intern.fi` | +26/-1 | `intern_praefix`, `intern_ab` |

`intern_ab` (the name from byte *n* on, i.e. `"methode m"` → `"m"`) copies
the text **out first**: `intern_nummer` can make the text buffer move, and
then `intern_zeiger` pointed into the void. That is stated at the head of
`intern.fi` already and is really relevant here for the first time.

---

## 7. Two findings from the build

### A. The parenthesis on the next line

The first build of `firnc1` failed — at this spot in its own
source text:

```firn
let gemerkt: usize = (*p).kein_slit
(*p).kein_slit = 0
```

After `.kein_slit` the new hook saw an opening parenthesis and made a
method call `(*p).kein_slit((*p))` out of it. But the parenthesis was on
the **next line** — and in Firn the line break ends the statement
(SPEC §10, semicolon optional).

The check for that has existed for a long time: `Parser::cont` (in
`firnc1`: `weiter`) answers exactly the question „may the expression
continue with this token?". It stood in the postfix part before the loop,
not before the parenthesis. Fixed in both compilers with one condition.

**The lesson:** a new postfix form has to answer the same line rule
as all the others. The bug was not found by a test but
by the compiler compiling itself — the self-build is the sharper test at
such spots, because it holds 18 000 lines of real source text against the
new rule.

### B. Two routes to module renaming that drifted apart

`firnc0` renames module names **after** parsing (`modules.rs`), `firnc1`
**during** parsing (`parser.fi`). For that, `firnc1` scans in advance for
all names declared by the module itself — a token pass that watches for
`fn`/`struct`/`const` + identifier. In an `impl` block it finds
`fn push` as well and would have taken `push` into the renaming list of the module;
`firnc0` sees only the finished function `Bytes__push` there and never
`push`.

The consequence would have been: a module that has a method `push` **and**
a free call `push(..)` somewhere would have resolved different names in the
two compilers — exactly the sort of deviation that only comes to light in
the self-comparison. The pre-scan now skips `impl` blocks entirely
(`ist_impl_bei`/`impl_ende`).

---

## 8. The application example: `std.str` gets a wrapper

`lib/str/std_impl.fi` (102 lines, included only by
`tools/strlib/src/std_str.fi`, generates `lib/std/str.fi`) gives three
types methods:

* **`Span`** -- the reading view, receiver by value: `length`, `is_empty`,
  `chars`, `part`, `ab`, `to`, `equal`, `compare`, `equal_without_case`,
  `starts_with`, `ends_with`, `find`, `find_back`, `find_char`,
  `contains`, `count_char`, `count_part`, `trim`, `trim_left`,
  `trim_right`, `without_prefix`, `without_suffix`, `utf8_char`, `utf8_part`
* **`Bytes`** -- the owning buffer, `*self` resp. `*mut self`: `length`,
  `at`, `is_text`, `equal`, `span`, `write`, `add`, `clear`,
  `free`, `append`, `set`, `repeat`, `big_here`, `small_here`
* **`Str16`**: `length`, `at`, `equal`, `add`, `add_cp`, `set_at`,
  `clear`, `free`

**Wrappers exclusively.** Every method calls exactly the free function that
already exists, and does nothing else. That makes the conversion
demonstrably behavior-preserving — there is no second implementation that
could drift apart. The free functions remain reachable unchanged and
continue to be used by the library itself.

`tests/812_impl_std_core.fi` computes both ways **side by side**
(`core.finde(welt) != str.finde(kern, welt)` → return 12) and prints a line
at the end that `firnc0` and the self-compiled `firnc1` must produce
character-identically.

---

## 9. Test coverage

**`tests/810_impl_core.fi`** — 21 checked points, each with its own
return value: the three receivers; address taking for value receivers; a
receiver that is already a pointer; `(*z)` for the explicit copy;
aggregate return and binding beforehand; method calls method; aggregate as
an argument; receiver as a **field** (`r.corner.sum()`), as an **array
element** (`field[i].set(..)`) and nested (`r.corner_sum()` calls
`(*self).corner.sum()`); free function and method with the same name.

**`tests/811_impl_module_core.fi`** with **`tests/modules/geo.fi`** — 12
points across the module boundary: method without a module prefix, a
writing method that itself calls a method, a second `impl` block for the
same type, the free function `unit()` next to `Rect.unit()`,
a receiver via a pointer.

**`tests/812_impl_std_core.fi`** — 24 points on `std.str`, see above.

**`tests/neg/impl_*.fi`** — eight negative tests, each with a position,
message and error count:

| File | Message |
|---|---|
| `impl_no_method.fi` | `type 'Dot' has no method 'diff'` (the hint names the existing ones) |
| `impl_free_func_is_no_method.fi` | the same message for `p.dot_sum()` |
| `impl_receiver_without_address.fi` | `the receiver of 'Dot.sum' needs an address, this expression has none` |
| `impl_receiver_is_ptr.fi` | `'Dot.first' expects the receiver as a value, found *mut Dot` |
| `impl_no_struct.fi` | `type 'i32' has no method 'sum'` |
| `impl_argument_ty.fi` | `argument 1 of 'Dot.set_x' has type bool, expected i32` |
| `impl_arg_count.fi` | `method 'Dot.set_x' expects 1 argument(s), found 2` |
| `impl_without_receiver.fi` | `the first parameter of a method is the receiver: 'self', '*self' or '*mut self'` |

All eight are rejected by `firnc1` as well (exit 1). Each of them reports
**exactly one** error: a broken method aborts the whole `impl` block, so
that the first one — the only one that explains anything — does not drown
in a cascade.

---

## 10. Deliberately left out

* **Associated functions without `self` (`Typ::neu(..)`).** Part 4 of
  the assignment, „only if time is left". They are not built, and visibly
  so: a method without a receiver is an error with a clear statement
  (`impl_without_receiver.fi`) instead of a clueless syntax error. The call
  would additionally need `Typ::name` as an expression form; `::` is not a
  token of its own in the tokenizer today (`Enum::Variante` is read as two
  `:`).
* **Generic types.** There is no `impl Vec[T]`. After monomorphization an
  instantiation has a different name than the template; the methods would
  have to be monomorphized along with it. That is worth a round of its own
  and is the reason why the application example is `std.str` and not
  `std.vec`.
* **Attributes on methods.** `#[no_gc]` or `#[must_consume]` in front of a
  method is a syntax error; an attribute in front of `impl` is explicitly
  rejected. `#[must_consume]` on the **type** still works, even if a
  method returns it — that check goes via the type, not via the
  name.
* **`#[no_gc]` and methods.** A method call in a `#[no_gc]` function
  is **rejected**. The check runs without the type table of the receivers
  and therefore cannot say which function is behind it. A hole in a
  promise would be worse than a missing convenience; whoever needs both
  calls `Typ__methode(..)` directly.
* **Dynamic dispatch, interfaces, operator overloading.** Not part of this
  round (assignment).
* **Method chains over pointer receivers.** See §2: deliberately no silent
  intermediate value.

---

## 11. Acceptance

Measured on `r45-impl` with a self-built `firnc0` and freshly built
helper binaries (`.firnc1`, `.astdump`, …).

| Check | Base `f48e51c` | now |
|---|---|---|
| `bash ./test.sh` | 673/673 | **690/690** |
| `tools/self_compare.sh` | 196 identical / 0 differing / 0 failing | **199 / 0 / 0** |
| `tools/fixpoint.sh` | stage 2 == stage 3, 309 468 lines | **stage 2 == stage 3, 315 088 lines** |
| parser (`parser_compare.sh`) | 236 identical, 1 known difference | 248 identical, 1 known difference |
| layout/ABI (`types_compare.sh`) | 186 identical, 0 different | 198 identical, 0 different |
| type checker (`sema_compare.sh`) | 145 identical, 1 known | 146 identical, 1 known |
| lowering (`fir_compare.sh`) | 144 identical, 1 known | 145 identical, 1 known |

The one known deviation is unchanged `tests/590_f64.fi` (literal
`1e308`, rounding case from round 20 — not a parser bug).

## 12. Lines

| | Lines |
|---|---|
| `firnc0` (Rust) | +591 / −21 |
| `firnc1` (Firn, with `lib/rt/intern.fi`) | +481 / −5 |
| library (`lib/str/std_impl.fi`, generated `lib/std/str.fi`) | +205 |
| tests (3 programs, 1 module, 8 negative tests) | +543 |

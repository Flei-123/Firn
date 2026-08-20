# Round 50: bounds — generics and interfaces brought together

**Base: `cc1710f` (main after rounds 46/47/48).** Branch `r50-generik`.

Generic templates (`Vec[T]`, `Map[K,V]`) have existed since round 30, and
interfaces with dynamic dispatch (`interface I`, `dyn I`) since round 46.
Between the two lay a gap that one can see in a single line:

```firn
fn vec_sortiere[T: Scalar](v: *mut Vec[T]) { … a < b … }
```

`Scalar` says what **shape** `T` has (integer, `bool`, pointer) — not what
`T` **can do**. The comparison therefore had to be hard-wired, and with that
`Vec[T]` could sort exactly those types for which the compiler knows a `<`.
A `Vec[Person]` could be created and filled, but not sorted.

This round closes the gap: **the name of an interface is a bound.**

```firn
interface Ord {
    fn kleiner(*self, b: *Self) -> bool
}

fn vec_sortiere[T: Ord](v: *mut Vec[T]) { … a.kleiner(b) … }
```

---

## 1. The syntax — and why exactly this one

```text
fn f[T: Ord](…)                 eine Schranke
fn f[T: Scalar + Ord](…)        mehrere, alle gelten gleichzeitig
struct Paar[T: Ord] { … }       auch an einem Typ, nicht nur an einer Funktion
fn f[K: Int, V: Ord](…)         je Parameter eigene Schranken
```

**No new character, no new keyword.** The place after the
colon already existed (`[T: Int]`, round 30); the only new thing is that
**any** name may stand there. `Any`, `Int` and `Scalar` remain the three
built-in bounds, and every other name is the name of an interface.
`+` as a separator is the only ingredient — and `+` cannot mean anything
else at this place, because there is no expression between two type
parameter names.

**The name is NOT resolved during parsing.** `Bound::parse` yields
`Bound::Iface(name)` for every unknown name, without asking whether the
interface exists. That is intentional: `interface Ord` may stand further
down in the same file or in a completely different one, and the parser
always sees only one file. A typo therefore only comes to light at the
instantiation — but with the list of known names (§3).

**The check happens at the INSTANTIATION**, in `mono::bind_params`, i.e.
before the type checker runs. At that point there is neither a struct table
nor resolved types; what there is are names: the registration from
`iface.rs` and the list of all function names of the merged program.
Exactly from that the message is built — and exactly for that reason it can
say **which method is missing**, instead of turning up later as „unknown
method" in the middle of an instantiated copy.

### `Self` — without it there is no ordering

An interface from round 46 knows only concrete parameter types:

```firn
interface Ord { fn kleiner(*self, b: *Punkt) -> bool }   // nur für Punkt
```

An ordering, however, compares **two values of the same type**. That is why
the signature may now name `Self` — the type that implements the interface:

```firn
interface Ord { fn kleiner(*self, b: *Self) -> bool }
impl Ord for Punkt { fn kleiner(*self, b: *Punkt) -> bool { … } }
impl Ord for i32   { fn kleiner(*self, b: *i32)   -> bool { … } }
```

The whole special treatment: a method whose signature names `Self` is
**not resolved globally** but **per implementation** (`resolve_mit_self` in
`iface.rs`, `aufloesen_self` in `iface.fi`). Globally, `Self` would have no
type at all.

**`Self` and `dyn` exclude each other.** Over `dyn I` it is only clear at
runtime which type is behind it; `*Self` would be a different type for each
one, and the caller could not form the argument. Such a call is
therefore an error — with a hint towards the way that does work:

```
error: 'Ord.kleiner' nennt 'Self' und ist deshalb nicht ueber 'dyn Ord' aufrufbar
   = hinweis: rufe sie ueber eine schranke auf: 'fn f[T: Ord](x: *T)' — dort steht der typ fest
```

That is the object safety rule of this language, in one sentence and in one
place. `dyn I` remains permitted unchanged for interfaces without `Self` —
even for the same interface, as long as only its `Self`-free methods are
called via `dyn`.

### `impl I for <base type>`

`vec_sortiere[i32]` has to keep working. So since this round a
**built-in type** may implement an interface as well:

```firn
impl Ord for i32 { fn kleiner(*self, b: *i32) -> bool { return *self < *b } }
```

That creates the ordinary function `i32__kleiner(self: *i32, b: *i32)` —
the same naming scheme as for a struct (round 45). Two consequences:

* **Methods of a base type are valid program-wide.** `modules.rs` does not
  rename them. The type `i32` belongs to no module, so neither do its
  methods; if the method were called `vec__i32__kleiner` in `std.vec`, the
  resolution would still look for `i32__kleiner` and would find nothing.
  `firnc1` always did it that way — `eigene_suchen` skips `impl` blocks —
  and here both compilers are now the same for the same reason.
* **A base type gets no method table.** A table exists only per
  struct implementation, because only a struct can stand behind a `dyn I`
  (`hook_cast` demands a pointer to a struct). `(&n) as dyn Zeigbar`
  with `n: i64` is an error, `tests/neg/bound_dyn_base_ty.fi`.

---

## 2. What follows from that: the dispatch is static

That is the actual gain, and it cost **not a line in the code generator
and no new FIR opcode**. After monomorphization, in

```firn
fn kleineres[T: Ord](a: *T, b: *T) -> *T { if a.kleiner(b) { return a } … }
```

for `T = Punkt` there is an ordinary method call on `*Punkt` — and
`impls.rs` has resolved that from the static type alone since round 45.
There is nothing to dispatch at this place.

```asm
; fn f[T: Ord] (--no-opt)        | ; dyn OrdnungD (--no-opt)
    mov rdi, qword ptr [rbp-40]  |     mov rcx, qword ptr [rbp-64]   ; Tafel
    mov rsi, qword ptr [rbp-48]  |     mov rax, qword ptr [rcx]      ; Eintrag 0
    call _F0.Punkt__kleiner      |     mov qword ptr [rbp-176], rax
                                 |     mov rdi, qword ptr [rbp-136]  ; Datenzeiger
                                 |     mov rsi, qword ptr [rbp-176]
                                 |     mov rax, qword ptr [rbp-168]
                                 |     call rax
```

`tools/bounds/run.sh` records that — and it does so not with the clock but
on the generated code. Two programs, the same work; what is checked is:

| Check | bound | `dyn` |
|---|---|---|
| indirect calls (`call <register>`) in the assembly | **0** | ≥ 1 |
| `lea … .L__iface…` (address of a method table) | **no** | yes |
| `calli` in the FIR | **0** | ≥ 1 |
| `vtab` in the FIR | **0** | ≥ 1 |
| named `call … Punkt__kleiner` | yes | — |

in **three build stages** (`release-fast`, `--no-opt`, `dev-fast`) and in
**both compilers**. The counter-check with `dyn` is part of the test:
without it, it would pass even if it measured nothing at all. The run hangs
in `test.sh` as step 8c (there without the callgrind measurement, so that
it stays short).

### Measured (callgrind, 2.000.000 calls in a loop)

| | instructions total | per iteration |
|---|---|---|
| bound, `release-fast` | 10.000.019 | **5** |
| `dyn`, `release-fast` | 58.000.046 | **29** |
| bound, `--no-opt` | 182.000.106 | **91** |
| `dyn`, `--no-opt` | 208.000.145 | **104** |

Read honestly: the **pure** dispatch costs 13 instructions per call
(`--no-opt`, both sides without inlining — the three loads from
docs/RUNDE46.md §4 plus the indirect jump and the register saving it
entails). With the optimizer the gap opens further,
5 against 29, and **not** because the dispatch gets more expensive, but
because the static call disappears entirely: `inline.rs` inlines it, and
the rest falls victim to constant folding. An indirect jump cannot do that.

That is the same finding as in round 46 (there 5 against 26 with an
interface without an argument) — the new thing is that the static side can
now be written down **with an interface** and not only without one.

---

## 3. The error messages

Every one of them names the line, the column and, in the hint, what to do.
The interesting ones are those that name a method.

**No `impl` — the message says which method would be missing:**

```
error: typ 'Kreis' setzt die schnittstelle 'Ordnung' nicht um — schranke am typparameter 'T' von 'kleineres'
  --> tests/neg/bound_no_impl.fi:28:21
   = hinweis: es fehlt 'fn kleiner(*self, *Self) -> bool' in 'impl Ordnung for Kreis { … }'
```

**The type already has a part — then only the rest is listed.** `Punkt` has
`kleiner` from an ordinary `impl` block, but no `impl Ordnung for` block;
only `gleich` is named:

```
error: typ 'Punkt' setzt die schnittstelle 'Ordnung' nicht um — schranke am typparameter 'T' von 'f'
   = hinweis: es fehlt 'fn gleich(*self, *Self) -> bool' in 'impl Ordnung for Punkt { … }'
```

For that, `schranke_pruefen` reads the names of all functions of the
merged program and asks for every method of the interface whether
`<Typ>__<Methode>` exists. If the type had **all** methods and only lacked
the block, the message says exactly that („the block `impl … { … }` is
missing").

**A bound on an interface that does not exist:**

```
error: unbekannte schnittstelle 'Ordnunng' als schranke am typparameter 'T' von 'f'
   = hinweis: bekannt sind: Ordnung (eingebaut: Any, Int, Scalar)
```

**A pointer as a type argument** — here the cause is a different one, and
the message says so:

```
error: typargument '*Punkt' erfuellt die schranke 'Ordnung' des typparameters 'T' von 'f' nicht
   = hinweis: eine schnittstelle wird mit 'impl Ordnung for <typ>' umgesetzt;
              ein zeiger- oder feldtyp hat keinen namen, unter dem das stehen koennte
```

**The same bound twice** — during parsing, not only at the instantiation:

```
error: die schranke 'Ordnung' steht zweimal an 'T'
   = hinweis: jede schranke wird hoechstens einmal genannt
```

**Several bounds, one violated** — what is reported is the **first** one; a
second message about the same type argument would say nothing new.

### The 15 negative tests

| File | Case |
|---|---|
| `bound_no_impl.fi` | type without `impl` (the message names the method) |
| `bound_method_partial.fi` | type has one of two methods — only the missing one is named |
| `bound_base_ty_without_impl.fi` | base type without an implementation |
| `bound_unknown.fi` | bound on an unknown interface |
| `bound_duplicate.fi` | the same bound twice (parser) |
| `bound_contradiction.fi` | `Int + Ordnung`, `Int` violated |
| `bound_second_interface.fi` | `Ordnung + Anzeige`, the second one violated |
| `bound_nested.fi` | violation only in the SECOND instantiation level |
| `bound_struct.fi` | bound on a generic struct |
| `bound_ptr_arg.fi` | pointer as a type argument |
| `bound_signature.fi` | `impl` present, signature does not match (`Self` ≠ `i64`) |
| `bound_self_dyn.fi` | `Self` method called via `dyn` |
| `bound_dyn_base_ty.fi` | `as dyn I` on a base type |
| `bound_duplicate_impl_base_ty.fi` | two `impl Ord for i32` |
| `method_without_ty.fi` | method on an array type (which has no name) |

Changed as well: `generic_requirement.fi` (wording „anforderung" →
„schranke") and `impl_no_struct.fi` — whose old message („methoden gibt es
nur fuer struct-typen") is wrong as of this round; it now checks that
`i32.summe()` is cleanly rejected as „typ 'i32' hat keine methode 'summe'".

All 15 are rejected by `firnc1` as well (measured afterwards: `firnc1`
rejects 109 of 115 negative tests; the 6 exceptions are the same as before
this round and are unaffected by it).

---

## 4. The standard library: before / after

`lib/rt/vec.fi` (identical as `lib/std/vec.fi`, symlink).

| | before | after |
|---|---|---|
| `vec_sortiere` | `[T: Scalar]`, `a > b` in the body | `[T: Ord]`, `a.kleiner(b)` in the body |
| `vec_binaersuche` | `[T: Scalar]`, `<` and `==` | `[T: Ord]`, equality from the ordering |
| `vec_ist_sortiert`, `vec_untere_schranke`, `vec_sortiert_einfuegen`, `vec_senken` | `[T: Scalar]` | `[T: Ord]` |
| `vec_min`, `vec_max` | `[T: Scalar]` | `[T: Scalar + Ord]` |
| sortable types | the scalars for which the compiler knows `<` | **every type with `impl Ord`** |
| ordering selectable | no | yes, it belongs to the type |

**What was needed for that — `vec_zeiger[T]`.** `vec_at[T]` returns
`0 as T` beyond the end, and `0 as T` exists only for scalars. Exactly that
was what the bound `T: Scalar` hung on, and exactly for that reason a
`Vec[Punkt]` could not be sorted. A **pointer** has a null value for every
element type; everything that orders (sorting, searching, swapping,
inserting) now works over it. `vec_at`
remains `[T: Scalar]` unchanged — it is the comfortable version for scalars.

**What that costs.** `interface Ord` and ten implementations (`i8`…`isize`)
stand in `lib/rt/vec.fi` and are therefore present in every program that
includes `vec`. That is ten functions with one comparison each; a method
table is created for none of them (base type). Measured afterwards on a
program that never sorts — the compiler
itself: `firnc0 --emit=asm bin/firnc1.fi` yields **209.388** lines with the
old `vec.fi` and **209.579** with the new one. The price for `Ord` and ten
implementations in a program that does not use them is therefore **191
lines of assembly (+0,09 %)**.

**What it brings** — `tests/831_bounds_std_core.fi` drives both sides:
the same functions with `i32` (as before) and with

```firn
struct Person { alter: i64, nummer: i64 }
impl Ord for Person {
    fn kleiner(*self, b: *Person) -> bool {
        if (*self).alter != (*b).alter { return (*self).alter < (*b).alter }
        return (*self).nummer < (*b).nummer
    }
}
```

Two keys, and descending by the second one would be just as possible —
that is the point: the ordering belongs to the type, not to the sorter.
Before, this program could not be written.

**Equality from the ordering.** `vec_binaersuche` checks `!(a<b) && !(b<a)`
instead of `a == b`. That is not a convenience but the only equality
that fits a binary search: it has to be the same one the sorting used.
Otherwise the search would find a place at which, by the ordering, nothing
stands.

**Why `Ord` does not go via a key.** The simpler design would have been
`interface Ord { fn schluessel(*self) -> i64 }` — without `Self`, without
implementations for base types. It fails on a value that is already in the
test corpus: `tests/802_std_vec_core.fi` sorts `u64` and searches for
`9223372036854775808`. That fits into no `i64`. A key would have silently
made the ordering wrong for half of all `u64`.

---

## 5. What was changed

| `firnc0` (Rust) | Lines | what |
|---|---|---|
| `compiler/src/iface.rs` | +410/−45 | bound check with method names, `Self`, base type implementations |
| `compiler/src/mono.rs` | +84/−26 | all bounds per parameter, route to `iface.rs` |
| `compiler/src/sema_generic.rs` | +53/−22 | `Bound::Iface`, `+` lists, duplicate bound |
| `compiler/src/impls.rs` | +49/−16 | the receiver may be a base type |
| `compiler/src/modules.rs` | +9 | do not rename base type methods |

| `firnc1` (Firn) | Lines | what |
|---|---|---|
| `lib/firnc1/iface.fi` | +222/−48 | `Self`, base type implementations, `if_umsetzung_da` |
| `lib/firnc1/mono.fi` | +107/−12 | bound lists, interface bounds |
| `lib/firnc1/parser.fi` | +55/−17 | `+` lists, `Self` detection |
| `lib/firnc1/types.fi` | +32 | `grundtyp_name` (the inverse of `grundtyp`) |
| `lib/firnc1/sema.fi`, `lower.fi`, `codegen.fi` | +41/−16 | methods on base types, no table for a base type |

**No new FIR opcodes.** The opcode rule of this round (numbers 30–39
reserved) was not needed: static dispatch is an ordinary
`Call`, and the bound is a check, not an instruction. The reserved
range remains untouched — `fir.rs`/`fir.fi` are unchanged.

---

## 6. Acceptance

Measured on `r50-generik` after `rm -f .firnc1 .firnc2 .firnc3` (no
reused binary), with its own `mktemp -d` in every tool.

| Check | base `cc1710f` | now |
|---|---|---|
| `bash ./test.sh` | 751/751 | **PASS 773/773** |
| `bash tools/self_compare.sh` | 213 identical / 0 differing / 0 failing | **215 identical / 0 differing / 0 failing** |
| `bash tools/fixpoint.sh` | character-identical, 427.401 lines | **stage 2 == stage 3, character-identical, 431.972 lines** |

The +22 in `test.sh` can be explained file by file: 2 new programs x 3
build stages (`830`, `831`) = 6, 15 new negative tests = 15, the new step 8c
(`tools/bounds/run.sh`) = 1. The +2 in the self-comparison are the same two
programs; `tests/modules/bounds.fi` does not count (`firnc0` does not
compile a module individually).

Both comparison numbers come from an INDIVIDUALLY started run of the
respective script, each time after `rm -f .firnc1 .firnc2 .firnc3` — no
reused binary. `tools/fixpoint.sh` and `tools/bounds/run.sh`
create their working directory with `mktemp -d`; there are no fixed `/tmp`
names in this round.

**The fixpoint holds.** `.firnc2` (produced by a compiler that came from
Rust) and `.firnc3` (by one that came from Firn) are octet for octet
identical — 2.483.328 octets. The language change of this round is
therefore the same in both compilers, not merely similar.

The assembly of `.firnc2` grew from 427.401 to 431.972 lines (+1,07 %) —
that is the whole round, predominantly `iface.fi` and `mono.fi`.

---

## 7. Deliberately left out

* **Checking the template body against the bound.** Today the bound is a
  promise to the **caller**, not a restriction of the body. The body
  is only checked after the instantiation, i.e. against the **concrete**
  type: a template `fn f[T: Ord](a: *T)` may write `a.etwas_anderes()`, and
  that goes through as long as the instantiated type has this method. A
  template that is never instantiated is not checked at all. That is the
  flip side of monomorphization without a separate type check of the
  template; whoever wants to change it needs a checking pass over the body
  with `T` as an abstract type — a round of its own, and one that touches
  the error messages of all existing templates.
* **Statically unsatisfiable bound sets.** `[T: Int + Ordnung]` is allowed,
  even if no implementation of `Ordnung` is ever an integer type. It is
  reported at the instantiation, not at the declaration. To recognize that
  would mean enumerating all implementations — and more of those may still
  be added later.
* **Inheritance between interfaces** (`interface A: B`) and
  **default methods** — both have been open since round 46 and have
  remained so.
* **`dyn I` for base types.** An interface value carries a data pointer and
  a method table; a table is created only per struct implementation.
* **Bounds on `gc class` types.** `impl I for <gc class>` has worked since
  round 46; as a **type argument** of a template a class is only available
  as `Gc[K]`, and that is a pointer type — see the next line.
* **Ordering for `f64` and `bool`.** `lib/rt/vec.fi` implements `Ord` only
  for the ten integer types. `f64` would have brought floating point code
  into a file that `firnc1` compiles itself (whose code generator cannot do
  floating point); `bool` has no ordering that anyone expects.

---

## 8. Rejected approaches

**Resolving bounds during parsing.** First draft: unknown name =
error, as up to now with `Any/Int/Scalar`. Rejected as soon as
`interface Ord` is supposed to stand below `fn vec_sortiere[T: Ord]` — and
in `lib/rt/vec.fi` it stands exactly that way, because the implementations
for the base types lie in between. The parser always sees only one file; it
cannot answer this question.

**Structural satisfaction** („the type has the methods, so it satisfies the
bound"). Comfortable and wrong: `impl I for T` is the place at which the
type checker checks the **signatures**. Without a block there is no such
check, and a method that happens to have the same name but a different
meaning would have been accepted silently. The names of the existing
methods are read anyway — but only to make the **message** usable.

**`Ord` via a key** (`fn schluessel(*self) -> i64`). See §4:
fails on `u64` values above `i64::MAX`, and those are already in the test
corpus.

**Renaming base type methods per module.** That would have been the rule
`modules.rs` otherwise applies — and it would have turned `i32__kleiner`
from `std.vec` into `vec__i32__kleiner`, while the resolution at the call
site still looks for `i32__kleiner` (it computes from the **type**, and the
type is called `i32` in every module). Rejected in favor of the rule
„program-wide type, program-wide methods" — the same one that already
applies to `interface`, `enum`, `gc class` and generic templates.

**`struct_idx` as the key for „duplicate implementation".** It worked as
long as every implementation had a struct. A base type has none; all of
them would have carried `usize::MAX` and would have counted as the **same**
implementation. What is compared now is the method prefix (`i32`,
`geo__Punkt`) — it is there for both cases and is unambiguous.

**The suffix rule for base types as well.** `iface::typ_struct` looks, as
its third step, for „exactly one struct whose name ends in `__<Name>`"
(type from a module). For `i32` this rule finds `Vec__i32` — the struct
`Vec[i32]`. The bug was real and appeared in the test run after five
minutes (`'Vec__i32' setzt die methode 'Ord.kleiner' nicht um`). That is
why the base type is asked **first**, and the suffix rule applies only to
names that are not one.

---

## 9. What remained open — and where it hurts

**Type arguments from a module do not work.** That is OLDER than this round
(since round 30) and came to light here because `tests/830` tried it first:

```firn
groesster[schranken.Marke](&a, &b)      // error: unbekannter typ 'schranken.Marke'
```

Cause: the instantiation name (`groesster__schranken.Marke`) comes into
being during **parsing** and stands in the tree as the call name afterwards;
the module renaming only runs **after that** and does not touch the type
arguments in the registration. The substituted type is subsequently called
`schranken.Marke`, but the struct is called `schranken__Marke`. The same
holds for a module-local type that is used as a type argument within its
own module. Not repaired, because the repair would have to rewrite the
instantiation names in the whole tree — that is a round of its own and
touches `ast_kanon`. `tests/modules/bounds.fi` therefore checks what does
work: interface and template in the module, implementation and
type argument in the root file, plus `impl Reihe for u16` in the module.

**A small trap, recorded:** `*self as i64` is `*(self as i64)` — `as`
binds more tightly than the unary operators (SPEC §14.1 item 12). The
correct form is `(*self) as i64`. That cost one failed attempt in
`tests/modules/bounds.fi`, with a message that pointed at an empty line.

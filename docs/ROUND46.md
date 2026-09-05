# Round 46: interfaces — `interface` and dynamic dispatch

**Base: `a492d26` (main after rounds 43/44/45).** Branch `r46-interfaces`.

Round 45 brought methods — but explicitly only as a writing aid: `x.m(a)`
became `Typ__m(&x, a)`, decided solely by the **static** type. This
round adds the other case, which `SPEC.md` §6.2 has demanded since v0.1 and
which `DESIGN_GOALS.md` §1 presumes for `Io`: **one call site, many types.**

```firn
interface Area {
    fn area(*self) -> i64
    fn scale(*mut self, k: i64)
}

impl Area for Rect   { ... }
impl Area for Circle { ... }

let f: dyn Area = (&r) as dyn Area
f.area()             // through the method table -- which code runs is
                     // settled only at run time
```

---

## 1. The representation — binding

```text
interface I      -> struct "dyn I" in types::TypeCtx, 16 byte:
                      data:  *mut u8   (offset 0) -- the value itself
                      table: *mut u8   (offset 8) -- the method table
dyn I            -> exactly this struct (a FAT POINTER, not a pointer)
impl I for T     -> method table `.L__iface.I.T` in `.rodata`:
                      .quad T__m1
                      .quad T__m2      (order = the order in `I`)
```

The internal struct name carries a **space** (`"dyn I"`) — the same
construction as `"gc C"` (gc.rs) and `"methode m"` (impls.rs). It cannot
arise from any identifier of the source text, and `TypeCtx::name_of` prints
it unchanged: every error message says `dyn I`, exactly as one writes it.

### Why a struct and not a `Type` of its own

A `Type::Dyn(..)` would have touched **every** case distinction over `Type`:
layout, ABI, monomorphization, optimizer, debug info, code generator. As a
struct the interface value is an ordinary 16-byte aggregate:
`abi::classify` gives it two INTEGER words, and it is copied, passed,
returned and placed in the frame like any other struct. That is why
everything `tests/820` tries out — parameter, return value, struct field,
array element, copy — holds without a single line having been written for
it.

The entire language change thereby sits in the parser, the type checker and
**two** FIR instructions.

---

## 2. Two new FIR instructions

| Instruction | Textual form | Meaning |
|---|---|---|
| `Op::CallIndirect { target, args }` | `calli.i64 %7(%3, %4)` | call via a pointer; otherwise everything is as with `Call` |
| `Op::VtabAddr { table }` | `vtab.ptr @Area.Circle` | address of a method table (`.rodata`) |

Both produce the same instruction in **both** code paths of `firnc0` (base
path and register allocation) and in `firnc1`:

```asm
    lea rax, [rip + .L__iface.Area.Circle]
    …
    call rax
```

**Why `rax` and not `r11`:** `r11` is a working register in `regalloc.rs`
(`TEMP_REGS`) and can be the **home of a value** — an argument that happens
to be there would be destroyed when the target is loaded. `rax` is
never the home of a value (header of `regalloc.rs`) and is not an argument
register; that is why the target is loaded there and **last**, after all
arguments. That is the one spot of this round at which a wrong decision
would silently have produced wrong code.

The call itself runs through **the same** path as every other call
(`lower::lower_call`): aggregates in registers, hidden return pointer from
9 bytes on, stack arguments from the seventh word on. There is no second
call path that could drift apart.

---

## 3. How an interface value comes into being

**Explicitly, never silently.** `SPEC.md` §6.2 demands that dynamic
resolution be „written down explicitly", and §4.5 abolishes implicit
conversions in general. A silent conversion at an assignment that
additionally attaches a method table would be the worst place to start
with it.

```firn
let f: dyn Area = (&r) as dyn Area
```

The parentheses around `&r` are mandatory — `as` binds more tightly than
the unary operators (`SPEC.md` §14.1 item 12). For a value that already is
a pointer (`Gc[K]`, `*mut T`) they are unnecessary.

In the lowering that is exactly two words:

```text
store.ptr %zeiger, [%ziel + 0]
%t = vtab.ptr @Area.Circle
store.ptr %t,     [%ziel + 8]
```

The data pointer is **not modified** — no offset, no obfuscation.
That is the precondition for section 5.

---

## 4. The call

```text
%b = <address of the interface value>
%d = load.ptr [%b + 0]      ; the value itself
%t = load.ptr [%b + 8]      ; the method table
%z = load.ptr [%t + 8*k]    ; the k-th method of the interface
%r = calli.i64 %z(%d, ...)
```

Three loads and one indirect jump per call. The price is thereby visibly
present in the source text of the compiler and in the FIR — `SPEC.md` §1,
principle 1 („nothing hidden") and principle 4 („whoever does not order
does not pay"): a program without `interface` gets not a single one of
these instructions and no `.rodata` table.

### Measured (callgrind, `--tool=callgrind`, 2.000.000 calls in a loop)

| | instructions total | per iteration |
|---|---|---|
| static call, with optimizer | 10.000.017 | **5** |
| dynamic call, with optimizer | 52.000.037 | **26** |
| static call, `--no-opt` | 150.000.066 | **75** |
| dynamic call, `--no-opt` | 182.000.082 | **91** |

Read honestly: the **pure** dispatch costs 16 instructions (`--no-opt`,
both sides without inlining). With the optimizer the gap opens further — 5
against 26 — and **not** because the dispatch gets more expensive, but
because the static call disappears entirely: `inline.rs` inlines it, and
the rest falls victim to constant folding. That is exactly the reason why
`dyn` is written down explicitly in this language and is not the default.

---

## 5. The collector and the fat pointer

The data pointer is an ordinary pointer to the **start** of the value.
A `dyn I` lies in the frame or in a callee-saved register; the collector
scans both conservatively (`SPEC.md` §3.5.3), and before a collection
`Op::GcAddr { regs: true }` saves the registers into the state block.
With that an interface value keeps its object alive, even if there is no
other root left.

`tests/822_iface_gc_core.fi` demonstrates that without relying on
chance:

* 64 `gc class` cells are created and stored **exclusively** as
  `dyn Counter` in an array in the frame,
* afterwards 20.000 unreachable cells (`garbage`) come into being, and the
  runtime collects on its own while that happens,
* after two explicit `gc_collect()` calls **at most 200** objects are alive
  — so the 20.000 have really been swept — and the 64 values are unchanged,
* after that a write goes through the interface (`add`) and is read back.

If the fat pointers were not roots, the 64 cells would have been swept
along and their memory handed out to the garbage; the sum afterwards would
be junk. The test runs in all three compilation modes (`release-fast`,
`--no-opt`, `dev-fast`) and under `firnc1`.

**The one place where this does not carry is the heap:** there the
collector traces PRECISELY from the field layout and does not know the data
pointer in a `dyn I` field. A `dyn I` as a field of a `gc class` is
therefore an error — and it is one at the spot where the rule already
lives: `gc.rs` permits only integers, `bool`, pointers, `Gc[T]`,
`GcWeak[T]` and arrays of those in a class anyway, so no struct either and
therefore no `dyn I`
(`tests/neg/iface_dyn_in_gc_class.fi` records that). A second check next to
it would have been a second truth.

### `impl I for <gc class>`

For the proof above to be possible at all, a `gc class` may implement an
interface. The receiver `*self` is then `Gc[K]` — a
`gc class` value only exists on the heap, and a pointer to it is the only
way to touch it (`SPEC.md` §3.5.1). Technically the receiver carries the
internal struct name `"gc K"` for that; `self` as a **copy** is an error
with a clear statement for a class.

**Restriction, openly named:** whether `K` is a class is recorded in the
registration of `gc.rs`, and that is filled during **parsing**.
`gc class K` therefore has to stand before the `impl` block (in the same
file or in one read in earlier). If it stands after it, the type checker
reports "'K' is a gc class and cannot be a value" -- understandable,
but not the message one would wish for. Both compilers behave the same way
here, because both read the same registration at the same point in time.

---

## 6. What is checked — and where

| Check | Place | Negative test |
|---|---|---|
| all methods of the interface present | `iface::pruefe_umsetzung` | `iface_method_missing.fi` |
| return type matches | the same | `iface_signature_ret.fi` |
| parameter type matches | the same | `iface_signature_parameter.fi` |
| parameter count matches (counted without the receiver) | the same | `iface_parameterzahl.fi` |
| receiver is a pointer to exactly this type | the same | — |
| no two `impl I for T` | the same | `iface_duplicate_impl.fi` |
| the interface exists | the same | `iface_unknown.fi` |
| `dyn I` with an unknown `I` | `iface::hook_resolve_ty` | `iface_dyn_unknown.fi` |
| receiver of an interface method is `*self`/`*mut self` | parser | `iface_receiver_value.fi` |
| interface method has no body | parser | `iface_body.fi` |
| only methods of the interface are reachable via `dyn` | `iface::hook_methode` | `iface_no_method.fi` |
| conversion only from a pointer to a struct | `iface::hook_cast` | `iface_no_ptr.fi` |
| the type really implements the interface | the same | `iface_does_not_impl.fi` |
| `*dyn I` is not an interface value | `iface::hook_methode` | `iface_ptr_receiver.fi` |
| `dyn I` not in the GC heap | `gc.rs` (field types of a class) | `iface_dyn_in_gc_class.fi` |

Every message names the line, the column and, in the hint, the **expected
signature**:

```
error: 'Circle' does not implement the method 'Area.scale'
  --> tests/neg/iface_method_missing.fi:13:6
   = note: 'fn scale(*mut self, i64)' is expected in the block
```

All 14 negative tests are rejected by `firnc1` as well. With
`iface_dyn_unknown.fi`, `firnc1` ends with 5 („the lowering gives up")
instead of 1 — that is **not** new and not interface-specific: an
unknown type name has always yielded an error type without a message of its
own in `firnc1` (`let x: Unbekannt = 1` behaves exactly the same).

---

## 7. Name resolution

Methods of an interface are **ordinary functions**: `impl I for T`
creates exactly the same `T__m` as `impl T` from round 45. The interface
only additionally says what **must** be in there. It follows that:

* The static call remains possible (`r.area()`), and it is the same
  code that the table names.
* A second `impl` block without an interface for the same type is allowed.
* A free function may still have the same name (`tests/820`:
  `area(a, b)` next to `Rect.area()`).

**Via `dyn` only what stands in the interface is reachable** — even when
the concrete type can do more. Otherwise the table would not be the whole
truth.

### Modules

`interface` names are valid **program-wide** and are not renamed — like
enums and `gc class` (`SPEC.md` §14.1 T6). An `impl Size for Circle`
in a module, by contrast, produces `draw__Circle__size`, and exactly
that name has to stand in the table.

The type name in the registration stands there as it was written in the
source text — `firnc0` renames **after** parsing (`modules.rs`), `firnc1`
**during** parsing (`parser.fi`), and neither of them touches the
registration. Both compilers therefore look for the struct in the same
order (`iface.rs::typ_struct`, `iface.fi::typ_struct`):

1. the name itself,
2. `gc <Name>`,
3. exactly **one** struct whose name ends in `__<Name>` (the case „type in
   a module"). Several hits are an error — guessing would be the
   more dangerous choice.

`tests/821_iface_module_core.fi` drives both at the same time: two
implementations in the module, one in the root file, all over the same
interface.

---

## 8. What was changed in both compilers

| `firnc0` (Rust) | Lines | what |
|---|---|---|
| `compiler/src/iface.rs` | **+1010 new** | everything specific: registration, parser, checks, resolution, lowering building blocks, `.rodata` tables |
| `compiler/src/impls.rs` | +100/−28 | `impl I for T`, method prefix without `"gc "`, receiver of a class |
| `compiler/src/fir.rs` | +29 | `CallIndirect`, `VtabAddr`, textual form |
| `compiler/src/regalloc.rs` | +55/−2 | both instructions, call barriers extended |
| `compiler/src/codegen_x86.rs` | +47 | the same instructions in the base path, tables |
| `compiler/src/lower.rs` | +59/−3 | dispatch in the ordinary call path, `as dyn I` |
| `compiler/src/sema.rs`, `parser.rs`, `mem2reg.rs`, `inline.rs`, `main.rs` | +46 | hooks |

| `firnc1` (Firn) | Lines | what |
|---|---|---|
| `lib/firnc1/iface.fi` | **+558 new** | the same registration, the same name arithmetic |
| `lib/firnc1/parser.fi` | +299/−13 | `interface`, `impl … for …`, `dyn I`, pre-scan |
| `lib/firnc1/codegen.fi` | +168/−1 | `O_CALLI`, `O_VTAB`, method tables |
| `lib/firnc1/lower.fi` | +143/−7 | dispatch, `as dyn I` |
| `lib/firnc1/sema.fi` | +126/−1 | registration, check, call, conversion |
| `lib/firnc1/fir.fi` | +29 | the same two instructions, the same textual form |
| `bin/firnc1.fi`, `bin/semadump.fi`, `bin/firdump.fi` | +21 | pass the registration through |

`bin/astdump.fi` and `bin/layoutdump.fi` **deliberately** do not get the
registration: their yardstick (`ast_canon.rs`, `layout_canon.rs`) knows
only the root file, and there `dyn I` — like `Gc[C]` and `E!T` — is an
unknown name and becomes `?`. Both sides do the same thing, and the
comparison stays exact.

---

## 9. Deliberately left out

* **Default methods** (`fn m(*self) -> i64 { … }` in the interface). They
  would need a function without a type behind it and a rule for what name
  it carries. Today an error with a clear statement instead of a syntax
  error.
* **Static resolution over interfaces** (`fn f[T: Area](x: T)`,
  `SPEC.md` §6.2, first half-sentence). That is the monomorphization side
  and belongs to the requirements on type parameters (§14.1 T7) — a round
  of its own.
* **Interfaces as a requirement on a generic template**, for the same
  reason.
* **Inheritance between interfaces** (`interface A: B`).
* **`dyn I` in the GC heap** — see section 5, that is a promise, not a
  gap.
* **The reverse check** (`x.as?[T]` from `dyn I` to the concrete type). For
  that the table would have to carry a type tag; today it contains only
  method pointers. The room for it is there (one word before the table),
  the decision is postponed.
* **`#[no_gc]` and interfaces.** A call via `dyn` is rejected in a
  `#[no_gc]` function — the same rule as for methods in round 45,
  and here even compulsory: the checker does not know which function runs.
* **Tables only for used implementations.** One table is generated per
  complete implementation, even if no `as dyn` ever touches it. The content
  depends solely on the declaration; a table that comes into being only
  sometimes would be the sort of state one does not want to see while
  debugging.

---

## 10. Rejected approaches

**A `Type::Dyn` of its own.** First draft, rejected after counting the
case distinctions: `types.rs`, `abi.rs`, `layout.rs`, `mono.rs`,
`sema.rs`, `lower.rs` and `codegen_x86.rs` would each have needed a new
branch, and `firnc1` the same ones again. The struct with the
space in its name costs none of them.

**Silent conversion at assignment and argument.** More comfortable, but
against `SPEC.md` §4.5 and §6.2 — and it would have raised the question of
which of the possible interfaces is meant as soon as a type implements
several.

**The receiver as a type form of its own** (`TypeExpr::Named("impl T")`),
to make `impl` for `gc class` possible without an ordering rule. Rejected
because `modules.rs` does not rename this name along: a module type would
have been unfindable afterwards — the module support from round 45 would
have broken for a corner case.

**`r11` as the jump register.** Obvious (caller-saved, not an argument
register) and wrong: `regalloc.rs` hands out `r11` as the home of a value.
Found while reading `TEMP_REGS`, not through a test failure — that is why
the justification now stands at both places in the code.

**A side table between the type checker and the lowering.** Not built, as
in round 45: the receiver type and the method name are available in both
phases, and the derivation is pure name arithmetic. A table would have to
be carried along by `firnc1` without being able to do anything the type
does not already say.

---

## 11. Acceptance

Measured on `r46-interfaces` with a freshly built `firnc0` and freshly
built helper binaries (`.firnc1`, `.astdump`, `.semadump`, `.firdump`,
`.layoutdump`).

| Check | Base `a492d26` | now |
|---|---|---|
| `bash ./test.sh` | 696/696 | **719/719** |
| `tools/self_compare.sh` | 201 / 0 / 0 | **204 identical / 0 differing / 0 failing** |
| `tools/fixpoint.sh` | character-identical | **stage 2 == stage 3, character-identical (344.864 lines of assembly)** |
| parser (`parser_compare.sh`) | 240 identical, 1 known | 254 identical, 1 known |
| layout/ABI (`types_compare.sh`) | 190 identical, 0 different | 204 identical, 0 different |
| type checker (`sema_compare.sh`) | 147 identical, 1 known | 148 identical, 1 known |
| lowering (`fir_compare.sh`) | 146 identical, 1 known | 147 identical, 1 known |

The one known deviation is unchanged `tests/590_f64.fi` (literal
`1e308`, rounding case from round 20 — not a parser bug).

**How it was measured.** The base values come from a separate export of
`a492d26` (`git archive`, freshly built, `bash ./test.sh` -> `PASS 696/696`),
and the four comparison numbers on both sides from an INDIVIDUALLY started
run of the respective script. Inside `test.sh`, `parser_` and
`typen_vergleich` count nine files more each (intermediate states that the
earlier steps deposit in the tree); that applies equally to both sides and
is not an effect of this round — like is therefore compared with like.

The increase can be explained file by file: the fourteen negative tests
pass the parser (only the type checker rejects them) and are counted there;
`tests/820`, `tests/821`, `tests/modules/draw.fi` and `lib/firnc1/iface.fi`
are added, and `tests/822` counts as „not core" (gc). For the type checker
and the lowering only `tests/820` remains — the other new files cannot be
checked individually by `firnc0` (module, gc) or they are negative tests.

## 12. Lines

| | Lines |
|---|---|
| `firnc0` (Rust) | +1422 / −33 |
| `firnc1` (Firn) | +1326 / −22 |
| tests (3 programs, 1 module, 14 negative tests) | +652 |

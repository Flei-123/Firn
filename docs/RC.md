# `Rc` / `Weak` — level 2 of the memory model

Reference: `SPEC.md` §3.2 (three levels), §3.4 (`Rc[T]`, `Weak[T]`,
`Arc[T]`), §3.6 (raw pointers), `DESIGN_GOALS.md` §2 (fallible allocation),
`../osum-browser/FIRN-ANFORDERUNGEN.md` requirement **S7**.

This file describes what is in the tree, how it is used, and **what is
deliberately missing**. Everything claimed here is backed by a test program
that `bash test.sh` runs in three build stages.

---

## 1. What for

Level 2 is for **shared, immutable** values: computed style values,
interned atoms, font data. Exactly the cases in which Stylo takes `Arc`.
One value, many readers, no modification, release at the last reader.

`Rc` is **not** the tool for the DOM — see section 6.

---

## 2. Where it lives

| File | Role |
|---|---|
| `tests/modules/rc.fi` | **the one implementation** (module `rc`) |
| `lib/rc/rc.fi` | symlink to exactly this file — the library path exists without duplicating code |
| `lib/rc/parts/*.fi` | the bodies of the test programs |
| `lib/rc/gen_tests.sh` | assembles body + implementation into `tests/55*_rc_*.fi` and `tests/neg/rc_*.fi` |
| `tests/550_rc_basic.fi` … `554`, `tests/neg/rc_*.fi` | the generated test programs |

**Why copied together and not `import modules.rc`?** Stage 0 does not
resolve generic templates across module boundaries. Both forms already fail
in the parser:

```
var a: rc.RefCount[Style] = ...    // error: expected '=' after the name in a 'var' statement
rc.rc_new[Style](&h, w, &a)        // error: only direct function names can be called
```

(`compiler/src/sema_generic.rs::hook_generic_call` demands a simple
`Ident`; the module system delivers a `Field` expression at this place.)
`lib/str` has been using the same solution since round 2
(`tools/strlib/expand.py`): the library stands **once** in the tree and is
inserted verbatim into the test programs. Generate with

```
bash lib/rc/gen_tests.sh
```

---

## 3. Interface

```firn
error AllocError { OutOfMemory }

struct RefCount[T] { block: usize }        // = Rc[T]   from SPEC 3.4
struct WeakRef[T]  { block: usize }        // = Weak[T] from SPEC 3.4
struct RcHeap { ... }                      // a heap of fixed capacity
```

| Function | Meaning |
|---|---|
| `rc_heap_init(h, bytes) -> bool` | create the heap via `mmap`. `false` = failed — a **visible** failure, no silent substitute. No `MAP_FIXED`, no fixed address. |
| `rc_heap_free(h)` | `munmap` |
| `rc_new[T](h, value, out) -> AllocError!bool` | put a value on the heap, strong reference into `*out`. **Fallible** (DESIGN_GOALS 2), the result is `#[must_consume]`. |
| `rc_read[T](r) -> T` | **read only** -- returns a copy |
| `rc_clone[T](r) -> RefCount[T]` | strong counter + 1 |
| `rc_free[T](h, r)` | strong counter − 1, clears `*r`; at 0 and without a weak reference the block goes into the free list |
| `rc_strong_number[T](r)`, `rc_weak_number[T](r)` | counter values, measurable |
| `rc_empty[T]()`, `rc_is_empty[T](r)`, `rc_equal[T](a,b)` | null value, test, identity |
| `weak_of[T](r) -> WeakRef[T]` | weak reference, does **not** keep alive |
| `promote[T](w) -> RefCount[T]` | upgrade; **visibly empty** if the strong counter is 0 |
| `weak_freigeben[T](h, w)` | weak counter − 1; releases the block if strong is 0 as well |
| `rc_heap_lebende/belegte_bytes/allokationen/freigaben(h)` | real counts for tests and measurements |
| `rc_roh_adresse`, `rc_wert_adresse`, `rc_roh_verweis` | raw access according to SPEC §3.6, explicitly for tools and the leak proof |

Block layout: a 32-byte header (`strong`, `weak`, `class`, free list
links), then the value. Eight size classes 64 ... 8192 bytes with one free
list each; larger payloads are `AllocError::OutOfMemory` (demonstrated in
`tests/553_rc_fallible.fi`).

### Example

```firn
var h: RcHeap = heap_empty()
if rc_heap_init(&h, 65536 as usize) == false { return 90 }

var a: RefCount[Style] = rc_empty[Style]()
let ok: bool = rc_new[Style](&h, Style{ color: 1 as u32, size: 16 as u32, lines: 3 }, &a)
               catch false
if ok == false { return 1 }

var b: RefCount[Style] = rc_clone[Style](a)     // strong = 2
let s: Style = rc_read[Style](b)                // read only
rc_free[Style](&h, &b)                          // strong = 1
rc_free[Style](&h, &a)                          // strong = 0 -> block released
```

---

## 4. `Rc` is ALWAYS immutable

There is no `RefCell` equivalent, no interior mutability and **no writing
function** in this module. `rc_read` returns a copy; the
attempt to modify the shared value through it is a compiler error:

```
tests/neg/rc_immutable.fi:396:5
error: left side is not an assignable expression (variable, field, index or '*pointer')
```

Whoever wants to share **and** modify takes `Gc[T]` (SPEC §3.5) or a lock.

---

## 5. Fallible allocation

`rc_new` returns `AllocError!bool`. A discarded result is a
compiler error:

```
tests/neg/rc_discarded.fi:393:5
error: the result must not be discarded: the type 'AllocError!bool'
       is marked with #[must_consume]
```

`tests/553_rc_fallible.fi` establishes: a heap with one page → 64 blocks,
the 65th allocation reports `AllocError::OutOfMemory` and the output
reference stays empty; after one release the next allocation succeeds
again; a payload that is too large fails cleanly as well; `try` passes the
error through the call chain.

Unlike with the GC there is **no collection before the failure** here — the
counting releases immediately, there is nothing to catch up on.

---

## 6. Cycles leak — and why `Rc` is not intended for the DOM

**That is not a bug of this implementation but the property of counting.**
If two values hold each other strongly, no counter ever falls to 0. The
memory stays held until the end of the program.

`tests/552_rc_cycle_leak.fi` makes that visible instead of hiding it.
Measured output (the same in all three build stages):

```
1 1 2 128 200 12800 198 1 0
```

* `1 1` — after discarding **both** outer handles, both strong
  counters still stand at 1: the cycle holds itself.
* `2 128` — two live blocks, 128 occupied bytes, although nobody can
  reach them any more.
* `200 12800` — after 100 cycles created **and discarded** there are 200
  blocks and 12.800 bytes. The consumption grows monotonically: a leak.
* `198` — resolved by hand is only the one cycle whose addresses the
  test still knows. For the other 99 the address is gone — that is exactly
  the situation in a real program.
* `1 0` — the same structure with `Weak` instead of a second strong
  reference: the upgrade visibly returns empty (`1`) and at the end **zero**
  blocks are occupied (`0`).

For comparison `tests/554_rc_dauerlauf.fi`: 20.000 rounds with creating,
cloning, weak references and releasing **without** a cycle end with
`20000 0 0 192 1` — zero live objects, zero occupied bytes, and the heap
never grows beyond 192 bytes (the free lists are reused).
So the counting itself does not leak; only the cycle does.

Because `Rc` is always immutable, a cycle cannot even be built with
ordinary application code — the test adds the back reference over
a raw pointer (SPEC §3.6). With interior mutability (which deliberately
does not exist here) the same cycle would be ordinary application code.

### The DOM

A DOM consists almost entirely of cycles: parent reference **and** child
list, listeners that hold their node while the node holds the listener,
live `HTMLCollection`s, JS wrappers that hold the node while the node
holds its wrapper. Every one of these cycles leaks under counting. Setting
`Weak` by hand only works if the cycle is **obvious and local** —
with the DOM it is neither the one nor the other.

That is why `SPEC.md` §3.4 says explicitly: **`Rc` is not intended for the
DOM.** For that there is level 3, the opt-in tracing GC from §3.5
(`gc class`, `Gc[T]`, `GcWeak[T]`). `Rc` remains for shared, immutable
leaves of the graph: style values, atoms, font data.

---

## 7. Deviations from SPEC §3.4 and open points

These points belong in `SPEC.md` §14.1; they are enumerated here
completely and are **not** retouched away in the SPEC.

| No. | Deviation | Reason |
|---|---|---|
| A1 | The types are called `RefCount[T]` / `WeakRef[T]`, not `Rc[T]` / `Weak[T]` | `Rc`, `Arc`, `Weak` are reserved in the parser as type constructors that are not yet implemented (`compiler/src/parser.rs`) and report "'Rc[T]' is not implemented in stage 0". A pure Firn module cannot occupy these names; the compiler sources belong to other modules in this round. The **function names** follow the contract. |
| A2 | `rc_new(...)` instead of `Rc[T].new(...)` | stage 0 knows no methods |
| A3 | `h: *mut RcHeap` instead of `inout alloc` | stage 0 knows no `inout` |
| A4 | return `AllocError!bool` + output pointer instead of `AllocError!Rc[T]` | monomorphization does not substitute type arguments in the payload of an error union: `fn f[T](..) -> AllocError!RefCount[T]` reports „unbekannter typ 'Zaehlverweis__T'". The allocation stays fully fallible and `#[must_consume]`. |
| A5 | `Arc[T]` has been built since **round 47**: `lib/rc/arc.fi`, types `Atomverweis[T]`/`AtomSchwachverweis[T]`, counters really atomic (`__atomar_addieren` -> `lock xadd`, `compiler/src/atomic.rs`) | proof `tools/atomic/run.sh` and `tests/830`-`833`. Honestly named it remains: `aufwerten_atomar` needs a compare and exchange for real concurrency, which round 47 does not build, and stage 0 still has no threads (SPEC §7). See `docs/ROUND47.md`. |
| A6 | No destructors: references stored in a value have to be released by hand | `drop` (SPEC §3.3) is not built in stage 0. Affects only values that themselves contain references. |
| A7 | Heap with a fixed capacity, no growing | it makes the allocation honestly fallible and is the basis for memory limits per job |

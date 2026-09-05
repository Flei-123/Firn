# Firn -- language specification

**Working title:** Firn - **File extension:** `.fi` - **As of:** v0.2 (2026-08-13)
**Author:** Justin (GitHub: Flei123) - **Target systems:** Osum / the osum
kernel **and the Osum browser engine**, x86_64

> **A companion document.** `DESIGN_GOALS.md` deals with ten known weak spots of
> today's languages (function colours, fallible allocation, capability modules, a
> stable ABI, the speed of debug builds, in-place initialization,
> comptime/reflection, data layout/SoA, hot reload) and separates what has to go
> into the foundation **now** from what can be retrofitted. Where it contradicts
> this specification, `DESIGN_GOALS.md` wins and 7 and 15 respectively get
> corrected -- which has already happened.

> **Renameability.** The language name and the file extension stand at *exactly
> one* place in the compiler: `compiler/src/config.rs` (`LANG_NAME`, `FILE_EXT`,
> `LANG_NAME_LOWER`). Every error message, every help text and every file search
> reads from there. Renaming the language is a change to three constants, not a
> refactoring.

---

## Change history

### v0.2 (2026-08-13) -- the browser requirements worked in

The trigger: decision **B1** in the project `osum-browser` -- *every line of
executable code in the browser engine is Firn.* With that, Firn is no longer a
language for a kernel but **critical path number 1** of an ecosystem that
comprises the DOM, layout, a JavaScript engine, TLS/crypto and a rasterizer.
The authoritative requirements document:
`../osum-browser/FIRN-ANFORDERUNGEN.md` (13 sections, acceptance in 13).

Four decisions from v0.1 are **revised** as a result. They stand here openly,
because a specification that hides its about-turns is worthless:

| v0.1 said | v0.2 says | Why |
|---|---|---|
| "**No GC** -- not even in the app profile, never" | an **opt-in tracing GC** for marked types (3.5) | The DOM is a cyclic object graph, and the JS engine needs a collector anyway. Without a GC you end up on Gecko's route: reference counting plus a retrofitted cycle collector -- more work, not less |
| "**Inheritance** deliberately left out" | **single inheritance for `gc class`** (4.4) | The DOM *is* an inheritance hierarchy (`Node`->`Element`->`HTMLElement`->...) and is described that way normatively in Web IDL. Rebuilding it with composition costs more than it saves |
| "References are **always** second class" | Second class stays the **default**; `Gc[T]` and `Rc[T]` are **first-class** pointers (3.2) | Second-class references solve the lifetime problem elegantly, but in principle they cannot express a cyclic graph. That is exactly what the DOM needs |
| "A **WASM backend** in v0.6, aarch64 in v0.5" | both **postponed, without a date** (10.5) | `FIRN-ANFORDERUNGEN.md` 11 makes it explicit: the browser needs neither a WASM nor an aarch64 backend. WASM *execution* in the browser is an interpreter written in Firn, not a compiler backend. That relieves the scope considerably |

Newly added: 8 (strings, including **WTF-16**), 9 (**constant time** for
crypto), 5.3 (**unwinding/exceptions** for JS semantics), 6.4 (compile-time code
generation), 10.3 (the performance target of **<= 2x Rust**), 16 (traceability
from requirement to section).

Unchanged: the profiles (2), error handling through result types as the normal
route (5.1), `comptime` instead of a macro language (6), no LLVM in the
bootstrap path (10.2), the bootstrap stages (11), and the state of the stage 0
implementation (14).

### v0.1 (2026-08-13) -- first version
A language for kernels and applications, the prototype `firnc0` built and tested.

---

## 0. Why a language of our own at all

Osum is written in Rust today. That works, but it produces a dependency that
Justin does not want in the long run: Rust decides what the kernel may do (Rust
editions, `no_std` limits, LLVM target support, compiler bugs, project politics).
Whoever builds an operating system of their own but obtains the compiler from
others does not really own their system -- they rent it.

Since the browser decision the stakes are higher. Firn has to carry not only a
kernel but **half a million lines of browser engine**. That is the hardest test
there is for a language -- and at the same time the chance for Firn to grow up
along the way. A language design mistake costs not weeks but years in this
situation. That is why this document has become more detailed and decides more
than would be necessary for a kernel alone.

**Honesty up front:** this is a decade-long project. Rust took 9 years to reach
1.0, Zig is at 0.16 after 11 years and still not stable. This document describes
the *goal*; the accompanying compiler implements the section marked in 14 -- but
that part for real, all the way to a running binary.

---

## 1. Guiding principles

1. **Nothing happens hidden.** No implicit allocation, no implicit type
   conversions, no operator overloading, no hidden function calls.
2. **One language, two profiles.** Kernel code and application code share the
   syntax, the type system and the compiler. The difference is which
   *capabilities* are available -- not which dialect.
3. **Safe by default, unsafe on request.** Memory errors are compile errors,
   unless you write `unsafe`.
4. **Whoever does not order does not pay.** GC, vtables, unwinding tables,
   bounds checks: each of these capabilities costs only the code that requests
   it. The rasterizer, the tokenizer and the crypto primitives pay for none of
   it. This principle is new in v0.2 and the most important one.
5. **One way to do a thing.** Where C++ has five ways and Rust three, Firn has
   one.
6. **The compiler belongs to us.** No LLVM in the bootstrap path (10.2).
7. **Readability before brevity.** The compiler will be written in itself;
   unreadable code is a systemic risk then.
8. **Exceptions to these principles are named, not concealed.** Every place
   where Firn violates principle 1 or 5 (vtables, GC, unwinding) stands in this
   document with a justification and a price.

---

## 2. Profiles -- `kernel` and `app`

A module declares its profile in the first line:

```firn
profile kernel   // freestanding, no allocator, no runtime
profile app      // standard library, allocator, optional GC heap
```

Without an entry, `app` applies. The compiler switch `--profile=kernel` forces
the profile for the whole compilation unit.

**A TARGET WITHOUT AN OPERATING SYSTEM DOES THE SAME** (round
ARM-FREESTANDING). `--target=x86_64-none` and `--target=aarch64-none` name a
machine with nothing under it -- `none` is the word the GNU and LLVM triples
use for that -- and every property in the table below follows from the
absence rather than from a word in line 1. Such a target therefore TURNS THE
KERNEL PROFILE ON, and it is the weakest of the three sources: `--profile=`
wins over it, and a `profile` declaration in the source wins over it, so
nothing is silently reinterpreted. `profile app` together with a `-none`
target is a contradiction and is reported at the declaration.

Two consequences are worth stating, because they are what make the target
worth having: a freestanding source no longer has to say `profile kernel` at
all (the command line decides, so the same source can be built for a machine
WITH an operating system), and the x86-64 output is provably unchanged --
`--target=x86_64-none` and the plain build of a `profile kernel` source
produce the same octets (`tools/freestanding/none.sh`).

| Property | `kernel` | `app` |
|---|---|---|
| Heap allocation | only through an explicitly passed `Allocator` (built in round 73, `lib/mem/core_alloc.fi`) | a global allocator is available |
| Hidden allocation | forbidden (compile error) | forbidden (the same rule) |
| `Rc[T]` (counted, acyclic) | not available | available, explicit |
| **`Gc[T]` (tracing collector)** | **not available** | available, **opt-in per type** (3.5) |
| **Unwinding / `throw`** (5.3) | **forbidden** | allowed in `#[unwinds]` functions |
| Panic on an out-of-range index | calls `osum_panic`, configurable | run-time panic handler |
| Floating point | only with `#[allow_fp]` (the FPU state!) | free |
| Stack depth checkable (`#[max_stack]`) | yes | yes |
| Target binary format | ELF object, freestanding | ELF executable |

### 2.1 The standard library under `kernel` (round 73)

Up to round 72 EVERY `import std.*` was rejected in the kernel profile. That
was right in intent and too coarse in effect: `Span::trim`, `find`,
`compare`, the UTF-8 reader, `text_to_i64`, `digit_count` and the whole of
`math` ask nobody for memory and make no system call. They fell under the
ban only because they stood in the same FILE as the functions that call
`mmap`.

**The rule since round 73:** a module of the standard library may be
imported under `profile kernel` if it **declares `profile kernel` in its
own first line**. Everything else stays forbidden, with the message
unchanged.

That is not a name list in the compiler, and it is not taken on trust.
Firn compiles whole programs (12): an imported module lands in the **same
compilation unit** as the kernel that imports it, so every rule of this
section is checked on it as well -- a `syscall` hidden in such a module is
an error at the line where it stands, and so are `gc class`, `#[unwinds]`
and unmarked floating point. Whoever writes a new freestanding module needs
no compiler change; he writes one line and the compiler proves the rest.

The library was cut along that line in round 73:

| module | needs | kernel |
|---|---|---|
| `std.core` | nothing | **yes** -- `Span` and the whole reading text layer, `text_to_*`, `digit_count`, the UTF-8 reader, the `Allocator` interface, `Arena` |
| `std.math` | nothing | **yes** |
| `std.str`, `std.num` | an allocator (`Bytes`, `Str16`, `Atom`) | no |
| `std.vec`, `std.map`, `std.rc`, `std.mem` | an allocator (`mmap`) | no |
| `std.io`, `std.rt` | an operating system (`SYS_WRITE`, `SYS_READ`) | no |

`std.str`, `std.num` and `std.math` keep answering to every name they
answered to before: the pieces are include files (`lib/str/core_*.fi`,
`lib/num/core_*.fi`, `lib/math/core_math.fi`) and are put together into
both modules textually -- one source text, two front doors. The honest
price of a stage 0 without type aliases (14): whoever imports `std.core`
and `std.str` into one program gets `core.Span` and `str.Span` as two
distinct types of the same shape.

Proof: `demos/kernel/kcore.fi` says `import std.core`, compiles in the
kernel profile to a freestanding ELF object without an undefined name and
without a `syscall` instruction, and boots in QEMU
(`tools/core/run.sh`, test.sh section 30).

The browser runs in the `app` profile. The rasterizer, the tokenizer and the
crypto library run in the `app` profile as well, but use **no** GC types and are
marked with `#[no_gc]` in addition (3.5.4) -- the compiler then makes sure
statically that no collection can be triggered inside those functions.

---

## 3. The memory model -- the central decision

`FIRN-ANFORDERUNGEN.md` 1 calls it "the most important single decision. It makes
or breaks the whole project." That assessment is shared.

### 3.1 The problem, named exactly

Two kinds of memory stand opposite each other and have **opposite**
requirements:

* **Hot, acyclic data** -- tokenizer buffers, rasterizer edge lists, crypto
  state, image decoders. Millions of operations per second, not a cycle far and
  wide, every indirection and every barrier costs measurably.
* **The object graph** -- DOM nodes with parent *and* child references, live
  `HTMLCollection`s, event listener closures that hold on to their node, JS
  wrappers <-> DOM nodes, the layout tree -> the DOM tree, observers.
  **Cyclic throughout, across a language boundary.**

A single strategy cannot do both well. The mistake you can make here is to
decide for *one* of them.

### 3.2 The decision: three levels, ascending in cost

Firn has **three** kinds of reference. The default is the cheapest one; the
expensive ones have to be written down in the type.

| Level | Reference | Cost | Cycles? | First class? | For what |
|---|---|---|---|---|---|
| **1** | ownership + `&T` / `inout T` (second class) | **zero** | no | no -- parameters only | The default. Tokenizer, rasterizer, crypto, collections, 90 % of all code |
| **2** | `Rc[T]` / `Weak[T]` -- counted, immutable | one counter, atomic only with `Arc[T]` | **leaks on cycles** | yes | Shared *immutable* values: computed style values, interned atoms, font data |
| **3** | `Gc[T]` / `GcWeak[T]` -- a tracing collector | an assignment barrier + collection pauses | **yes, it resolves them** | yes | DOM nodes, JS objects, closures, observers -- and only those |

**Level 1 stays the normal case and is unchanged from v0.1:** one owner,
assignment moves (except for trivial types), references live only in the call
frame and may neither be stored nor returned. That is why Firn needs **no
lifetime annotations** and the checker stays intraprocedural. The details are in
3.3.

**Level 3 is the new thing in v0.2** and is described in detail in 3.5.

### 3.3 Level 1 -- ownership and second-class references (unchanged)

* Every value has **exactly one owner**. Assignment and hand-over **move**,
  unless the type is *trivial* (integers, `bool`, `f32`/`f64`, raw pointers,
  arrays of trivial types, structs with exclusively trivial fields). There is
  **no** user-definable copy constructor.
* `&T` -- shared, reading access, any number of them.
  `inout T` -- exclusive, writing access, exactly one.
* **Second-class status:** references may only be function parameters and may
  only be passed further down. Not in structs, not in arrays, not returned, not
  bound to long-lived variables.

It follows that a reference never outlives its call frame -- the reason why
there are no `'a` parameters. The models: Hylo/Val ("mutable value semantics"),
Swift's parameter conventions.

**The price, unchanged and honest:** no reference fields in structs, no
functions that return references, no self-referential structures. Iterators are
built as an index plus an accessor or as `with_element(i, fn(inout T))`. For
trees and graphs without a GC: `u32` handles into an arena array.

**Destruction:** deterministic at the end of the block, in reverse order of
declaration. `fn drop(inout self)` is a normal call at a predictable place.
`defer {}` for type-independent cleanup, `errdefer {}` only on the error path.
Moved values are no longer released (static move tracking; conditional moves are
rejected conservatively instead of being quietly given a run-time flag).
`#[must_consume]` enforces that a value is consumed (lock guards, `!T` results,
DMA buffers).

**Arenas** stay a first-class tool and are more important for the browser than
ever: parser nodes, intermediate layout results and style computation per frame
belong in an arena with `reset()`, not in the GC.

### 3.4 Level 2 -- `Rc[T]`: shared, but immutable

Satisfies `S7` from the requirements ("shared, immutable values with counting",
for computed style values -- Stylo does that with `Arc`).

* `Rc[T]` is **always immutable**. There is no `RefCell` equivalent, no interior
  mutability through `Rc`. Whoever wants to share and mutate takes `Gc[T]` or a
  lock.
* `Arc[T]` is the thread-safe variant (an atomic counter). A separate type, so
  that single-threaded code does not pay for the atomic counter.
  **Built since round 47** as the Firn module `lib/rc/arc.fi`
  (`Atomverweis[T]`, `AtomSchwachverweis[T]`); the counter is really atomic
  (`lock xadd`, `compiler/src/atomic.rs`, proof `tools/atomic/run.sh`). Named
  honestly: upgrading a weak reference needs a compare-and-swap that round 47
  does not build -- it is correct today (one thread, 7), but it is not a
  threading promise. `docs/ROUND47.md`.
* `Weak[T]` breaks cycles manually -- for cases where the cycle is obvious and
  local.
* **Cycles leak.** That stands in the documentation as such and is the reason
  why `Rc` is explicitly **not** intended for the DOM.

### 3.5 Level 3 -- the opt-in tracing GC

#### 3.5.1 Why at all, and why not the alternatives

Three routes were on the table (`FIRN-ANFORDERUNGEN.md` 1):

| Route | Result |
|---|---|
| **Arena + typed indices** | Resolves cycles elegantly and is fast. But it fails at the JS boundary: **the JS engine necessarily needs a collector**, because JS object lifetimes are not known statically. So you would have *two* systems, and the most dangerous class of cycle -- DOM node <-> JS wrapper -- runs exactly across their boundary. That is precisely where the leaks arise in real browsers |
| **Reference counting + weak references** | Predictable, but **cycles leak**. Gecko therefore retrofitted `nsCycleCollector`: a collector, only more complicated and slower than a proper one. The route ends where you did not want to go, only at a higher price |
| **A tracing GC, opt-in per type** | **Chosen.** The DOM and JS share one collector, the boundary does not exist. Ladybird demonstrates with `LibGC` that a manageable library of one's own is enough for it |

The decisive thing is the **opt-in**: it does *not* make Firn a GC language.
The GC is a heap that a type requests explicitly.

#### 3.5.2 What it looks like

```firn
gc class Node {
    parent:   GcWeak[Node]        // weak: breaks no cycle, but it is
    children: GcVec[Gc[Node]]     //       semantically right for observers
    listeners: GcVec[Gc[Closure]] // cycle node -> closure -> node: no problem
}

gc class Element extends Node {
    tag:   Atom
    attrs: GcMap[Atom, Str]
}

fn append(parent: Gc[Node], child: Gc[Node]) {
    parent.children.push(child)
    child.parent = weak(parent)      // the assignment barrier: only here
}
```

* `gc class` is the **only** way to declare GC-managed values.
  A `gc class` value exists exclusively on the GC heap; it does not exist on the
  stack and not as a field of an ordinary `struct`.
* `Gc[T]` is a **first-class** pointer: storable, returnable, placeable in
  collections. Exactly what level 1 cannot do and the DOM needs.
* `GcWeak[T]` satisfies `S3` (observers, caches, wrapper tables).
* `struct` stays what it was: flat, value-based, without a GC, without a vtable.

#### 3.5.3 How the collector works -- and what that costs

| Property | Decision | The price, named openly |
|---|---|---|
| **Heap tracing** | **precise**. Every `gc class` gets a `trace` function generated by the compiler from the known field layout | none worth mentioning |
| **Stack tracing** | **conservative** (the stack and the registers are searched for bit patterns that look like GC pointers) | Two real disadvantages: (a) **false retention** -- an integer that happens to look like a pointer keeps an object alive; (b) **no moving collector is possible**, because a root pointer found conservatively may not be rewritten. So **no compaction**, and the allocator has to deal with fragmentation through size classes |
| Why conservative all the same | Precise stack maps force the code generator to emit safepoint metadata at every call and restrict register allocation. That is a lot of work and slows down exactly the optimizer that has to reach <= 2x Rust according to 10.3. **Ladybird's `LibGC` scans conservatively too** and carries a real browser with it | The price is capped and measurable; the alternative costs months on the critical path |
| **When collection happens** | **only at allocation sites** of a GC type. No preemptive collection, no signals, no safepoint polls in loops | An endless loop without GC allocation blocks a collection -- acceptable, because it then produces no garbage either |
| **Algorithm** | mark-sweep, **incremental with tri-colour marking** from v0.5 on (`S5`), a Dijkstra insertion barrier when writing a `Gc[T]` field | The barrier costs -- but **only** when writing `Gc[T]` fields. Non-GC code never executes it (guiding principle 4) |
| **Pause times** | measurable through `gc.stats()`, boundable through `gc.set_budget(ms)` (`S6`) | |
| **Finalizers** | `fn finalize(inout self)` (`S4`), runs **after** collection, may **not** resurrect and may not create new GC objects. **Built since round 47**; stage 0 has no methods, hence a cleanup kind per object plus a dispatcher, and the three prohibitions are enforced at **run time** (a visible abort) instead of being checked by the compiler -- 14.1, `docs/ROUND47.md` | restricted, but predictable |
| **Threads** | One GC heap **per thread**, no hand-over of `Gc[T]` between threads (`Gc[T]` is not sendable, 7) | Parallel layout works on arena data, not on GC data. That is a real restriction and it stands here so that it is known when the layout is designed |

#### 3.5.4 `#[no_gc]` -- the guarantee for hot paths

```firn
#[no_gc]
fn tokenize(input: &[u8], out: inout TokenBuf) -> usize { ... }
```

Inside a `#[no_gc]` function the following are forbidden: GC allocation, calling
functions without `#[no_gc]`, writing into `Gc[T]` fields. The compiler checks
that **transitively** and reports an error with line/column if the chain breaks.
It follows statically: **no collection can take place inside this call tree**,
there is no barrier and no pause. That is the promise to the rasterizer, the
tokenizer and the crypto code.

#### 3.5.5 Acceptance

`FIRN-ANFORDERUNGEN.md` 13 item 2 / `TODO-FIRN.md` 0.9: a DOM prototype with
parent/child cycles and listener cycles, **a 24 h soak test without memory
growth**. Until that test runs, the GC counts as *designed*, not as
*demonstrated*. See `ACCEPTANCE.md`.

### 3.6 Raw pointers and `unsafe`

`*T` and `*mut T`. Dereferencing, pointer arithmetic, `transmute`, MMIO and
inline assembly only inside `unsafe { }`. `unsafe` unlocks **only** these
operations; moves and type rules keep applying. Every block needs a
justification as a comment (`--deny=undocumented-unsafe`, the default in the
`kernel` profile). Satisfies `L3` -- the rasterizer cannot be written fast
without this exception.

#### 3.6.1 A raw pointer must not outlive its frame (round 79)

The address of a LOCAL -- a `let`/`var`, an array or a field of one, or a
parameter, whose slot lies in the frame too -- must not leave the frame that
local lives in:

```firn
fn bad() -> *mut i64 {
    var x: i64 = 5
    return &x          // error: x is dead on return, the pointer is not
}
```

Checked at compile time by `compiler/src/escape.rs` and its twin
`lib/firnc1/escape.fi`. Caught are: the return value, a write through a
pointer that is not this frame's (an out parameter included), a store into a
struct or an array that is returned, the argument of the thread primitive,
and the handover to a function that KEEPS what it is given -- the last one
without any annotation, because the compiler works that property out of the
callee's body and drives it to a fixed point over the whole program.

**This is not a lifetime system.** Where the analysis cannot decide, it
allows; every such case is named in `docs/ROUND79.md` 4 instead of being
guessed at. A false alarm would cost more than a missed case: it makes
correct programs unbuildable and teaches everybody to switch the check off.

**The way out** is `#[allow_escape]` in front of a function (14.2). A
hardware address, a page table, a stack a thread is handed, and the stack
pointer the collector needs for its conservative scan (3.5.3) do have to
leave the frame. The attribute stays visible in the source and switches the
check off for exactly that function -- never globally and never silently.

---

## 4. Where the ideas come from -- and what is deliberately missing

**From Rust:** ownership and moves, `unsafe` as a boundary, result types instead
of exceptions in the normal case, enumerations with payload and exhaustive
pattern matching, a module system with explicit exports, expression orientation.

**From Zig:** `comptime` instead of templates and macros, "no hidden control
flow", explicitly passed allocators, `defer`/`errdefer`, errors as a value type,
compile-time reflection, `test` blocks in the source file.

**From C:** a flat, predictable memory layout, direct mappability onto machine
instructions, a small core language, honesty about the ABI.

**From Ladybird/`LibGC` (new in v0.2):** the opt-in tracing GC with a
conservative stack scan as a route proven to carry a small team.

### 4.4 Inheritance -- the about-turn, justified openly

v0.1 ruled inheritance out. v0.2 allows it **in a restricted form**:

* **only for `gc class`**, not for `struct`, not for enumerations
* **only single** (one base), no multiple inheritance
* fields are **never** virtual; methods only with `virtual` / `override`
* upcasting is free (`Gc[HTMLElement]` -> `Gc[Node]`), downcasting is checked:
  `node.as?[HTMLInputElement]` returns `?Gc[HTMLInputElement]` and checks the
  type tag at run time

The reason: the DOM is described **normatively** in Web IDL as an inheritance
hierarchy -- `Node` -> `Element` -> `HTMLElement` -> `HTMLInputElement`, over a
hundred classes deep. `L6` explicitly demands static **and** dynamic dispatch.
The alternative (composition with a `base` field plus manual upcasting and
hand-built type tags) rebuilds inheritance, only worse and with more manual
work. Where a language has to model a domain that is itself a hierarchy,
inheritance is the honest tool.

**The price:** Firn thereby has vtables and violates guiding principle 1
("nothing hidden") in one place. The price is bounded: only `gc class`, only for
methods marked `virtual`, and `struct` stays entirely free of it. A rasterizer
never sees a vtable.

### 4.5 Deliberately left out

| Left out | Reason |
|---|---|
| Lifetime parameters (`'a`) | the largest driver of complexity in Rust; 3.3 makes them superfluous |
| Multiple inheritance, classes for value types | 4.4 is the narrowly drawn exception |
| Operator overloading | `a + b` is supposed to be an addition |
| Implicit conversions (even lossless ones) | `u8 -> u32` is written `as u32` |
| Exceptions as the *normal* error route | `L7`: parsers produce expected errors all the time; exceptions for that are poison for speed. 5.3 is the narrowly drawn exception for JS |
| Macros with a syntax of their own | `comptime` and build scripts are enough (`FIRN-ANFORDERUNGEN.md` 11: "no Turing-complete macro system") |
| `async`/`await` **as a language colour** | 7 -- replaced by `Io` as a parameter (`DESIGN_GOALS.md` 1) |
| Overloading function names | it makes error messages and self-hosting harder |
| Automatic dereferencing | `p.*.field`, not `p.field`. The exception: `Gc[T]` is dereferenced automatically, because `node.*.children.*` would be unreadable -- that is deliberate and it stands here |
| A preprocessor | `comptime if` replaces `#ifdef` |
| **JIT / code generation at run time** | `FIRN-ANFORDERUNGEN.md` 11: the JS engine is a bytecode interpreter. Ladybird removed its JIT again in 2024, V8 has a jitless mode |
| **Dynamic libraries** | linked statically; saves `dlopen` in Osum entirely (`R5`) |
| **C++ interop** | at B1 there is no C++ code. That is exactly what cost Ladybird 1.5 years with Swift |

---

## 5. Error handling

Three strictly separated categories. The third one is new in v0.2.

### 5.1 Expected errors -- result types (`L7`, the normal route)

```firn
error IoError { NotFound, Permission, Closed }

fn read_all(path: &Str, alloc: inout Arena) -> IoError!Buf {
    let fd = try open(path)
    errdefer close(fd)
    ...
    return buf
}

let buf = read_all(&p, inout a) catch |e| {
    match e { IoError.NotFound => return default_buf(), else => return e }
}
```

`!T` is a union of an error set and a success type; the error set may be left
out and **inferred** by the compiler. An unhandled `!T` cannot be dropped
(`#[must_consume]`). No unwinding, no landing pads -- this is the path a parser
walks a million times.

### 5.2 Programming errors -- `panic`

An index out of range, division by zero, a violated assertion. Not handleable,
calls a handler (`app`: a message plus abort; `kernel`: `osum_panic`).
`--release-fast` switches the checks off; the case is then undefined and that
stands in the documentation as such.

**A stack overflow (`L12`)** belongs here: the HTML tree builder and the JS
interpreter recurse deeply, and a deeply nested page must not be a crash.
Firn solves that with (a) guard pages that trigger a clean panic path instead of
a `SIGSEGV`, and (b) the intrinsic `stack_remaining() -> usize`, with which a
recursive descent can give up before the next step -- that is the route the HTML
specification provides for nesting limits anyway.

### 5.3 Unwinding for JS semantics (`L8`) -- new in v0.2

JavaScript has `throw`, Web IDL throws `DOMException`, and both have to travel
back through hundreds of call levels of the interpreter and the bindings.
Routing all of it through `!T` would mean making every single IDL function a
result function -- the effort and the amount of code would be considerable.

**The decision: Firn gets unwinding, but strictly fenced in.**

* Only functions with `#[unwinds]` may raise or let through a `throw`.
  Without that marking a function is unwind-free -- the compiler checks that
  transitively, as with `#[no_gc]`.
* **Table-driven unwinding in two phases**, no `setjmp`. The cost on the success
  path: **zero instructions**. The cost in the binary image: unwinding tables --
  but only for `#[unwinds]` functions.
* While unwinding, the `drop`s and `defer`s of the frames being left run. That
  is the place where unwinding is expensive to implement, and it is explicitly
  named here as an item of effort.
* In the `kernel` profile `throw` is **forbidden**.
* The `catch` in 5.1 (for `!T`) and the `catch` in 5.3 (for `throw`) are
  **different** constructs with different syntax (`catch |e|` versus
  `try { } catch (e) { }`), so that they do not get confused.

**An honest assessment:** this is the second place (after 4.4) where the browser
forces something on Firn that a system language would otherwise not need. The
fence (`#[unwinds]`) is the countermeasure.

---

## 6. Metaprogramming

### 6.1 `comptime` -- generics without a second type system

```firn
fn max[comptime T: type](a: T, b: T) -> T {
    comptime assert(is_ordered(T), "max requires an orderable type")
    return if a > b { a } else { b }
}

struct Vec[comptime T: type] { data: *mut T, len: usize, cap: usize }

comptime if target.arch == Arch.X86_64 { ... } else { ... }
```

Monomorphization (`L5`), no type erasure, no hidden vtables.
**A drawback named openly:** errors in generic code only show up at
instantiation. Mitigated by `comptime assert` and declared requirements
(`where has_method(T, "next")`), but it stays worse than Rust's traits.

### 6.2 Interfaces (`L6`)

`interface` with **static** dispatch (monomorphized, the standard case) and
**dynamic** dispatch through `dyn Interface` (a vtable, written down
explicitly). Together with 4.4 that covers the DOM hierarchy: `gc class` for the
hierarchy itself, `interface` for cross-cutting capabilities (`EventTarget`,
`Serializable`).

### 6.3 Enumerations and pattern matching (`L4`)

Sum types with payload, `match` with an **exhaustiveness check at compile
time** -- an uncovered case is an error, not a warning. The HTML tokenizer has
around 80 states, CSS values a huge set of variants; without both of these it
becomes unmaintainable. The code generator produces **jump tables** where the
variants lie densely (`P4`) -- 80 states must not be 80 comparisons.

### 6.4 Compile-time code generation (`G1`-`G4`) -- new in v0.2

A browser consists to a considerable extent of *generated* code: Web IDL
bindings (Ladybird: 697 `.idl` files), CSS property tables, 2,231 HTML entities,
Unicode tables from the UCD, CLDR data.

* **Build scripts** (`build.fi`) run before compilation, are ordinary Firn
  programs and write Firn source text. No Turing-complete macro system --
  deliberately, see 4.5.
* Generated source text is readable and keeps its relation to the input file
  through `#line` entries, so that the debugger shows something sensible (`G2`).
* For large static tables (`G4`) the standard library provides **perfect
  hashing** and **compressed tries** as a build-time tool.
* Acceptance (`FIRN-ANFORDERUNGEN.md` 13 item 6): a Unicode table is generated
  from the UCD.

---

## 7. Concurrency

* **Building blocks in the language:** `atomic[T]` with an explicit memory
  order, `fence`, and the rules from 3. A happens-before memory model (`N2`).
* **Data race prevention (`N4`):** two markings, corresponding to Rust's
  `Send`/`Sync`, but derived by the compiler instead of implemented by hand:
  `#[sendable]` and `#[shareable]`. **`Gc[T]` is neither of the two** -- GC
  heaps are thread-local (3.5.3). Parallel layout therefore works on arena data
  and `Arc[T]` style values, not on DOM nodes. That restriction is a consequence
  of the GC decision and it stands here so that it is known when the layout is
  designed, and not first while debugging.
* **No `async`/`await` as a language colour** (revised 2026-08-14, justified in
  detail in `DESIGN_GOALS.md` 1). An `async` marking colours every caller and
  tears the ecosystem apart -- which is why there are two incompatible I/O
  worlds in Rust. Firn adopts **Zig's model from 0.16** instead:
  **`Io` is passed as a parameter**, exactly like the `Allocator`.
  `io.async(f, ...)` expresses *independence* and is infallible;
  `io.concurrent(...)` demands real concurrency and may fail.
  `Future[T]` is `#[must_consume]`, cancellation (`cancel`) is part of the
  contract. With that `N6` is **satisfied without touching the compiler** --
  there is no state machine transformation and no `async` keyword. The
  implementations: `Io.Threaded`, `Io.SingleThread` (deterministic, satisfies
  `N7`), later `Io.Evented` with stackful coroutines. The price: one stack per
  coroutine, and `Io` has to be passed through.
* **Structured concurrency** as a library pattern on top of `#[must_consume]`.
* **A deterministic single-thread mode** for reproducible reftests (`N7`).

---

## 8. Strings -- new in v0.2

`FIRN-ANFORDERUNGEN.md` 3 calls this "underestimated, but critical". Rightly so:
the string type runs through every module and cannot be changed afterwards.

### 8.0 `str` -- the language type (round 70)

Everything from 8.1 on describes the LIBRARY types. Above them sits **one**
type that the language itself knows:

```firn
let text: str = "hello"          // the literal IS a str
if text == "quit" { ... }        // == compares the CONTENT
let greeting: str = text + "!"   // + concatenates
```

**Since round 88 the type has a second spelling: `string`** -- out of the
C# flavoured alias family of round 70/71 (13). It is an alias, not a second
type: `let x: string = "test"` and `let y: str = "test"` mean exactly the
same thing, pass into each other's functions without a cast, and every error
message names the canonical `str`. Proof: `tools/firstrun/run.sh` case 08
and counter-check D.

* **What a `str` is:** two machine words, `p: *mut u8` and `n: usize` --
  exactly the layout of `str.Span`. A `str` is a VIEW of octets that nobody
  may change any more.
* **Immutable.** There is no operation that writes into the octets behind a
  `str`. Whoever wants to build text takes `Bytes`.
* **Substrings cost nothing.** `s.trim()`, `s.part(a, b)`, `s.ab(k)` move two
  words; the octets stay where they are. No copy.
* **`str` and `Span` may be used for each other** -- same two words, same ABI.
  That is why the whole library of 8.1 works on a `str` without a conversion
  function: `s.trim()`, `s.length()`, `s.starts_with(...)`, `s.find(...)`.
  **Since round 88 that holds for the METHODS as well:** where `Type__method`
  finds nothing, the method resolution asks the layout compatible views --
  every one of the 22 methods of `impl Span` (`part`, `ab`, `to`, `find_back`,
  `utf8_part`, ...) is reachable on a `str`. Up to round 87 only those
  resolved for which the module `str` happened to have a FREE function of the
  same name; that was an accident of the naming scheme, not a rule.
* **Who owns the octets:**

  | origin | storage | freed by |
  |---|---|---|
  | literal `"hello"` | the frame of the enclosing function | the frame |
  | `a + b` | the **GC heap** | the collector |
  | out of a `Span`/`Bytes` | wherever the buffer lies | its owner |

  Nothing is copied silently. The ONLY places where octets are copied are
  `a + b` and `__str_copy` (which `io.read_line` uses so that a line survives
  its buffer).
* **`+` allocates and therefore needs the collector.** A program that works
  with `str` pulls the collector runtime in automatically; the signal is read
  off the tokens -- the type name `str`, or a text literal next to `+`, `==`,
  `!=`. **Since round 88 the compiler also writes the setup:** where it links
  the runtime in, `gc_init()` stands as the first instruction of the process,
  in `_start`, before the first instruction of the user, exactly once. Nobody
  has to call it any more to join two pieces of text.

  An explicit `gc_init()` in the source text keeps working and sets up
  nothing a second time (it is idempotent); `gc_set_max_bytes` afterwards
  keeps working too. Under the profile `kernel` nothing of this arises --
  there is no `_start` there and no collector (2).

#### The literal -- and why nothing breaks

A text literal is **decided by the context**:

* where an **array type** is wanted, it is the array literal it has been since
  round 39: `var t: [u8; 20] = "...\0"` keeps working unchanged, including the
  null octet, the exact length and the message on a length mismatch;
* **everywhere else** it is a `str`.

That cannot change the meaning of any existing program: a text literal WITHOUT
an array context is an error today ("the type of the array literal cannot be
inferred"). `b"..."` and `u"..."` stay array literals -- `str` holds octets,
and a sequence of `u16` is not that.

`""` is a valid, EMPTY `str` (`p = 0`, `n = 0`); as an array literal it stays
an error, because an array needs at least one element.

#### `[T; _]` -- the length comes out of the literal (round 79)

```firn
var greeting: [u8; _] = "hello"        // 5
var terminated: [u8; _] = "abc\0"      // 4, the null octet counts
var numbers: [i64; _] = [10, 20, 30]   // 3
var zeros: [u8; _] = [7 as u8; 5]      // 5
```

Writing the count out by hand is a source of error that the compiler cannot
catch: too small a count is an error, but the usual reaction is to pad the
text with blanks until it fits -- and then the LENGTH passed on afterwards is
a second, independent number that no reader can check. `_` removes the whole
class.

Scope, binding: **the outermost length only** (`[[u8; _]; 3]` is an error)
and **only out of a literal** -- a text literal, an array literal or
`[v; n]`. A parameter, a field and a `const` have no initializer to take a
length from and have to write the number out. Both refusals are compile
errors with line and column. The PARSER fills the length in as soon as it has
read the initializer, so nothing after it ever sees a `_`.

### 8.1 Four separate types

| Type | Contents | Well formed? | For what |
|---|---|---|---|
| `Bytes` | raw octets | -- | Network data, files, image data. **Not text** (`Z7`) |
| `Str` | UTF-8 | **guaranteed** -- checked at the boundary | Everything internal to the web platform (`Z1`) |
| **`Str16`** | a sequence of `u16` code units | **explicitly not** | **JavaScript strings** (`Z2`) |
| `Atom` | a `u32` tag into a global table | -- | Tag, attribute and property names (`Z4`) |

The separation of `Bytes` and `Str` is enforced by the compiler: `Bytes` never
becomes text silently.

### 8.2 `Str16` and WTF-16 -- the point almost everybody overlooks

JavaScript strings are sequences of 16-bit code units and may contain
**individual, unpaired surrogates**. `"\uD800"` is a valid, everyday JS string.
A language whose string type only allows well-formed Unicode **cannot implement
JavaScript correctly** -- it would either replace (a wrong result) or abort
(wrong behaviour).

* `Str16` checks **nothing** and normalizes **nothing**. It is a `[]u16` with
  string operations. That is intentional, not an omission.
* Conversions are **explicit** and their fallibility stands in the type:
  * `Str -> Str16` always succeeds
  * `Str16 -> Str` is fallible: `to_utf8() -> ?Str` (nothing with an unpaired
    surrogate) and `to_utf8_lossy() -> Str` (replaced by U+FFFD) respectively
  * **`Wtf8`** as a lossless bridge: it can hold unpaired surrogates in a
    UTF-8-like encoding. For caching and IPC
* A negative check is part of the acceptance: a test constructs a `Str16` with
  an unpaired surrogate and checks that it is **preserved**, that `to_utf8()`
  returns nothing and that `to_utf8_lossy()` returns U+FFFD.

### 8.3 Atoms (`Z4`)

Tag and attribute names are compared millions of times during selector matching.
`Atom` is an interned `u32` tag; comparison is an integer comparison. Frequent
atoms (`div`, `class`, `id`, ...) are assigned at **build time** (6.4) and have
fixed, small numbers -- which makes `match` jump tables over tag names possible.

### 8.4 Numbers <-> text (`Z5`, `Z6`)

Two requirements that are guaranteed to be wrong if implemented naively:

* **`strtod`, correctly rounded.** CSS numbers and `parseFloat` have to be
  bit-exact. The implementation: a fast route over 128-bit fixed point
  (Eisel-Lemire) with a fallback to exact big number arithmetic in the edge
  cases.
* **The shortest output with a round-trip guarantee** (Ryu/Grisu class).
  `Number.prototype.toString` is prescribed **exactly** in ECMAScript; a naive
  output fails test262.

Both belong in the standard library, not in the compiler, and are checked
against the public test vectors.

### 8.6 `v128` -- the vector register (round 82)

A **primitive type of the language**: 128 bits, sixteen octets, sixteen byte
aligned, at home in one SSE register. It is what makes the vector and crypto
instructions of the processor reachable from Firn at all -- without it
`aesenc` and `sha256rnds2` cannot be emitted, and a scalar AES loses a factor
of a hundred and ninety against one that uses them (9.4, docs/ROUND82.md).

**`v128` carries NO element type.** Sixteen octets mean whatever the
instruction applied to them says they mean; `__v128_add32` reads them as four
`u32`, `__aesenc` as one AES state. A type per reading (`u8x16`, `u32x4`)
would only produce conversions that generate no code.

**`v128` has NO operators.** No `+`, no `^`, no `<<`, no `==`. `a + b` would
have to mean one of `paddb`/`paddw`/`paddd`/`paddq`, and any choice is wrong
three times out of four. Everything happens through named intrinsics whose
name says which machine instruction comes out.

**The intrinsics** are spelled `__name(...)`, like every other primitive of
this language (`__atomic_add`, `__mmio_read32`, `__gc_state`). The set of
round 82: `__v128_load`/`store`/`zero`/`from_u64`/`get_u64`/`get_u32`/
`set_u32`, `__v128_xor`/`and`/`or`/`andnot`, `__v128_add8`/`add32`/`add64`/
`sub32`, `__v128_shuffle8`/`shuffle32`/`alignr`/`unpacklo32`/`unpackhi32`/
`unpacklo64`/`unpackhi64`/`shl_bytes`/`shr_bytes`/`shl32`/`shr32`/`shl64`/
`shr64`/`blend16`, `__aesenc`/`aesenclast`/`aesdec`/`aesdeclast`/`aesimc`/
`aeskeygenassist`, `__sha256rnds2`/`sha256msg1`/`sha256msg2`, `__pclmulqdq`,
`__crc32_u8`/`__crc32_u64` and `__cpu_features`.

An immediate operand (`__v128_shuffle32`, `__v128_alignr`, `__v128_blend16`,
`__aeskeygenassist`, `__pclmulqdq`, the byte and bit shifts, the extractors)
**must be an integer literal** and is range checked at compile time. It is
encoded into the instruction; there is no register form of it.

**The calling convention:** a `v128` parameter travels in the SSE class of
System V AMD64 (`xmm0`-`xmm7`, then the stack), a `v128` result comes back in
`xmm0` -- exactly like `f32`/`f64`. All sixteen `xmm` registers are caller
saved, so nothing is rescued in a prologue.

**Availability is a RUN TIME question.** `__cpu_features() -> u64` asks `cpuid`
and yields a bit set (`lib/std/cpu.fi` names the bits). The compiler inserts
no check by itself: it cannot know which two implementations you consider
equivalent. Whoever uses AES-NI asks first and keeps a path that does not.
`lib/std/crypto/accel.fi` is the worked example, and both of its paths are
held against the same NIST vectors (`tools/bench82/run.sh`).

**Deliberately not in round 82:** 256 and 512 bit vectors (`v256`/`v512`, they
need `vzeroupper` discipline), a second register class in the linear scan (a
function with a `v128` in it goes over the base path of the code generator,
as one with an `f64` has since round 71), and the aarch64 equivalents
(`aese`/`aesmc`, `sha256h`) -- see docs/ROUND82.md §6 and §7.

---

### 8.5 Ropes (`Z3`)

`document.write` and JS concatenation in loops produce quadratic costs if every
concatenation copies. A rope structure (`Rope`) is planned as a library type --
a **SHOULD**, not a **MUST**, and therefore without a date.

---

## 9. Constant time and crypto -- new in v0.2

`FIRN-ANFORDERUNGEN.md` 7: "the point almost everybody overlooks." At B1 TLS 1.3
is written from scratch. And here the requirement stands **in direct
contradiction** to 10.3: an optimizer that rewrites `if (secret)` into a jump or
removes a `memset` of key material as dead code destroys the security. That is
exactly why this has to go into the language design **now** and cannot be
retrofitted -- an optimizer built without a brake cannot be talked round later.

### 9.1 `secret[T]` -- a secret as a type

```firn
fn ct_eq(a: &[secret[u8]], b: &[secret[u8]]) -> secret[bool] {
    var acc: secret[u8] = 0
    var i: usize = 0
    while i < a.len {
        acc = acc | (a[i] ^ b[i])      // allowed: data independent
        i = i + 1
    }
    return acc == 0                     // yields secret[bool]
}
```

`secret[T]` is a type qualifier on integer and `bool` types. The type checker
forbids on it:

* **Branching**: `if secret_bool { }` is a **compile error**, not a warning.
  Instead `select(cond, a, b)` -> `cmov`.
* **Indexing**: `table[secret_index]` is an error (a cache side channel, `C4`).
  Instead a constant-time selection pattern over the whole table.
* **Division** and `%`: variable latency on real hardware -- forbidden.
* **Output** or conversion to non-`secret`: only through `declassify(x)`, which
  stands out in the source text and can be searched for in reviews.

The marking propagates through expressions: `secret[u8] ^ u8` yields
`secret[u8]`.

### 9.2 The promise of the optimizer (`C1`, `C2`)

The `secret` marking is carried down into the IR. For values with this marking
the following holds for **every** optimization pass:

* a `select` may **never** be rewritten into a branch
  (the obvious "optimization" for predictable conditions is forbidden)
* memory accesses may not be transformed into data-dependent accesses
* stores to `secret` memory are **never** considered dead
* `barrier(inout x)` is an opaque barrier that every pass respects

**Checkable, not merely claimed:** `#[constant_time]` on a function switches on
a check **in the code generator**. If a conditional jump arises there whose
condition depends on a `secret` value, compilation aborts with an error. That is
stronger than a downstream test -- but the test (an inspection of the assembly)
stays in addition, because a check in the compiler can be wrong too.

### 9.3 Further crypto requirements

| # | Requirement | The implementation in Firn |
|---|---|---|
| `C3` | erase memory reliably | `secure_zero(inout buf)` -- built in, untouchable for DCE |
| `C5` | 128-bit arithmetic | `u128`/`i128` as real types; in addition `mul_wide(u64, u64) -> (u64, u64)` as an intrinsic (maps onto `mul`) |
| `C6` | AES-NI, SHA extensions, CLMUL | through inline assembly (`L15`) and, later, named intrinsics -- a **SHOULD** |
| `C7` | the system CSPRNG | `sys.random(inout buf)`, a mandatory part of the runtime in the `app` profile |

---

## 10. The backend strategy

### 10.1 The structure

```
source .fi
  +-> lexer --> parser --> AST
        +-> resolver (names, modules)
              +-> type checker + move/reference checker + secret check
                    +-> HIR (desugared: for->while, comptime resolved,
                             monomorphization)
                          +-> FIR  (own IR, SSA-like, basic blocks,
                                    carries the secret and gc markings)
                                +-> optimizer (10.3)
                                +-> backends:
                                     +- x86_64  (own codegen)   <- the only target
                                     +- aarch64 (postponed, 10.5)
                                     +- wasm32  (postponed, 10.5)
                                     +- llvm-ir (optional, never in the bootstrap path)
```

**FIR** is the breaking point by design: typed, basic blocks with exactly one
terminator, no x86 peculiarities, documented in `docs/FIR.md`, with a text
format for tests. New in v0.2: FIR carries the `secret` marking (9.2) and knows
which values are GC pointers (for the generated `trace` functions).

### 10.2 Why not LLVM -- unchanged, but more expensive now

The justification from v0.1 still holds: independence is the goal of the
project; self-hosting demands that the compiler written in Firn does not have to
serve a C++ ABI; in the kernel LLVM produces calls of its own accord (`memcpy`,
floating point helpers) and has its own ideas about stack probing; and the
entire compiler is supposed to be reproducibly buildable in under a minute.

**What has changed:** with the performance target from 10.3 this decision is
considerably more expensive than in v0.1. Back then "2-5x slower than LLVM" was
acceptable. Now the requirement reads `P9`: **<= 2x Rust**, measured. That means
a considerable part of the compiler work goes into the optimizer, not into
language features.

An additional argument has come along in v0.2 and it weighs heavily: **9 demands
an optimizer with a brake.** LLVM does not give that promise -- there is no
dependable guarantee in LLVM that a `select` will not become a jump after all.
Whoever is serious about constant-time crypto is even better off with an
optimizer of their own. That was not a planned advantage in 2026, but it is one.

**The door stays open:** an `llvm` backend behind a switch, as a yardstick
("what would LLVM have made of it?"). Never in the bootstrap path.

### 10.3 The performance target: <= 2x Rust (`P1`-`P9`) -- new in v0.2

**This is the requirement on which a Firn-only approach can realistically
fail.** An HTML tokenizer runs over every character of every page. If it is 10x
too slow, the browser is 10x too slow, and that cannot be optimized away later.

The mandatory passes in the optimizer:

| # | Pass | Why |
|---|---|---|
| `P1` | **inlining, across module boundaries as well** | without it every accessor call is a real call |
| `P2` | constant folding, simplification, dead code elimination | the basis, present in stage 0 |
| `P3` | **register allocation** (linear scan; graph colouring later) | stage 0 has *none* -- it puts every value on the stack. That is the largest single item |
| `P4` | **jump tables for `match`** | 80 tokenizer states must not be 80 comparisons |
| `P5` | remove bounds checks where provable | otherwise memory safety costs 20-40 % in the rasterizer |
| `P6` | hoist loop invariants, unrolling | a **SHOULD** |
| `P7` | **layout control over structures** (`#[packed]`, `#[align]`, a fixed field order) | a DOM node is created by the million |

**Measurement, not assertion:** a microbenchmark suite compares the same
programs in Firn and in Rust on the same machine. The result is documented as a
number -- **even when the target is missed**. An honestly measured "3.4x slower"
is valuable; an asserted "roughly 2x" is worthless.

### 10.4 Debug information and tools (`W1`-`W9`)

500,000 lines without a debugger are unmaintainable (`W3`). So the backend
includes: **DWARF basics** (the line table, function ranges, later variable
locations), a **test runner with machine-readable output** (`W2` -- without it
there are no WPT rates and no progress curve), **package management with
reproducible builds** (`W1`), as well as a **profiler** (`W4`) and a **fuzzing
hookup** (`W5`), the latter because parsers and image decoders are the main
attack surface at B1 and there is no hardened foreign library.

### 10.5 What has been postponed

`FIRN-ANFORDERUNGEN.md` 11 makes clear what is **not** needed:

* **no aarch64 backend**, until Osum targets ARM
* **no WASM backend** -- WASM *execution* in the browser is an interpreter
  written in Firn, not a compiler backend
* **no JIT**, no code generation at run time
* **no dynamic libraries**, **no C++ interop**

v0.1 had planned aarch64 for v0.5 and WASM for v0.6. Both are struck and without
a date. The dream "Firn in the browser instead of JavaScript" remains -- but it
is a *later* goal now and blocks nothing.

---

## 11. The bootstrap plan

| Stage | written in | compiled by | Result | Status |
|---|---|---|---|---|
| **0** | Rust | `cargo`/`rustc` | `firnc0` -- compiles the subset from 14 | **runs** |
| **1** | Firn (the subset in 14) | `firnc0` | `firnc1` | v0.3 |
| **2** | Firn (the full scope) | `firnc1` | `firnc2`, after which `firnc2` compiles itself | v0.4 |
| **3** | -- | -- | **fixpoint:** `firnc2` and `firnc2'` bit-identical -> self-hosting (`L1`, acceptance 13.1) | v0.5 |
| **4** | -- | -- | `firnc0` frozen, a precompiled `firnc1` archived (a countermeasure against "trusting trust") | v0.6 |

The rules: stage 1 may only use what `firnc0` can handle. Every stage is checked
by the **complete** test suite, not by "it builds". The fixpoint comparison is
the only statement about correctness that carries weight.

---

## 12. Syntax

```firn
profile app

import std.io

const MAX: u32 = 100

struct Point { x: i32, y: i32 }

enum Shape { Circle(f64), Rect(Point, Point) }

gc class Node { parent: GcWeak[Node], children: GcVec[Gc[Node]] }

fn dist2(p: &Point, q: &Point) -> i32 {
    let dx = p.x - q.x
    let dy = p.y - q.y
    return dx * dx + dy * dy
}

fn main() -> i32 {
    let p = Point{ x: 3, y: 4 }
    let q = Point{ x: 0, y: 0 }
    if dist2(&p, &q) == 25 { return 0 } else { return 1 }
}
```

`let` is immutable, `var` is mutable. Square brackets for generic parameters
(`Vec[u8]`), so that `<` unambiguously stays a comparison and the parser gets by
without backtracking. Semicolons are optional. Visibility through an `export`
list per module, not on the individual element.

### 12.7 Compound assignment and the step operators (round 70)

```firn
x += 5      x -= 5      x *= 5      x /= 5      x %= 5
x &= m      x |= m      x ^= m      x <<= 3     x >>= 3
x++         x--
```

**`x op= e` is EXACTLY `x = x op e`** -- the same type rules, the same
message on a mismatch, the same instruction and therefore the same overflow
behaviour (checking in `--debug`, wrapping in `--release-fast`, 13). It is
not a second kind of arithmetic; it is a shorter way of writing the same
one. In the compiler both go through the very same check
(`sema::binop_type`), so that they cannot drift apart.

**The left side is evaluated ONCE.** `a[f()] += 1` calls `f()` exactly once.
That is a guarantee of the language, not a property of the optimizer: the
compound assignment is its own statement, and the lowering computes the
address of the target once and then loads, computes and stores through it. A
rewrite into `a[f()] = a[f()] + 1` in the parser would be the classic
mistake here; `tests/1338_assign_op_once.fi` counts the calls and checks the
counting itself with the written out form as a counter-check.

**`++` and `--` are STATEMENTS, never expressions.** `y = x++` does not
exist here, and neither does `a[i++]`. The reason is not taste:

* the difference between prefix and postfix inside an expression is one of
  the most productive sources of error in C -- `*p++` and `(*p)++` mean
  different things, and readers get it wrong;
* in C++ the ORDER OF EVALUATION around it is even undefined: `i = i++ + 1`
  has no defined meaning, and `f(i++, i++)` may compute anything.

As a pure statement on a line of its own the meaning is unambiguous, and
nothing is lost: whoever wants the old value writes it down.

`let` stays immutable. `x += 1` on a `let` binding runs into exactly the
same wall as `x = x + 1`, with the same message.

### 12.1 The grammar of the v0 subset (EBNF)

This is the grammar that `firnc0` **really** implements. The extensions from
3-9 (`gc class`, `enum`/`match`, `interface`, `secret`, `throw`) are
deliberately not contained in it yet.

```ebnf
program     = { item } ;
item        = fn_decl | struct_decl | const_decl | profile_decl
            | import_decl | export_decl ;          (* round 2 *)
profile_decl= "profile" ident ;
import_decl = "import" ident { "." ident } ;       (* round 2 *)
export_decl = "export" "{" [ ident { "," ident } ] "}" ;   (* round 2 *)
fn_decl     = [ "extern" ] "fn" ident "(" [ params ] ")" [ "->" type ] block ;
params      = param { "," param } [ "," ] ;
param       = ident ":" type ;
struct_decl = "struct" ident "{" { ident ":" type "," } "}" ;
const_decl  = "const" ident ":" type "=" expr ;

type        = "i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64"|"usize"|"isize"|"bool"
            | "*" [ "mut" ] type
            | "[" type ";" int_lit "]"
            | fn_type                              (* round 58 *)
            | ident ;
fn_type     = "fn" "(" [ type { "," type } ] ")" [ "->" type ] ;

block       = "{" { stmt } "}" ;
stmt        = let_stmt | var_stmt | assign | if_stmt | while_stmt
            | for_stmt | jump_stmt                 (* round 2 *)
            | return_stmt | expr_stmt | block ;
for_stmt    = "for" ident "in" expr ".." expr block ;      (* round 2 *)
jump_stmt   = "break" | "continue" ;                       (* round 2 *)
let_stmt    = "let" ident [ ":" type ] "=" expr ;
var_stmt    = "var" ident [ ":" type ] "=" expr ;
assign      = lvalue "=" expr ;
lvalue      = ident | lvalue "." ident | lvalue "[" expr "]" | "*" lvalue ;
if_stmt     = "if" expr block [ "else" ( block | if_stmt ) ] ;
while_stmt  = "while" expr block ;
return_stmt = "return" [ expr ] ;

expr        = or_expr ;
or_expr     = and_expr { "||" and_expr } ;
and_expr    = cmp_expr { "&&" cmp_expr } ;
cmp_expr    = add_expr [ ( "=="|"!="|"<"|"<="|">"|">=" ) add_expr ] ;
add_expr    = mul_expr { ( "+"|"-"|"|"|"^" ) mul_expr } ;
mul_expr    = unary   { ( "*"|"/"|"%"|"&"|"<<"|">>" ) unary } ;
unary       = ( "-" | "!" | "~" | "&" | "*" ) unary | postfix ;  (* ~ round 68 *)
postfix     = primary { "." ident | "[" expr "]" | "(" [ args ] ")" | "as" type } ;
primary     = int_lit | bool_lit | qualified | "(" expr ")" | struct_lit
            | array_lit | "syscall" "(" args ")"
            | closure ;                            (* round 58 *)
closure     = [ "gc" ] "fn" "(" [ params ] ")" [ "->" type ] block ;
qualified   = ident [ "." ident ]                  (* modul.name, round 2 *) ;
struct_lit  = ident "{" { ident ":" expr "," } "}" ;
array_lit   = "[" [ expr { "," expr } ] "]"
            | "[" expr ";" expr "]" ;              (* repetition, round 2 *)
```

**The line end closes a statement -- unless the next line continues the
expression (round 68).** A semicolon is optional; outside brackets a line
break ends the statement. From round 68 on there is one exception, and it is
defined by a fixed list of tokens rather than by a guess: an expression is
CONTINUED when the first token of the following line can only ever stand
BETWEEN two operands, that is one of

```text
+  -  /  %  &  |  ^  <<  >>  &&  ||  ==  !=  <  <=  >  >=  .  as
```

so a long condition or a long mask may be broken with the operator at the
start of the following line, and a chain of field accesses may be broken at
the `.`. An operator at the END of a line has continued the expression since
round 2 and goes on doing so.

**`*`, `(` and `[` are deliberately NOT in that list**, and that is the
whole point of it: a line may legitimately begin with `*p = 0`, with
`(*p).f = 0` or with an index, and none of those may silently become a
multiplication, a call or an index belonging to the line before. Inside
brackets a line break has never ended anything and still does not. Proof:
`tests/1232_line_continuation.fi` and
`tests/neg/1233_star_is_no_continuation.fi`.

---

## 13. Numbers, layout, ABI

* Integer types with the width written out: `i8...i64`, `u8...u64`,
  `u128`/`i128` (9.3), `usize`, `isize`.
* **The second spelling (round 70).** Since round 70 the same types have a
  second, C#-flavoured name. It is an **alias, not a new type**: `int` and
  `i32` are THE SAME type and pass into each other without a cast, and
  `impl Ord for int` creates the very same method as `impl Ord for i32`.

  | second spelling | canonical | | second spelling | canonical |
  |---|---|---|---|---|
  | `sbyte` | `i8` | | `byte` | `u8` |
  | `short` | `i16` | | `ushort` | `u16` |
  | `int` | `i32` | | `uint` | `u32` |
  | `long` | `i64` | | `ulong` | `u64` |
  | `double` | `f64` | | `float` | `f32` |
  | `string` | `str` | | | |

  The canonical form inside this repository stays `i32`/`i64`/`u8`; error
  messages name it too.

  Two promises belong to that, and they are promises on purpose:

  * **`int` is ALWAYS 32 bits, `long` ALWAYS 64 -- on every platform.** In
    C/C++ `long` is 32 bits on Windows and 64 on Linux; the same source text
    computes differently depending on where it is translated, and that has
    cost decades of portability bugs. That is exactly the reason why Firn
    writes `i32`/`i64` in the first place. The second spelling inherits the
    fixed width; it does not inherit the ambiguity.
  * **`byte` is UNSIGNED (0..255), `sbyte` is `i8`.** A byte is a storage
    unit -- an octet --, not a number one calculates with; the natural range
    of an octet is 0..255. C#, Go, Rust and Zig all see it that way. Java's
    signed `byte` is the outlier and it did not come out of a decision but out
    of a lack: Java has no unsigned types at all. The stock of this project
    says the same thing: `u8` occurs 6992 times, `i8` 13 times.

  **ROUND 71: `float` is given out and it means `f32`** -- as in C, C++, C#,
  Java and Go. It was held back in round 70 on purpose, so that it would not
  first mean `f64` and then something else.

  **ROUND 88: `string` closes the family.** The list is the one of C#, and
  there the text type is called `string`; it was the only name missing, so
  the most obvious line a stranger writes, `let x: string = "test"`, was
  answered with "unknown type 'string'" -- for no reason anybody could
  name. `string` and `str` (8.0) are ONE type: the same layout, the same
  methods, they pass into each other without a cast, `impl ... for string`
  creates the same method as `impl ... for str`, and a type error names
  `str` in both spellings. **The canonical form stays `str`**, and the
  repository writes `str` throughout -- as it writes `i32` and not `int`.

  One difference to all the pairs above, and it matters for whoever reads
  the compiler: `str` is not a primitive type but the builtin STRUCT
  (`compiler/src/strtype.rs`). That is why `string` may not go into the
  tables that map onto primitive KINDS (`types.fi::alias_ty`); the name is
  folded one step later, right before the struct is looked up
  (`sema.rs::resolve_ty`, `types.fi::canon_str`).
* **Literals are typeless until they are used.** Where the context says
  something, that holds (`let y: i64 = 5`, `let y: float = 2.5`). Where
  nothing at all says anything, `i32` holds for integers and `f64` for
  floating point (the suffix `1.5f` forces an `f32` there) -- the default type since round 70, as in C#, Java and
  Go. The overflow check is unaffected: `let x = 5000000000` is an error,
  because 5000000000 does not fit into an `i32`.
* **Overflow semantics stated explicitly (`L9`):** `+ - *`, `/ %` and a
  narrowing `as` check in `dev`/`dev-fast`/`release-safe` and wrap in
  `--release-fast` (defined, **not** undefined -- deliberately different
  from C). **Round 72** is what makes this table true rather than aspirational
  -- until then `release-safe` ran every optimization pass and left the
  arithmetic underneath exactly as unchecked as `release-fast`, and the CLI
  default (no `--opt-level=` at all) silently built `release-fast`, not
  `dev-fast` as documented, so `firnc -o x file.fi` never checked anything
  either:

  | level | checks `+ - * / % as` |
  |---|---|
  | `dev` (`--no-opt`) | yes |
  | `dev-fast` (**the CLI default**) | yes |
  | `release-safe` | yes |
  | `release-fast` | no -- wraps, silently and by definition |

  In addition there are explicit operators that are NEVER checked, in any
  level, on purpose: `+% -% *%` (wrapping) and `+| -| *|` (saturating) --
  written where the wrap-around itself is the point (a hash function's own
  multiplication, a checksum, a millisecond counter that is allowed to
  roll over), so a program does not have to switch off checking everywhere
  else just to get that one line. `profile kernel` (2) has no runtime of
  its own to fall back on: a checked operation that goes out of range
  there calls an EXTERNAL symbol `osum_panic` that the kernel author
  must define -- an undefined reference at the final link (not at
  compile time, since the object file is freestanding on its own) is the
  honest outcome for a kernel that never does; `demos/kernel/start.s`
  shows a minimal one (write the message to COM1, halt). HTML and CSS
  parsing have edge cases that the specification prescribes exactly; you
  need both of them without a detour for that.

  **Both compilers do this**, and that is not decoration: `firnc1`
  (`lib/firnc1/*.fi`, the compiler written in Firn itself, `L1`/section 11)
  reads the same six operators, makes the same checked-versus-wrapped
  decision and prints the same message, down to the file, the line and the
  column. It has to -- `tools/fir_compare.sh` compares the two intermediate
  representations as TEXT, and the message is part of it. `docs/ROUND72.md`
  has the numbers.
* **Floating point (`L10`):** `f32`/`f64` following IEEE 754 with exact
  semantics, including the treatment of NaN. JS numbers *are* doubles, `calc()`
  computes in doubles; deviations show up as a wrong layout. No "fast maths", no
  reordering of floating point operations by the optimizer -- with IEEE 754 that
  is not value-neutral.
* Conversions exclusively with `as`, lossless ones included.
* **Struct layout (`P7`):** declaration order with natural alignment, **no**
  reordering. `#[packed]`, `#[align(n)]`.
* **The calling convention:** System V AMD64 (`rdi, rsi, rdx, rcx, r8, r9`, the
  return value in `rax`, 16 byte alignment, `rbx, rbp, r12-r15` preserved).
  `extern "C"` is the same. In addition (`L13`): a documented, stable
  **Firn-to-Firn ABI** across component boundaries, because the browser consists
  of separate Osum components that talk to each other.
* `syscall(nr, a1, ..., a6)` is built in and maps directly onto `syscall`
  (`rax, rdi, rsi, rdx, r10, r8, r9`).

### 13.1 The result location -- a guarantee, not an optimization

With `let x: T = expression`, `return expression` and a field assignment the
producing expression knows the **destination address** and writes there
directly. **No** intermediate value comes into being on the stack that is copied
afterwards.

That counts as a **language guarantee**, not as an achievement of the optimizer
-- so it holds in `--dev` and `--dev-fast` as well. The reason is practical:
`let b: [u8; 8<<20] = ...` must not claim 8 MB of stack first. That is exactly
where Rust fails (`Box::new([0u8; 8*1024*1024])` runs over the stack), which is
why Rust-for-Linux had to rebuild the library `pin-init`.

**The state of the implementation (2026-08-14):** for **aggregate returns** it
is already like that -- `abi::ret_needs_sret()` classifies returns above 16
bytes as `MEMORY` with a hidden pointer in `rdi`, and lowering passes the
destination address through (`compiler/src/lower.rs:604`). **Still open:** the
same guarantee for struct and array literals as well as for the planned `init`
expression. In detail in `DESIGN_GOALS.md` 6.

---

## 14. What `firnc0` (stage 0) implements -- binding

The contract between the specification and the code. Everything here really
runs; everything else in this document is future work and is carried in the
README as "not yet".

**Contained:**
* A lexer and a hand-written recursive descent parser, error messages with file,
  line, column, source line and a marker; several errors per run.
* A type checker: `i8...i64`, `u8...u64`, `usize`, `isize`, `bool`, `*T`/`*mut T`,
  structs with field access, arrays of fixed size with an index, functions. No
  implicit conversion.
* Expressions: `+ - * / %`, `& | ^ << >>`, comparisons, `&& ||`
  (short-circuiting), unary `-`, `!`, `~` (round 68), `&`, `*`.
  `!` is the LOGICAL negation of a `bool`, `~` the bitwise complement of an
  integer; neither stands in for the other, in either direction
  (`tests/1230_bitnot.fi`, `tests/neg/1064_tilde_needs_integer.fi`,
  `tests/neg/1231_not_needs_bool.fi`).
* Statements: `let`, `var`, assignment to a variable/field/index/dereference,
  `if`/`else`, `while`, `return`, blocks. Functions with parameters, recursion.
* `syscall(...)` with up to 6 arguments.
* **FIR** in basic blocks, documented, with text output (`--emit=fir`), constant
  folding and dead code elimination, with a before/after test.
* **x86_64 codegen without LLVM**: assembly output for `as`/`ld`, the System V
  ABI, a register assignment of its own -- *named honestly:* that is **not**
  register allocation in the sense of liveness analysis or graph colouring.
  Stage 0 gives every FIR value a stack slot of its own and computes in
  `rax`/`rcx`/`rdx` (pure spilling). Correct, but slow. Real register allocation
  is `P3`.
* A test suite with >= 40 `.fi` programs plus negative tests.

**Not contained (stage 0), state after round 3:** `interface`, `drop`, the move
checker, the reference types `&T`/`inout T` as checked types (raw pointers
only), arenas, unwinding/`throw`, `u128`, `Arc[T]`, the standard library,
concurrency, package management, aarch64, WASM, an LLVM backend. Type
constructors that are not implemented report an error of their own with
line/column instead of a syntax error (`Rc[T]`, `Weak[T]`, `Arc[T]`; proof:
`tests/neg/int_gc_not_implemented.fi`).

**Round 3 struck the following from this list:** error unions `E!T` with
`try`/`catch` (14.1.error_unions), `secret`/the constant-time primitives (9,
`compiler/src/ct.rs`) as well as the **entire opt-in tracing GC**: `gc class`
with single inheritance, `Gc[T]`, `GcWeak[T]`, fallible allocation
`AllocError!Gc[T]`, `#[no_gc]` and `Rc`/`Weak` as a pure Firn module (14.1.gc).

**Round 2 struck the following from this list** (each one documented
individually in 14.1): modules/imports and `export` (item 15), generics by
monomorphization (14.1.types), `enum`/`match` with an exhaustiveness check
(14.1.types), the string types `Bytes`/`Str`/`Str16`/`Atom` as a library in Firn
(14.1.str), `.debug_line` for `gdb` (item 16) as well as optimization far beyond
constant folding and DCE: mem2reg, CSE, inlining, block merging and **real
register allocation** (14.1.opt). The sentence named above under "x86_64
codegen", "pure spilling, no register allocation", has applied only to
`--no-opt` since round 2.

### 14.2 Attributes

The specification relies on attributes in many places (`#[must_consume]`,
`#[no_gc]`, `#[constant_time]`, `#[unwinds]`, `#[packed]`, `#[align(n)]`,
`#[layout(soa)]`, `#[no_move]`, `#[abi_stable]`, `#[frozen]`, `#[hot]`). They
come into being at very different times. So that this does not turn into a
thicket, the following applies:

* **One registry.** `compiler/src/attrs.rs` is the only truth about which
  attributes exist, where they belong, how many arguments they take and whether
  stage 0 implements them. `firnc --list-attrs` prints the registry.
* **Never ignore one silently.** A known but unimplemented attribute is a
  **compile error** with line, column and a note about its intended purpose. An
  overlooked `#[constant_time]` would be the most dangerous bug this language
  can have (9.2).
* **Unknown attributes** deliver a suggestion if it is a typo.
* **The wrong target** (for example `#[packed]` in front of a function) is an
  error.

**`#[allow_escape]`** (round 79, 3.6.1) belongs to the registry as well: in
front of `fn`, no arguments, implemented in both compilers. It switches the
escape analysis off for that function's body and empties its summary, so a
vouched-for function does not send its callers red instead.

**`#[arch(x86_64)]` / `#[arch(aarch64)]`** (round ARM-FREESTANDING): in front
of `fn`, exactly one argument, implemented in both compilers. It says which
MACHINE a definition belongs to; every definition for another machine is
thrown away before the type checker runs, so several definitions of one name
may stand in the same source as long as at most one of them survives. The
argument is one of the words the `--target` names are built from; an unknown
one is an error and not a silent removal, and a name whose every definition
belongs to another machine is reported at the definition rather than at the
call site.

It exists because of the inline assembler (2, 14.5): an assembler template is
not a Firn expression that has not been ported, it is a line for a particular
assembler, and a language that offers one owes its user a way to say which
machine a piece of source belongs to. It is deliberately NOT conditional
compilation in general -- no `#[arch]` on a type or a constant, no negation,
no nesting.

Exactly one is implemented in stage 0: **`#[must_consume]`**, in front of `fn`
and in front of `struct`. What is checked is the subset decidable without a move
checker -- *the result of a call must not be discarded as a statement*. The full
form from 3.3 (*the value has to be passed to a consuming function*) comes with
the move checker in ROADMAP phase 2. This restriction is documented in the
compiler and named explicitly here, so that `#[must_consume]` does not promise
more than it delivers.

### 14.1 Addendum: deliberate deviations of the stage 0 implementation (`firnc0`)

Records where the implementation is narrower than the text above -- so that the
specification and the code do not drift apart.

1. ~~**Aggregates across function boundaries.**~~ **Struck in round 2** (module
   `kern`): structs and arrays are allowed as parameters and as return values.
   The System V classification is in `compiler/src/abi.rs`
   (`ArgClass::{Integer, Memory}`, `classify`) and is the only truth about the
   calling convention. The class `Sse` is deliberately missing there as long as
   there are no floating point types (see item 20). Proof:
   `tests/100_agg_param_8.fi` to `tests/105_agg_value_semantics.fi`.
   **Two deliberate deviations from System V remain** and are recorded here (see
   item 15 on top of that):
   * Aggregates of the MEMORY class (> 16 bytes) are passed as a *hidden pointer
     to a copy made by the caller* instead of as a stack copy.
   * Returns of aggregates already go through the hidden pointer in `rdi`
     **from 9 bytes on** (System V uses `rax:rdx` for 9-16 bytes); up to 8 bytes
     `rax` delivers the word.
   Both are self-consistent (the caller and the callee follow the same rule),
   but **not** binary compatible with C for these cases.
2. **Typeless literals.** There is **no** default type. `let x = 5` is an error,
   `let x: i32 = 5` and `let x = 5 as i32` are right. Context is supplied by: a
   type annotation, the target type of an assignment, a parameter type, a return
   type, `as`, the other (typed) operand of a binary operator, the index
   position (`usize`).
3. **No run-time checks.** Overflow, division by zero and going out of range are
   not checked in stage 0 (13 describes the target state). The behaviour
   corresponds to `--release-fast`.
4. **`const`** is restricted to scalar integer and `bool` expressions evaluable
   at compile time.
5. **Global variables** do not exist (only `const`). Looked at again in
   round 68 and deliberately left standing: a `static` needs a data section
   with an initialisation order, a rule for the collector (is a `static
   Gc[T]` a root?) and one for threads. That is a round of its own, not a
   side effect of one. What a kernel does instead is in `docs/ROUND59.md`
   section 2, what the JavaScript engine does instead in `docs/ROUND63.md`
   gap 5.
6. ~~**The `profile` declaration** is parsed and checked, but has no effect.~~
   **Struck in round 52** (`compiler/src/prof.rs`, `compiler/src/core.rs`,
   `docs/ROUND52.md`): `--profile=kernel` and `profile kernel` respectively
   enforce the table from 2 -- no `import std.*`, no `gc class`, no `syscall`,
   no `#[unwinds]`, floating point only with `#[allow_fp]` -- and produce a
   freestanding **ELF object file** (`-c`, no `ld`, no `_start`, no contact with
   libc). Along with that came inline assembly
   (`asm("...", in("dx") p, out("rax"), clobber("memory"))`), MMIO
   (`__mmio_read/schreiben8|16|32|64`) and interrupt entry points
   (`#[interrupt]`, which saves 14 registers and closes with `iretq`). Proof:
   `demos/kernel/core.fi` boots in QEMU, with **both** compilers
   (`tools/freestanding/run.sh`). In the app profile everything stays as before:
   a freestanding binary with `_start` and without libc.
   **Round 73** made the import rule precise (2.1): a `std` module that
   declares `profile kernel` itself is admitted, the rest stays forbidden.
   And the `Allocator` that this section has promised since round 52 now
   exists -- an interface with `alloc`/`free`/`grow`, an alignment and a
   failure case that is a VALUE (`Block { p, n, ok }`), with two
   implementations: `core.Arena` (a bump allocator over foreign memory, no
   system call) and `mem.PageAllocator` (`mmap`). It is passed as an
   ordinary parameter -- `str.join(alloc, parts, ", ")` against
   `span.find(part)` -- so that at the call site you can see what costs
   memory. Proof: `tests/1402`, `tests/1403`, `demos/kernel/kcore.fi` and
   the soak run in `tools/core/run.sh` (240 000 requests, exactly ONE
   system call, RSS flat, with a leaking counter-check).
7. ~~**`extern fn`** is recognized syntactically, but rejected with a clear
   error.~~ **Struck in round 75** (SPEC 14.5, `compiler/src/extfn.rs` and,
   for stage 1, `lib/firnc1/parser.fi`/`codegen.fi`): a declaration WITHOUT a
   body, closed with `;` instead of `{ }` -- `extern fn strlen(p: u64) -> u64;`.
   The call goes out under the BARE, unmangled name (no `_F0.`/`_F1.` prefix),
   using the System V AMD64 classification round 71 already built
   (`abi.rs`/`types.fi::word_class`). `#[link_name(c_symbol)]` overrides the
   symbol when the Firn name and the foreign one differ; without it the bare
   Firn name is used. The reverse direction, `#[export_c]` in front of an
   ordinary `fn` with a body, emits that function under its bare name too, so
   C can call it back -- proof in both directions: `tools/extfn/run.sh` links
   the produced object file against a hand-written `strlen` in assembly (Firn
   calling out) and against a small C driver that calls a `#[export_c]`
   function back (C calling in), in BOTH compilers, with the exit code
   checked. Deliberately out of scope: variadic externs (`printf`-style),
   struct arguments across the boundary beyond what item 1 above already
   covers, and any form of dynamic loading -- `extern fn` here means a NAME
   the linker resolves at link time, nothing more.
8. **The return value of the program.** `fn main() -> i32`; `_start` calls
   `main` and passes the result to `exit` (exit code = value & 0xFF).
9. ~~**At most 6 function parameters.**~~ **Struck in round 2** (module `kern`):
   arguments from the seventh INTEGER word on are placed at `[rsp+8k]` before
   the `call`, the 16 byte alignment is preserved; the callee reads them at
   `[rbp+16+8k]`. Proof: `tests/108_stack_args.fi` and the codegen test
   `stapelargumente_ab_dem_siebten_wort`.
10. **Parameters are immutable** (like `let` bindings).
11. ~~**No repetition literal `[value; N]`.**~~ **Struck in round 2**
    (module `kern`): `[value; N]` exists, `N` is a constant expression. The
    value is evaluated exactly once; up to 8 elements lowering unrolls it,
    beyond that a loop comes into being. Proof:
    `tests/109_repeat_literal.fi`.
12. **`as` binds more tightly than the unary operators.** `&s.a as u64` means
    `&(s.a as u64)`; what is meant is `(&s.a) as u64`.
13. ~~**No `break`/`continue`.**~~ **Struck in round 2** (module `kern`):
    `break`, `continue` and `for i in a..b` exist; the desugaring takes place
    exclusively in lowering (`continue` in a `for` loop jumps to the increment
    block, not to the head). Outside a loop `break`/`continue` is an error with
    line/column. Proof: `tests/106_for_loop.fi`, `tests/107_break_continue.fi`,
    `tests/neg/core_break_outside.fi`.
14. **The assembly output** is Intel syntax with `.intel_syntax noprefix`. `as`
    and `ld` are called exclusively as assembler and linker, never a C compiler.
15. **The module system (round 2, module `kern`).** `import path.module`,
    `export { a, b }` and `module.name` exist; several `.fi` files are compiled
    into **one** binary. What is implemented is *whole-program compilation with
    separate namespaces* (names of non-root modules are called `module__name`
    internally), **not** separate object files with interface files.
    **Round 48 -- the project system.** Added to that is the manifest
    `firn.paket` (`paket`, `version`, `start`, `quelle`, `oeffentlich`,
    `brauche`), a fixed search order (the importing file -> the root file ->
    the project sources -> `brauche` dependencies -> `$FIRNLIB` ->
    `<exe>/../lib`), **visibility at module level** through the `oeffentlich`
    list, detection of package cycles and of module name clashes, and the build
    driver `firnc --paket <dir>`. Without a manifest **nothing** changes. It
    stays with whole-program compilation: no separate object files, no network,
    no lock file, no version resolution -- `W1` and ACCEPTANCE item 5 therefore
    stay open (`docs/ROUND48.md`).
16. **Line numbers for the debugger (round 2, module `kern`).** The compiler
    writes `.file`/`.loc` directives; `as` produces `.debug_line` from them.
    Instruction-accurate lines exist **only without the optimizer**
    (`--no-opt`), because the FIR carries no source positions and the optimizer
    removes instructions and renumbers blocks. With the optimizer the line of
    the `fn` declaration remains. `gdb` does not show variables yet (no
    `.debug_info` for local names).

17. **Enumeration names are program-wide, not per module (round 3).** An `enum`
    is carried in the registry of `sema_match` under its bare name. An `enum` in
    an imported module therefore has to be addressed as `Ampel::Rot`, **not** as
    `zustand.Ampel::Rot`; two modules may not declare an enumeration of the same
    name. Proof: `tests/231_module_match.fi`. (Round 3 fixed a real bug at the
    same place: the arm bodies of the `match` cases live in the registry and
    were not rewritten by the module system -- `match` in an imported module was
    unusable. `compiler/src/modules.rs` now visits them through
    `sema_match::take_match`/`put_match`.)

18. **Error messages in imported modules (round 3).** Line and column are
    correct, but the **file name** shown is that of the root file. The source
    map does carry file numbers, but the diagnostic does not yet select the
    right file from them. Open.

19. **The constant-time primitives: only the three building blocks, no
    `secret[T]` (round 4).** Implemented from 9 are: `select(condition, a, b)`
    (becomes `cmov`, never a jump), `barrier(x)` (an opaque barrier) and
    `secure_zero(pointer, number_of_bytes)` (survives every pass, `rep stosb`).
    They are in `compiler/src/ct.rs` and apply on scalar types (integer, `bool`,
    pointer). **Not** implemented are the type qualifier `secret[T]`, the
    propagation of the marking through expressions, `declassify` and with them
    the effect of `#[constant_time]`: the attribute stays carried in `attrs.rs`
    as *not implemented* and reports a clean error
    (`tests/neg/attr_not_implemented.fi`). The check in the code generator (a
    conditional jump on a `secret` value aborts) is present, but it only gets
    something to work on with `secret[T]`. A deviation in notation: 9 writes
    `barrier(inout x)` and `secure_zero(inout buf)`; stage 0 has no `inout`, so
    `barrier` takes the value and returns it, and `secure_zero` takes a pointer
    and a byte count. A function of the same name defined by the user hides the
    primitive. Proof: `tests/430_ct_select.fi` to `tests/433_ct_secure_zero.fi`,
    five negative tests `tests/neg/ct_*.fi` and four codegen proofs in
    `compiler/src/ct.rs`.

20. ~~**No floating point types (confirmed in round 4).**~~ **Both arrived:**
    `f64` in round 11 (14.1.f64), `f32` in round 71 (14.1.f32). And the
    sentence about `abi.rs` came true exactly as it was written down: the SSE
    class was added together with `f32`, and the exhaustiveness check of the
    compiler then named every case distinction that had to handle it.

#### 14.1.types -- sum types, pattern matching, generics (round 2, module `types`)

With round 2 `firnc0` implements 6.3 (`L4`) and generics (`L5`): `enum` with
payload, `match` with an **exhaustiveness check at compile time** (a missing
case = an error with line/column and the name of the variant), jump tables in
the code generator and monomorphization. Deliberately narrower than the text
above is the following:

T1. **`match` is a statement, not an expression.** `let x = match e { .. }` is
    not supported; every case has a block as its body. The reason: the result
    value of a pattern match demands a merging of values (phi) in lowering,
    which stage 0 does not have. Assignment in the body is the substitute.
T2. **An enumeration may not lie by value inside a `struct`.**
    `struct S { a: E }` reports an error with a note about `*mut E`. The reason:
    the struct layout is fixed before the enumerations are laid out.
    An enumeration inside an enumeration is allowed, on the other hand (nested
    patterns).
T3. **No generic enumerations** (`enum Option[T]`). Only functions and structs
    are generic.
T4. **No `match` inside the body of a generic template.** The arm bodies lie
    outside the AST and would not be substituted per instantiation; the compiler
    reports that as an error instead of quietly producing wrong code.
T5. **No alternative patterns** (`A | B`) and no guards (`if`) in a pattern.
T6. **Enumerations and generic templates are file-local.** `modul.E::V` is not
    resolved; an enumeration is used in the file in which it stands (several
    files in one compilation are allowed, as long as the enumeration is not
    addressed across the file boundary).
T7. **Requirements on type parameters** are restricted to `Any`, `Int` and
    `Scalar` (no interface system, 6.2 stays open).
T8. **Range patterns** apply only to integers and are restricted to integer
    bounds (`1..4`, `4..=9`, `-5..=-1`).

The memory layout of an enumeration (binding, the basis for `tok` and for
debugging): `__tag: u32` at offset 0, after it the payload from
`round_up(4, payload_align)` on; the payload areas of the different variants
**overlap**, the size and the alignment follow from the largest variant (4 at
least). The naming scheme of monomorphization: `name__T1_T2`.

#### 14.1.gc -- the opt-in tracing GC, `gc class`, the DOM prototype (round 3)

Implemented and demonstrated with running code:

* **`gc class Name [extends Base] { ... }`** with a prefix layout -- the
  inherited fields lie at the front, which is why the upcast `Gc[Element]` ->
  `Gc[Node]` is free. Downwards only checked: `x.as?[Element]` returns the null
  value if the type does not fit.
* **`Gc[T]`** as a first-class strong pointer (field access without
  `(*p).field`), **`GcWeak[T]`** as a weak reference with `weak(g)`/`stark(w)`,
  the null values `gc_null[T]()`/`weak_null[T]()`.
* **Allocation is fallible**: `gc C{...}` has the type `AllocError!Gc[C]`
  (DESIGN_GOALS 2). With an exhausted heap it **first collects, then fails** --
  demonstrated in `tests/535_gc_fallible_allocation.fi` with an upper limit of
  256 KiB.
* **Mark-sweep**, stop-the-world, collection only at allocation sites and at
  `gc_collect()`. **Precise** heap tracing through a compiler-generated type
  table (the field offsets per class, separated into strong and weak), a
  **conservative** scan of the stack **and** the registers. No compaction, a
  size-class allocator, `mmap` without a fixed address.
* **An insertion barrier** when writing a `Gc` pointer into a field
  (`gc_barriers()` counts them).
* **`#[no_gc]`** checked transitively: forbidden are GC allocation, calling an
  unmarked function and writing into a `Gc`/`GcWeak` field. The HTML5 tokenizer
  in `lib/html/` is marked that way.
* **Measurements at run time**: `gc_collections`, `gc_live_objects`,
  `gc_live_bytes`, `gc_heap_bytes`, `gc_pause_ns_last/max/total`,
  `gc_barriers`, `gc_set_max_bytes`.
* **`Rc[T]`/`Weak[T]`** as a pure Firn module (`tests/modules/rc.fi`), always
  immutable, fallible allocation, `#[must_consume]`. Cycles leak there
  **deliberately** and are made visible (`tests/552_rc_cycle_leak.fi`), instead
  of being explained away.

**Demonstrated on the DOM** (`lib/dom/dom.fi`, `tests/560_dom_cycles.fi`,
`tools/dom_soak/run.sh`): six kinds of cycle -- parent<->child both strong,
`Element extends Node` with attributes, node<->listener, collection->root, an
observer through `GcWeak`, node<->JS wrapper. The soak test: **100,000,000
cycle sets = 700,000,000 objects at a constant 1,364 KiB RSS**; the
reference-counting counter-check with an identical graph needs **750,080 KiB**
after 2,000,000 cycles. The report: `docs/reports/dom.md`.

**Honest limits of this implementation:**

* **`GcVec`/`GcMap` have existed since round 53** (`lib/gc/gcvec.fi`,
  `lib/gc/gcmap.fi`, `docs/ROUND53.md`), **`virtual` has not.**
  *Incremental collection* came along in round 44 (the longest pause
  **0.45 ms** instead of 3.54 ms), *finalizers* (`S4`) in round 47.
* **Collections, the stage 0 form (round 53, `docs/ROUND53.md`):** the collector
  traces the heap precisely through a type table with **fixed** field offsets; a
  growing collection does not fit in there. It therefore lies in a second
  object, the **slot buffer** -- an ordinary GC block that carries the bit
  `F_SLOTS` in the status word of its header and brings the element count, the
  stride and the pointer mask along itself. It is traced in slices of 64
  elements and stays grey in between; without that slicing the longest pause
  with 120,000 elements rises from **0.50 ms to 2.67 ms** (measured). The price
  of stage 0, named instead of concealed:
  * `GcVec[Gc[Node]]` is **one** nominal type, not one per element type --
    stage 0 has no generic `gc class`. The element type is checked at the
    **access** (`gcvec_anhaengen[Node](...)`), not at the field.
  * Instead of `parent.children.push(child)` it says
    `gcvec_anhaengen[Node](eltern.kinder, kind)` -- stage 0 has no methods, the
    same decision as with the finalizers.
  * In `GcMap` the keys **0 and 1** are reserved (an empty slot and a
    tombstone).
* **Finalizers, the stage 0 form (round 47, `docs/ROUND47.md`):** stage 0 had no
  methods and no function pointers at the time, so `fn finalize(inout self)` has become a
  pair -- `gc_finalisierer_setzen(p, art)` enters a **cleanup kind** into the
  block header, and the program declares **one** dispatcher
  `fn __gc_finalisiere(art: u64, p: *mut u8)` in its root file. The promises
  from 3.5.3 apply unchanged and are enforced at **run time** instead of merely
  checked: before the call all the `Gc[T]`/`GcWeak[T]` fields of the object are
  zeroed, `stark()` on a waiting object returns 0, and allocation (abort 71),
  `gc_collect()` (72) as well as writing a Gc pointer into a heap field (73)
  abort visibly. **There is therefore no resurrection** -- the block is released
  as soon as the finalizer returns. The order between two objects: **no
  promise**. At most once per object.
* **External root ranges (round 47):** `gc_wurzel_anmelden(p, bytes)` registers
  memory **outside** the GC heap that may contain `Gc[T]` (needed by `Arc[T]`,
  `lib/rc/arc.fi`). It is scanned conservatively at the start of every cycle;
  the price is a start pause that grows with the registered size.
* **One thread.** The state block is meant to be thread-local; stage 0 has only
  one.
* **The conservative scan has a price that can be measured:** an old pointer
  copy in a **live** stack frame keeps its object alive. Whoever checks a
  collection in the same body in which the object was created therefore does not
  measure what they think they are measuring. The runtime overwrites the dead
  stack area below its own frame (`__gc_scrub_tief`); for the live frame there
  is no remedy except precise stack maps.
* **`Gc[modul.Klasse]` cannot be written** -- a `gc class` from another module
  cannot be named in a type entry. Root programs therefore pass only numbers
  across the module boundary (see `lib/dom/soak_gc.fi`).
* **Fragmentation** with changing object sizes is unchecked; the soak test
  always uses the same set.

#### 14.1.module -- two limits of the module system removed (round 17)

**Import paths are searched relative to the importing file first**, and only
after that relative to the root file. Before, only the second rule applied, and
so one library could not include another. The fallback to the root remains, so
that existing source text still compiles unchanged.

**Generic templates from modules are usable.** The pre-scan for templates ran
per file immediately before that file was parsed; the root file is parsed first
and therefore did not know the templates of the modules.
`modules::build_program` now lexes all the files first and scans them in
advance.

Proof for both: `tests/630_modulkette.fi`.

**Generic templates see the names of their own module file** (round 18). They do
not lie in `Program::funcs` but in `sema_generic::REG`; the module rewriting
therefore never reached them. `modules::build_program` now sends the
templates of the respective file through the same `Renamer` -- their **name**
stays untouched, because the instantiation looks it up under the original name
and generic names apply program-wide.

With that a generic collection can be written as a library:
`lib/rt/vec.fi` includes `rt`, calls `rt.heap_alloc` from the body of a
template, and the root file writes `var v: Vec[i32] = vec_neu[i32]()`
(`tests/640_vec_module.fi`).

#### 14.1.sizeof -- `size_of[T]()` (round 16)

`size_of[T]()` returns the size of a type in **bytes**, determined at compile
time. Nothing of it remains at run time: the type checker computes the size from
the layout (`TypeCtx::size_of`), lowering inserts a constant.

```firn
let n: usize = size_of[i32]()       // 4
let m: usize = size_of[Punkt]()     // the computed struct layout, not the sum of the fields
var feld: [u8; 16] = [0 as u8; 16]  // serves as an array length
```

Built like `gc_null[C]()`: the parser recognizes the form and packs it as a call
with a reserved name (`size_of$...`) that contains the type name. `size_of` is
therefore **not a keyword** and collides with no identifier.

**Inside generic templates** the type parameter in the name is substituted as
well (`mono::subst_call_name`) -- without that the type checker reports *unknown
type 'T'* as soon as the template is instantiated.

**Limits:** only a **type name** as an argument, no composite type expression
(`size_of[*mut u8]` does not work -- whoever needs that gives the type a name).
`size_of[void]` is an error.

**What for:** without the element size the address of the `i`-th element cannot
be computed, and so there is no growing `Vec[T]`
(`docs/SELF_HOSTING.md` 4, item 2).

#### 14.1.comptime -- evaluation at compile time (round 12)

`compiler/src/comptime.rs` **runs your own functions at compile time** -- with
loops, branches, local variables and recursion. The entry point is every place
where a constant expression is expected:

```firn
fn fakultaet(n: i64) -> i64 { ... }

const FAK10: i64 = fakultaet(10)      // 3628800, at compile time
var feld: [u8; 120] = ...             // results serve as an array length
```

**The scope:** integers and `bool`; `let`/`var`, assignment to local variables,
`if`/`else`, `while`, `for`, `break`, `continue`, `return`, blocks; all
operators with short-circuiting for `&&`/`||`, conversions with correct
truncation, calls (recursive ones included).

**Limits that are enforced:** at most 2,000,000 executed statements and 64
nested calls. Both end with a message including the source position -- a
`comptime` must not hang the compiler (`tests/neg/comptime_endless.fi`).

**Not possible:** pointers, arrays, structs, `syscall`, floating point, GC
allocation. All of those need memory at compile time; that comes with `emit`. An
attempt is reported, not compiled wrongly in silence
(`tests/neg/comptime_ptr.fi`).

**`emit` has existed since round 13.** A `comptime { ... }` block at the top
level builds **Firn source text** with `emit_roh("...")` and `emit_zahl(x)`,
which is lexed, parsed and appended to the program in the same run -- after
which the type checker sees no difference from hand-written code.

```firn
comptime {
    emit_roh("fn tab_gross(c: i64) -> i64 {\n")
    for c in 97..123 {
        emit_roh("    if c == ")
        emit_zahl(c)
        emit_roh(" { return ")
        emit_zahl(gross(c))
        emit_roh(" }\n")
    }
    emit_roh("    return c\n}\n")
}
```

`firnc --emit=comptime` prints the generated source text instead of building on
-- that way one can check what the compiler really has in front of it.

**How `emit_roh` gets by without strings in the interpreter:** the parser has
already turned `"abc"` into an array literal of octets (14.1.str); the
interpreter reads it back. So `comptime` needs no string support in order to
produce text.

**The order within a compilation run:** the blocks run **before** type checking,
straight after the modules have been merged. They may therefore use **no
program-wide constants**, but they may call any function of the program. The
generated text gets a file number of its own (`<comptime>`) in diagnostics *and*
in the line table -- if the latter is missing, the code generator produces
`.loc` directives with a number that `as` does not know.

**Data access at compile time (round 14).** `datei_groesse("path")` and
`datei_byte("path", i)` read a data file while the compiler runs. Byte by byte
-- so the interpreter needs neither strings nor arrays.
`tests/602_comptime_ucd.fi` reads a file in the format of `UnicodeData.txt`
(semicolon-separated fields, the code point in field 0, the upper case mapping
in field 12) and produces the lookup function `ucd_gross` from it.

**SECURITY -- and from the very beginning.** File access at compile time is a
way in for supply chain attacks: an included library could otherwise read
`/etc/passwd` during the build and write the contents into the generated code.
Hence the following applies:

* only **relative to the root source file**,
* **no `..`** at any position in the path,
* **no absolute path**.

Both are rejected, with a message and a source position
(`tests/neg/comptime_file_absolute.fi`, `comptime_file_parent.fi`). That is
deliberately narrower than necessary; once Firn gets the capability model from
`DESIGN_GOALS.md` 3, it turns into a permission that a module has to request
explicitly.

#### 14.1.f64 -- floating point (round 11)

`f64` has been a language type since round 11: literals (`1.5`, `1e3`,
`1_000.25`, `1.5e-1`), the basic operations `+ - * /`, all six comparisons, the
sign `-x` (**really only since round 68** -- the code generator could do it
from the start, the type checker refused it, see 14.1.round68 R3) and the
conversions `integer as f64` / `f64 as integer` (truncating
towards zero, as in C). Proof: `tests/590_f64.fi` with 29 checks in all three
build stages, among them NaN, infinity and negative zero.

**IEEE 754 is adhered to, in the awkward case as well.** `ucomisd` sets
`ZF=PF=CF=1` for NaN -- the unordered case therefore looks like "less than or
equal", and `nan < 1.0` returned **true** on the first attempt. Right is wrong.
That is solved not by recomputing from the parity flag but by **swapping the
operands**: `a < b` is emitted as `b > a` with `seta`, and `seta`/`setae` are
unordered-safe by themselves. Only `==` and `!=` additionally need
`setnp`/`setp`.

**Deliberately left out, with a justification:**

* ~~**No `f32`.**~~ **Arrived in round 71** -- see 14.1.f32 below. With it,
  floating point literals became typeless and `1.5` is only an `f64` where
  nothing says otherwise.
* **No `%`** (that would be `fmod` and needs a library function) and **no bit
  operations** on `f64` -- on a bit pattern they have no sensible meaning.
  Whoever needs them converts to `u64` explicitly. Negative test:
  `tests/neg/f64_no_modulo.fi`.
* **No implicit conversion**, not even between `i64` and `f64`
  (`tests/neg/f64_no_implicit_conversion.fi`).
* **No constant folding.** The value of an `Op::Const` with `FTy::F64` is a
  **bit pattern**; the folding in `opt.rs` computes with integers and would make
  silent nonsense out of `1.5 + 1.5`. It is therefore blocked for every
  instruction in which an `f64` is involved. Correctly rounded folding comes
  with `comptime`.

**Two honest restrictions of the implementation:**

F1. **No register allocation for floating point.** The linear scan in
    `regalloc.rs` knows only the integer registers; `f32`/`f64` live in the SSE
    registers and would need a second register class with intervals of their
    own. As long as that is missing, **every function in which a floating point
    value occurs goes through the baseline path** in `codegen_x86.rs` --
    correct, but without register allocation and therefore considerably slower.
    The computation happens in `xmm0`/`xmm1`, reading and writing through `rax`.

F2. ~~**An ABI of its own instead of System V.**~~ **Settled in round 71.**
    Floating point arguments travel in `xmm0`-`xmm7`, results in `xmm0`, and
    aggregates are classified eightbyte by eightbyte as the ABI document
    prescribes. Measured against GCC in both directions (`tools/abi/run.sh`).
    What is left of the deviation concerns aggregates only and has nothing to
    do with floating point: arguments over 16 bytes travel as a hidden pointer
    to a copy owned by the caller, and returns over 8 bytes always through the
    hidden pointer in `rdi` (see the head of `compiler/src/abi.rs`).

#### 14.1.f32 -- the second floating point width (round 71)

`f32` is a language type: IEEE-754 binary32, four bytes, four of them in one
128-bit SSE register instead of two. The reason is not tidiness. Without it
you cannot even READ a WAV, an OBJ, a glTF, an STL, a GPU buffer or most
network protocols -- 32-bit floats stand in all of them, and "you need
nothing but Firn" fails at the first file.

**The second spelling is `float`.** Round 70 handed out `int`, `long`, `byte`
and `double` and deliberately held `float` back, so that it would not first
mean `f64` and then something else (8.2).

**Literals are typeless now.** Where the context says `f32`, the literal is an
`f32`; where nothing says anything, `f64` holds -- the default type, as in
C#. The suffix `1.5f` exists for exactly the place where there is no context
(`let z = 2.5f`). `2f` works as well, and so do `1e3f` and `1_000.5f`. That
is what makes the sentence in 14.1.f64 -- "floating point literals are NOT
typeless" -- history; it was true only as long as there was one type.

**Exactly ONE implicit conversion exists in the language, and this is it:**
`f32` -> `f64`. It loses nothing, because every binary32 IS a binary64. The
other direction throws digits away and needs `as f32`
(`tests/neg/f32_no_narrowing.fi`). It sits in ONE place in each compiler --
`sema::expr` marks it, `lower::lower_expr` carries it out -- so that no
context can lose it; `expr_types` keeps the OWN type of the expression,
because reading an `f32` variable as if it were eight bytes wide would reach
beyond its storage.

With two floating point operands of different width the WIDER one wins, and
symmetrically: `a_f32 + b_f64` and `b_f64 + a_f32` mean the same thing. That
is not decoration -- a meaning that depends on the order of writing is
exactly the kind of surprise this language does not want.

**THE ERROR THIS ROUND FOUND, written down so that nobody repeats it.**
Reading a decimal as a correctly rounded binary64 and narrowing it afterwards
is **not** correctly rounded. At the exact middle between two binary32 the
first rounding lands ON the middle, and the tie-to-even of the second then
decides without knowing which side the real value lay on. It is not a rarity:
measured against glibc `strtof`, **63568 of 239064** such cases came out one
ulp wrong. Figueroa's theorem (2p+2 bits are enough) does not carry here --
it holds for the results of ARITHMETIC, not for an arbitrary decimal.

The way that works is **round-to-odd** in between: cut off, and set the last
bit when anything was left over. A value whose last bit is set is never
exactly a binary32 middle, so the second rounding always has a direction and
it is the right one (Boldo/Melquiond; 53 >= 24 + 2). `float_exact` in the
lexer of `firnc1` and `strtod` in `lib/num` have that mode; `firnc0` reads
the text straight into an `f32`. The float token carries BOTH bit patterns
since round 71, because the narrowing has to happen on the TEXT and the text
only exists in the lexer.

**Text out again** has a round-trip guarantee of its own, and it is not the
one of `write_f64`: the shortest text for an `f32` is not the shortest text
of the double next to it (`0.1f` widened is 0.100000001490116119384765625).
`lib/num/f32_text.fi` shortens the digits of the widened double and CHECKS
every candidate -- read back, narrowed, compared octet for octet -- so that
whatever comes out has led back to the same value once. Among texts of equal
length the nearer one wins.

**Proof:** `tests/1450_f32_basics.fi`, `tests/1451_f32_context.fi`,
`tests/1452_f32_abi.fi`, `tests/1453_f32_library.fi`, four negative tests,
`tools/abi/run.sh` (the calling convention against GCC, both directions),
`tools/lexnum/run.sh` (four number readers, both widths) and
`tools/f32data/run.sh` (a real WAV and a real glTF against Python).

**Deliberately left out:** `f16`/`bf16`, `%` and bit operations on `f32` (the
same rule as for `f64`), and constant folding of floating point -- that comes
with `comptime`.

#### 14.1.str -- strings and numbers <-> text (round 2, module `str`)

With round 2, 8.1-8.4 are implemented: `Bytes`/`Str`/`Str16`/`Atom` with the
layout fixed in 8.1, WTF-16 **without any check at all**, WTF-8 as a lossless
bridge, correctly rounded `strtod` and shortest output with a round-trip
guarantee. Deliberately narrower than the text above is the following:

S1. **String literals have been usable in the source text since round 8.**
    `compiler/src/strings.rs` decodes `"..."` (UTF-8, checked),
    `b"..."` (raw octets) and `u"..."` (WTF-16) including all escapes, among
    them `\uXXXX` and `\u{...}` **with unpaired surrogates**;
    `firnc --strlit=<literal>` shows the result. The lexer calls that now
    (`lexer::string_literal`, BEFORE identifier recognition -- otherwise
    `is_ident_start` swallows the `b` or `u` of the prefix).

    **A literal is an ARRAY literal**, not a type of its own: `"abc"` has the
    type `[u8; 3]`, `u"abc"` the type `[u16; 3]`. So all the rules for arrays
    apply -- the length check in particular: `var a: [u8; 5] = "abc"` reports
    *array literal has 3 elements, 5 are expected*. The parser converts the
    literal immediately into `ExprKind::ArrayLit`; the type checker, lowering
    and the code generator never see it.

S8. **Literals lie in the frame, not in `.rodata`.** It follows from S1 that the
    data come into being as a sequence of individual store instructions when the
    block is entered. For messages and paths that makes no difference, for a
    4 KiB table it would. A real `.rodata` section with `Str`/`Str16` as a
    library type (a pointer plus a length) comes with the standard library;
    `LitValue::asm_data()` already produces the assembly data for it.
    Open on top of that: `Str`/`Bytes`/`Str16` as the **type** of a literal -- today the
    result is an array that one fills into one of those types by hand.
S2. **No floating point type in the language.** `strtod` returns and `dtoa`
    consumes the **bit pattern** of a `binary64` as a `u64`. The computation is
    entirely integer anyway (exact big number arithmetic); as soon as `f64`
    exists, only a thin shell comes on top. Rounding and the special values
    (`+/-0`, `+/-Infinity`, `NaN`) follow IEEE 754 and ECMAScript respectively.
S3. **`Bytes`/`Str`/`Str16`/`Atom` are library types** (`lib/str/*.fi`), not
    built-in types. The layout from 8.1 is adhered to; the separation is
    enforced by the type checker, because they are different `struct` types
    (negative tests `tests/neg/str_bytes_is_no_text.fi`,
    `tests/neg/str16_is_no_bytes.fi`). `Str` is `Bytes` with checked content
    (`bytes_is_str`), not a type of its own -- so the reinterpretation of
    `Bytes` as `Str` is not compiler-checked yet.
S4. **The API is pointer-based.** Because 14.1 item 1 (no aggregates across
    function boundaries), item 5 (no global variables) and the missing
    `inout`/`&` apply, the constructor is called `str16_init(s: *mut Str16)`
    instead of `str16_new() -> Str16`, and `atom_intern` takes the table as its
    first parameter. The names `str16_push`, `str16_len`, `str16_at`,
    `atom_intern` are unchanged as agreed.
S5. **No `Wtf8` type of its own.** WTF-8 is a `Bytes` representation with the
    functions `str16_to_wtf8` / `wtf8_to_str16`.
S6. **No `Rope`** (8.5 is a SHOULD without a date).
S7. **Atoms get their numbers at run time**, in the order of interning (8.3
    provides for fixed numbers at build time). Whoever needs fixed small numbers
    interns their names at startup in a fixed order.
S8. **`strtod` does not recognize the special forms**: no `Infinity`, no `NaN`,
    no hexadecimal floating point, no leading whitespace. Such inputs return
    "nothing consumed" (`consumed == 0`).
S9. **Beyond 780 significant digits** a sticky bit is set instead of computing
    on. The result stays correctly rounded (truncated digits can only leave an
    exact halfway point, never create one).

#### 14.1.opt -- the optimizer and register allocation (round 2, module `opt`)

O1. ~~**No register allocation, every value in a stack slot.**~~ **Struck in
    round 2** (module `opt`, requirement `P3`): `compiler/src/regalloc.rs`
    contains a real allocation -- liveness analysis per basic block, from it one
    interval per value, then a **linear scan** with an active list and weighted
    spilling (uses x loop depth). Handed out are `rbx`, `r12`-`r15`
    (callee-saved, saved in the prologue/epilogue) as well as `r8`-`r11` for
    intervals that do not enclose a `call`/`syscall`.
    The sentence in 14 ("a stack slot of its own for every FIR value ... pure
    spilling") describes only the **baseline path** from round 2 on, which still
    exists and is used when `emit_func_ra` is not responsible (see O2). Proof:
    `tests/opt/regalloc_loop.fi` -- the loop body in the generated assembly
    contains **not a single** stack access (`bash test_opt.sh` checks that).
O2. **Two codegen paths.** The register-aware path takes over only what it
    masters completely. It hands over to the baseline path with: more than six
    parameters or arguments (stack arguments, 14.1 item 9), unknown block
    numbering and **instruction-accurate debug lines switched on** (`--no-opt`,
    14.1 item 16). So: `--no-opt` produces code from the baseline path, without
    `--no-opt` code from the register path. Both paths deliver the same result
    for every test program -- which is exactly what `test.sh` checks.
O3. **Phi nodes -- CLOSED IN ROUND 92.** This entry used to say that FIR has
    none, that `mem2reg` could therefore only resolve `alloca`s written
    **exactly once**, and that cells written several times (loop counters!)
    were left to the register allocator, which keeps a non-escaping cell of
    up to 8 bytes in a register for the whole function -- "functionally
    equivalent, but more local than real SSA".
    Since round 92 `mem2reg` is the real SSA construction (dominator tree,
    dominance frontiers, renaming) and writes `fir::Op::Phi`; `phi.rs` takes
    the phis apart into copies again before code generation, so no backend
    sees one. The lowering is unchanged and `--emit=fir-raw` stays phi-free.
    The register allocator's cell promotion stays as well -- it still catches
    what `mem2reg` deliberately leaves in memory (mixed access widths,
    escaping addresses, `secret`). See `docs/ROUND92.md`.
O4. **The performance target 10.3 (`P1`, <= 2x Rust) not reached yet.** Measured
    on 2026-08-13 with `bash bench/run.sh` (6 microbenchmarks, each in duplicate
    in Firn and in Rust `-O`, the median of 7 runs): **median 2.75x**, range
    1.57x (Fibonacci) to 4.95x (matrix multiplication). The numbers are in
    `bench/RESULTS.md`, `README.md` and `ACCEPTANCE.md` -- not dressed up. The
    distance arises above all where LLVM vectorizes (sieve, matmul); Firn
    produces scalar code exclusively (no SIMD, `L16` open).
O5. **Bounds checks (`P5`).** In stage 0 the language does not produce any
    bounds checks at all (14.1 item 3). The pass that exists removes **provably
    repeated conditions** instead: a `brcond` whose condition was already
    decided on the way there (a chain of blocks with exactly one predecessor)
    becomes an unconditional jump. Proof: `tests/opt/redundant_check.fi`.
O6. **Inlining across module boundaries** follows from the fact that the module
    system (14.1 item 15) compiles all the files into ONE `fir::Module`; an
    imported call is indistinguishable from a local one for the pass. Recursion
    (indirect recursion included) is never inlined, `#[constant_time]` functions
    and functions with `secret` values stay outside.

#### 14.1.error_unions -- error unions `E!T` (round 3, module `fehlerunionen`)

With round 3, 5.1 is implemented as a language feature: the `error` declaration,
the type syntax `E!T` (return type, variable type, field type, parameter type),
implicit conversion at `return`, `try`, `catch` and `catch |e| fallback`. A `!T`
value is implicitly `#[must_consume]`.

**The representation (binding).** An error union is a struct in
`types::TypeCtx` with `__err: u32` at offset 0 (`0` = success, error codes from
`1` on in the declaration order of the error set) and `__val: T` at
`round_up(4, align(T))`; the pure error value `E` is the struct
`{ __err: u32 }`. So the aggregate ABI (14.1 item 1), register allocation and
codegen apply unchanged. The side tables and the checking are in
`compiler/src/errors.rs`, the lowering in `compiler/src/lower_errors.rs`.

Deliberately narrower than the text in 5.1 is the following:

F1. **The error set is not inferred.** `E!T` has to be written out in full; a
    `!T` without an error set (5.1: "the error set may be left out and inferred
    by the compiler") is not implemented and reports a syntax error with line
    and column.
F2. **Error set names are program-wide**, not per module -- like enumeration
    names (14.1.types). `LeseFehler::Ende` applies in every file,
    `modul.LeseFehler::Ende` does not exist. Proof:
    `tests/414_module_error.fi`.
F3. **`try` demands the same error set.** There is no union or widening of error
    sets and no re-keying while passing on; different sets are an error with
    line and column (`tests/neg/err_wrong_set.fi`).
F4. **`catch |e| ...` binds to an expression, not to a block.** The error value
    `e` has the type of the error set and is examined with `==`/`!=`
    (`tests/419_catch_binding.fi`); `match e { ... }` on an error value is
    **not** implemented and reports a clean error. In its notation too, `catch`
    is thereby narrower than the example in 5.1, which shows a block with a
    `return` inside it.
F5. **`defer` has existed since round 9, `errdefer` had not yet.**

    `defer <statement>` postpones the statement until the enclosing block is
    left. What is allowed is a block (`defer { ... }`) or a single statement
    (`defer close(fd)`). It is executed in **reverse order** of declaration, and
    on every exit: at the end of the block, at `return`, at `break` and at
    `continue`.

    * **`return` clears all levels**, the innermost first -- the return value
      has already been computed at that point (`lower::ret_term`).
    * **`break`/`continue` clear exactly the levels that were declared INSIDE
      the loop** (`lower::loops` remembers the depth of the `defer` stack when
      the loop is entered for that purpose).
    * **The time of evaluation: like Zig, not like Go.** The body is evaluated
      only on exit, with the values of *that moment*. Go evaluates the arguments
      immediately and stores them in hidden copies; that contradicts the
      principle "nothing hidden" (2). Proof: `tests/580_defer.fi`, section 3.
    * **A jump out of the body is an error** (`return`, `break`, `continue`) --
      it would tear apart the order of the remaining postponed statements.
      Negative tests `tests/neg/defer_return.fi`, `tests/neg/defer_break.fi`.
      Inside a loop that begins in the `defer` itself, `break`/`continue` are
      allowed.

    **`errdefer` has existed since round 10.** It runs only if the function is
    left through an error. `defer` and `errdefer` share ONE list per block level
    and run in a common reverse order -- if the `errdefer` stands behind the
    `defer`, it therefore runs first.

    What counts as the error path:
    * propagation through **`try`** (`lower_errors::return_error`),
    * a **`return E::Variant`** -- the type checker reports that as
      `CoerceKind::FromError`.

    An ordinary return value does not count as the error path, even if the
    function returns an error union.

    **An honest limit:** if a FINISHED error union is passed on
    (`let u: E!i32 = f()` ... `return u`), it is only known at run time whether
    the error path is taken. Stage 0 does not decide that and **rejects the
    case**, instead of silently ignoring `errdefer` -- with a note about
    `return try ...`. Proof: `tests/neg/errdefer_union_propagation.fi`.
    The run-time distinction (two cleanup paths behind a branch on the error
    code) is possible and will come when it is needed.
F6. **No success type `()`.** `E!()` cannot be written (stage 0 does not know
    `()` as type syntax); a function without a useful result returns for example
    `E!i32`.
F7. **Where implicit conversion happens** is enumerated exhaustively: `return`,
    `let x: E!T = ...`, assignment, a field of a struct literal and an argument
    of a call. In array literals and in comparisons there is **no** conversion.
F8. **In the error case `__val` is undefined.** Only `__err` is defined; whoever
    reads the success value in the error case (only possible through a struct
    view of one's own) reads filler values.
F9. **`catch` binds more weakly than any operator.** `a catch b * 2` is
    `a catch (b * 2)`, `(a catch b) * 2` needs parentheses. `try` binds as
    tightly as a unary operator: `try f() + 1` is `(try f()) + 1`.
F10. **An error union over a struct success type is no good as the field type of
    a struct.** When `sema::collect_structs` resolves the field types, the
    struct layouts are not fixed yet -- the error union would get a wrong size.
    Instead of a silently wrong layout there is an error with line and column
    (`tests/neg/err_union_in_struct.fi`). With a scalar success type
    (`E!i32`, `E!*mut u8`) the field type is allowed
    (`tests/408_union_field.fi`), and as a return, variable and parameter type
    every success type is.

F11. **`E!f64` carries its value** -- since round 68, and not before. Up to
    round 67 the success value of an error union over `f64` was silently
    NOT copied: `lower_errors.rs` had a table of its own for "which FIR type
    does this source type have" and that table had lost `f64`, so the copy
    turned into nothing and the reader got whatever lay in the slot.
    `lib/firnc1/lower.fi` carried the same hole, written in deliberately to
    stay bug compatible. There is one table now.
    Proof: `tests/1249_error_union_f64.fi`, `tests/1004_js_f64_union.fi`.

F12. **An error union over a struct of the SAME MODULE works -- since round
    76, and not before.** `fn f() -> E!S` inside a module whose `S` is
    declared right above it reported `unknown type 'S'`; in the root module
    the identical code worked. The cause was not the type system:
    `errors::hook_type` puts the success type of `E!T` ASIDE (into
    `REG.pending`) and leaves the placeholder `__eu#<n>` in the syntax tree,
    and `modules.rs::Resolver::ty` -- the pass that qualifies every type name
    of a module -- walks the tree, where the payload no longer is. So `S`
    never became `modul__S`.
    `firnc1` never had the bug: it renames while PARSING, so the payload is
    already qualified when it goes into the side table.
    Found while `lib/std/net.fi` was written (`NetError!Listener`), fixed in
    `errors::pending_inner`/`set_pending_inner`. Proof:
    `tests/1600_net_echo.fi` and every function of `lib/std/net.fi`,
    docs/ROUND76.md 4.1.

#### 14.1.asm -- the operands of an inline assembly block (round 52, 68)

```ebnf
asm_expr = "asm" "(" str_lit { "," asm_op } ")" ;
asm_op   = "in"      "(" str_lit ")" expr
         | "out"     "(" str_lit ")"              (* the VALUE, round 52 *)
         | "out"     "(" str_lit ")" expr         (* into MEMORY, round 68 *)
         | "clobber" "(" str_lit ")" ;
```

A1. **`out("rax")` without a target is the value of the expression.** There
    is at most ONE of those -- an expression has one value. That is the form
    of round 52 and it is unchanged.

A2. **`out("rdx") p` writes the register into `*p`, after the template has
    run.** Any number of those. `p` is a POINTER; what is written is always
    the WHOLE register, so the target has to be eight octets wide (`u64`,
    `i64`, `usize`, `isize` or a pointer). A narrower one would have its
    neighbour overwritten silently, which is why the type checker refuses it
    (`tests/neg/1246_asm_out_width.fi`).

A3. **Every output needs a register of its own.** Two outputs on the same
    register are an error with line and column
    (`tests/neg/1243_asm_out_twice.fi`) -- one of the two results would be
    lost without a word being said.

A4. **Order of evaluation:** the input expressions in source order, then the
    output ADDRESSES in source order, then the template. The register a
    result lands in is decided by the register NAME, not by the position in
    the operand list.

A5. **The code generator saves every result register on the stack first**
    and only then uses `rax`/`rcx` to write the results out. The other way
    round would destroy a result that is still to be written: an address has
    to be loaded into a register, and that register may itself be an output.

A6. **A memory AREA travels as a pointer in a register** (`in("rcx") p`)
    plus `clobber("memory")`. There is no placeholder syntax in the
    template -- what stands in it is x86 assembly and nothing else.
    Proof for all of it: `tests/1242_asm_multi_out.fi`, and in the kernel
    `demos/kernel/user.fi::rdmsr`, which takes edx:eax apart the way the
    processor delivers it.

#### 14.1.round68 -- three more places where a type is accepted (round 68)

R1. **`Gc[Derived]` fits into `AllocError!Gc[Base]`.** The free upcast of
    4.4 holds at `return`, in a `let`, as an argument and behind `catch`,
    also when the target type is an error union over the base type. Up to
    round 67 the conversion into an error union ran through a copy of the
    compatibility rule that did not know the upcast, and a local of the base
    type had to be put in between (`docs/ROUND63.md`, gap 7).
    Downwards there is still exactly one way: the checked `x.as?[C]`.
    Proof: `tests/1236_gc_upcast_return.fi`,
    `tests/neg/1237_gc_upcast_only_upward.fi`.

R2. **A function value in a FIELD is callable.** `c.hook(a, b)` where
    `hook` is a field of type `fn(A, B) -> R`. The receiver is NOT an
    argument -- it is only the place the value is read from. The resolution
    order is unchanged: a METHOD of the same name wins, the field is only
    looked at when there is no method. What that costs is measured on the
    emitted code, not asserted: `tools/fnfield/run.sh`.

R3. **`-x` works on `f64`.** 14.1.f64 named it as implemented and it was
    not. What the code generator does is flip BIT 63, not run `neg`, which
    is what makes `-0.0` come out as `-0.0` -- `0.0 - x` does not
    (`tests/1247_f64_negation.fi`). `~x` on an `f64` stays an error: a
    floating point value has no bits whose flipping would mean anything
    (`tests/neg/1248_f64_has_no_bitnot.fi`).


---

## 15. Open questions

1. **Retrofit precise stack scanning later?** Conservative scanning (3.5.3)
   rules out a compacting collector permanently. If fragmentation becomes a
   problem in the soak test, that is the place where improvement is needed --
   and it will be expensive. The decision is deferred until after the 24 h test.
2. ~~**`async`/coroutines**~~ -- **decided on 2026-08-14**: `Io` as a parameter
   instead of a language colour (7, `DESIGN_GOALS.md` 1). What stays open is
   only the size of the coroutine stacks and whether they may grow.
3. **Ropes** (`Z3`, a SHOULD) -- from when on is the effort worth it?
4. **SIMD** (`L16`, a SHOULD) -- as built-in vector types or only through inline
   assembly?
5. **Conditional moves.** Reject conservatively (the current choice) or use
   run-time flags like Rust's drop flags?
6. **Package management** (`W1`, a MUST) -- the design is still outstanding.
7. **Alignment with Osum.** As soon as `firnc` compiles kernel modules, the
   calling convention has to be checked against the existing Rust code in osum.

---

## 16. Traceability: requirement -> section

So that it can be checked that no requirement from `FIRN-ANFORDERUNGEN.md` has
quietly fallen off the table. The state of the implementation is in
`ACCEPTANCE.md`, not here.

| Requirement | Section |
|---|---|
| `S1` deterministic memory management as the default | 3.3 |
| `S2` an opt-in GC heap with cycle resolution | 3.5 |
| `S3` weak references | 3.4 (`Weak`), 3.5.2 (`GcWeak`) |
| `S4` finalizers - `S5` incremental - `S6` pause times | 3.5.3 |
| `S7` shared immutable values with counting | 3.4 (`Rc`/`Arc`) |
| `L1` self-hosting | 11 |
| `L2` memory safety as the default - `L3` marked unsafety | 3.3, 3.6 |
| `L4` sum types + an exhaustiveness check | 6.3 |
| `L5` generics with monomorphization | 6.1 |
| `L6` interfaces static **and** dynamic | 6.2, 4.4 |
| `L7` result types - `L8` unwinding for JS | 5.1, 5.3 |
| `L9` integer semantics - `L10` IEEE 754 | 13 |
| `L11` `const` evaluation - `L12` deep recursion | 6.1, 5.2 |
| `L13` a stable ABI - `L14` C FFI - `L15` assembly - `L16` SIMD | 13, 3.6, 15.4 |
| `Z1`-`Z7` strings including WTF-16 | 8 |
| `N1`-`N7` concurrency | 7 |
| `P1`-`P9` code generation and performance | 10.3 |
| `G1`-`G4` compile-time code generation | 6.4 |
| `C1`-`C7` crypto and constant time | 9 |
| `B1`-`B13` the standard library | 8, 3.4 -- the rest in ROADMAP phase 3 |
| `R1`-`R6` the runtime on Osum | ROADMAP phase 6 |
| `W1`-`W10` tools | 10.4 |

---

*This document is the truth about the goal. The code is the truth about the
state. Where the two diverge, the code wins -- and the document gets corrected,
not the code glossed over.*

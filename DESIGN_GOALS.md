# DESIGN_GOALS.md -- what Firn is meant to do differently from the start

**As of:** 2026-08-14 - **Related:** `SPEC.md` v0.2, `ROADMAP.md`, `ACCEPTANCE.md`

This document collects known weak spots of today's system languages and records,
for each of them, how Firn deals with it. **The point is not to build everything
at once** -- the point is to foreclose nothing. Some of these items can be added
later; others are foundation questions that cannot be changed after ten years of
code without tearing everything open. Separating those two categories is the
real purpose of this document, and it is in section 10.

The structure of each item: **problem** (with a real example) - **state of the
art** - **Firn's approach** (technical, not a declaration of intent) -
**consequence for the compiler and the language core today** - **priority and
phase**.

Wherever an approach is expensive or collides with another goal, the conflict is
stated explicitly. That matters more than any idea for a solution.

---

## 1. Function colours -- `async` tears ecosystems apart

### Problem

As soon as a function is `async`, every caller has to be `async`. The colour
eats its way up through the entire tree. The concrete consequences:

* **Rust:** there is `std::io::Read` and `tokio::io::AsyncRead` -- two
  incompatible ecosystems for the same thing. A library has to decide or
  maintain both twice over (`feature = "async"`). Traits with `async fn` were
  not possible at all for years, and even today `async fn` in traits comes with
  restrictions. On top of that the *runtime* colours as well: `tokio` code does
  not run on `async-std`.
* **JavaScript:** `await` only works in an `async function`; everything else
  needs `.then()`. The language has two control flows as a result.
* **Python:** `asyncio` has produced a second universe of libraries
  (`requests` <-> `aiohttp`, `psycopg2` <-> `asyncpg`).

For a browser this is not an academic problem: `fetch`, the event loop, web
workers, `Promise`, generators, timers and network I/O are all concurrent -- and
the rasterizer, the tokenizer and the layout code are not. If the language
colours, the engine falls apart into two halves.

### State of the art

* **Rust/JS/Python:** they colour. A state machine transformation in the
  compiler, the runtime in the library. Best performance, worst ergonomics.
* **Go:** does not colour -- goroutines are stackful, every call may block.
  The price: a runtime that is always there, growing stacks, a mandatory GC, no
  freestanding operation. Unusable for a kernel.
* **Zig 0.16 (April 2026):** the interesting third route. `Io` is passed as a
  **parameter**, exactly like the `Allocator`. From the release notes:
  *"Starting with Zig 0.16.0, all input and output functionality requires being
  passed an `Io` instance."* `file.close()` becomes `file.close(io)`.
  `io.async(...)` produces a `Future(T)` and expresses **independence**, not
  concurrency -- which is why it is infallible and runs on restricted `Io`
  implementations as well. `io.concurrent(...)` demands real concurrency and may
  fail with `error.ConcurrencyUnavailable`. The implementations: `Io.Threaded`
  (finished), `Io.Evented` (experimental, stackful coroutines) with the backends
  `Io.Uring`, `Io.Kqueue`, `Io.Dispatch`, plus `Io.failing` for tests.
  **The colour moves out of the language and into the type system** -- it is
  still there, but it is an ordinary parameter and not a second language.

### Firn's approach

**Zig's model is adopted -- unchanged in principle, because it fits Firn's
guiding principle 1 ("nothing happens hidden") and the explicitly passed
allocators exactly.** Firn therefore has **no** `async`, **no** `await` as a
keyword and **no** state machine transformation in the compiler.

```firn
// No async. io is a parameter like alloc.
fn fetch(io: Io, url: &Str, alloc: inout Arena) -> NetError!Response {
    let conn = try io.connect(url.host, 443)
    defer conn.close(io)
    try conn.write(io, request_bytes)
    return try conn.read_all(io, inout alloc)
}

// Express independence -- do not demand concurrency:
fn load_page(io: Io, urls: &[Str], alloc: inout Arena) -> !Vec[Response] {
    var futs = Vec[Future[NetError!Response]].with_capacity(inout alloc, urls.len)
    var i: usize = 0
    while i < urls.len {
        futs.push(io.async(fetch, io, &urls[i], inout alloc))   // infallible
        i = i + 1
    }
    defer futs.cancel_all(io)          // cancellation is a mandatory part of the contract
    return futs.await_all(io)
}
```

The core points of the Firn version:

1. **`Io` is an ordinary interface value** (`SPEC.md` 6.2, `dyn Io` with a
   vtable or statically monomorphized). No language magic.
2. **`io.async` expresses independence, not concurrency** and is infallible.
   `io.concurrent` demands concurrency and may fail. That distinction is Zig's
   best single idea in this area, because it lets the same code run on a
   single-thread `Io` and on an io_uring `Io`.
3. **`Future[T]` is `#[must_consume]`** -- whoever drops a future without
   `await` or `cancel` gets a compile error. That rules out the most common
   class of bug in Rust's `async` (forgotten futures that never run) from the
   outset.
4. **Cancellation is first class, not retrofitted.** `cancel(io)` is part of the
   contract of every `Future`. That is exactly what Rust's `async` painfully
   lacks to this day (cancellation safety is an unwritten rule there, not a
   type).
5. **Implementations in the `app` profile:** `Io.Threaded` (threads, first),
   `Io.SingleThread` (deterministic, for reproducible reftests -- satisfies
   `N7`), later `Io.Evented` with stackful coroutines. In the `kernel` profile
   there is a version of `Io` of its own that maps onto osum primitives.
6. **Stackful coroutines instead of state machines.** That is the price (see
   below), but it keeps the code generator small and allows *every* function to
   run inside a coroutine -- including deeply recursive ones such as the HTML
   tree builder, which nobody would ever want to transform into a state machine.

### Conflict of goals -- named openly

* **Stackful coroutines cost memory.** Each one needs a stack of its own. With
  ten thousand simultaneous connections that matters; with a browser and a few
  hundred simultaneous requests it is uncritical. State machines would use less
  memory, but they colour. **Firn chooses memory over ergonomics.**
* **Passing `Io` through is tedious legwork.** Every function that does I/O
  somewhere further down needs the parameter. That is honest, but annoying --
  the same criticism applies to Zig's allocator model. The remedy: `Io` and
  `Allocator` are frequently passed together in a `Ctx` struct.
* **Collision with `#[no_gc]` and constant time:** a function that takes `Io`
  cannot be `#[no_gc]` if the `Io` implementation allocates. That is intended
  and is enforced by the checker.

### Consequence for the compiler TODAY

Astonishingly small -- **that is the strongest argument for this model**:

* **Nothing in the code generator.** No `async` lowering, no state machines, no
  unwinding across coroutine boundaries.
* What is needed is needed anyway: `interface` with dynamic dispatch
  (`SPEC.md` 6.2), `#[must_consume]`, generics.
* **One real foundation requirement:** stackful coroutines need **stack
  switching** -- a small assembly routine that swaps `rsp`, `rbp` and the
  preserved registers. That is little code, but it demands that the code
  generator makes **no assumptions about stack continuity**: no red zones
  without need, no pointers to stack frames that would have to survive a switch.
  Firn's second-class references (`SPEC.md` 3.3) are a stroke of luck here: a
  reference cannot leave its call frame anyway.
* **What would already be wrong today:** introducing an `async` keyword. That
  does not happen.

### Priority and phase

**Foundation: only the compatibility with stack switching** (free of charge if
you know about it). **Implementation: ROADMAP phase 3** (`Io.Threaded` with the
concurrency library), **phase 4** (`Io.Evented`).
`SPEC.md` 7 is corrected accordingly: it still says "no `async`, `N6` stays
deliberately unsatisfied" -- what is right from now on is "no `async` **as a
language colour**; `N6` is satisfied through `Io`".

---

## 2. Failing allocation

### Problem

`Vec::push` in Rust **panics** when there is no memory. For an application
program that is defensible; for a kernel, for a browser tab with a memory limit
or for an embedded device it is unacceptable.

The proof is Linux: Rust-for-Linux could not use `alloc`, because every
allocation there is allowed to abort. It took `try_reserve` (only stabilized in
Rust 1.57, at the end of 2021), a kernel `Vec` of its own with
`push_within_capacity` and `try_push`, and to this day the kernel version is a
parallel world to the standard library. **The retrofit never became complete**,
because the failure path is missing from the type system: `Vec<T>` has no place
where an error would be allowed.

Zig does it the right way round: `try list.append(alloc, x)` -- the error is in
the type from the start. That is exactly why Zig is usable in the kernel space.

### State of the art

| Language | Behaviour |
|---|---|
| Rust `std` | panic. `try_reserve` retrofitted, does not cover everything |
| Rust `no_std` + `alloc` | the same; Linux maintains a collection library of its own |
| C | `malloc` returns `NULL` -- correct, but nobody checks it |
| C++ | `std::bad_alloc` as an exception; unusable in `-fno-exceptions` projects (that is, in almost all system projects) |
| **Zig** | **every allocation is fallible, `Allocator` is a parameter.** The model to follow |

### Firn's approach

**Every allocation is fallible -- from the first line on, without exception.**

```firn
// There is NO infallible allocation function in the standard library.
fn build(alloc: inout Allocator) -> AllocError!Vec[u32] {
    var v = try Vec[u32].with_capacity(inout alloc, 16)
    try v.push(inout alloc, 42)        // fallible, visible
    return v
}
```

For this to stay **ergonomic** -- otherwise nobody writes it and everybody
reaches for the shortcut -- three means are added:

1. **`try` is one word.** As in Zig. `try v.push(inout a, x)` is exactly as
   short as `v.push(x)` plus four characters. That is the difference between
   "feasible" and "nobody does it".
2. **Reserving capacity up front is the recommended style.**
   `try v.reserve(inout a, n)` once, then `v.push_within_capacity(x)` --
   **infallible**, because the capacity is proven. The hot path (tokenizer,
   rasterizer) thereby has **zero** error handling in the innermost loop. That
   is exactly what Linux rebuilt with `push_within_capacity` -- only here it is
   the normal route from the beginning.
3. **Arenas need no error handling per element at all.** `Arena.alloc` only
   fails when a new block is needed; whoever sizes the arena beforehand has an
   infallible path. For parsers and layout -- the browser's main sources of
   allocation -- that is the normal case.

**Unhandleable allocation failures** exist nevertheless: if the GC or the
runtime itself gets no more memory, that is a panic. But that is a clearly named
small set of places -- not every `push`.

**Interplay with the `app` profile and the GC:** `Gc[T]` allocation is
**fallible as well** (`AllocError!Gc[T]`). A collection is attempted first; only
if there is still no memory afterwards does the error come. That is the basis
for memory limits per tab (`B13` from the browser requirements) -- without
fallible GC allocation a tab that wants too much can only kill the whole
browser.

### Conflict of goals

* **Error noise.** Every call with `try` is more text. The remedies are above
  (capacity up front); Firn code nevertheless stays longer than Rust code at
  this point. **Deliberately accepted.**
* **Collision with operators.** A `+` on a collection type that would have to
  allocate cannot do so -- which is why there is no operator overloading (that
  is already in `SPEC.md` 4.5) and no implicitly allocating concatenation.
* **Collision with item 6 (in-place initialization):** a fallible constructor
  that builds at the destination has to clean up the half-finished state on
  failure. That is exactly the difficulty that `try_pin_init!` solves in Linux.
  Firn's answer is in section 6.

### Consequence for the compiler TODAY

* **The error union `!T` has to sit in the language core** (`SPEC.md` 5.1) --
  it is already planned that way, but not built in stage 0 yet.
* **`#[must_consume]` has to exist**, otherwise an `AllocError!T` can be dropped
  in silence. **Done on 2026-08-14**: Firn now has an attribute system
  (`compiler/src/attrs.rs`, `firnc --list-attrs`) and `#[must_consume]` in front
  of `fn` and `struct`. What is checked is the subset decidable without a move
  checker -- *the result of a call must not be discarded as a statement*; the
  full form follows with the move checker.
  An important side effect: **no known attribute is silently ignored.**
  `#[constant_time]`, `#[no_gc]` and the other ten are entered in the registry
  and are rejected with a clear message including line, column and intended
  purpose, instead of standing there without effect -- an overlooked
  `#[constant_time]` would be the most dangerous kind of bug in this language.
* **The standard library must never get an infallible allocation function.**
  That is a rule for phase 3 and the one place where discipline matters more
  than technology: if `Vec.push(x)` without `try` ever exists, it will be used
  and the whole approach is dead.
* Today (stage 0) there is no allocation -- so **nothing is foreclosed**.

### Priority and phase

**FOUNDATION.** Retrofitting is demonstrably impossible (Linux tried).
`!T` + `#[must_consume]` in **phase 2**, the standard library following this
rule in **phase 3**.

---

## 3. Capability-based modules -- supply chain security

### Problem

Today **every** library that is pulled in may do **everything**: read files,
open the network, start processes, read environment variables. A string helper
library has the same rights as the network stack.

Real incidents: `event-stream` (npm, 2018) -- a package that had been taken over
smuggled in code to steal Bitcoin wallets. `ua-parser-js`, `coa`, `rc` (npm,
2021) -- crypto miners and password thieves in packages with millions of weekly
downloads. `xz-utils` (2024) -- a back door built up over two years in a
compression library, which ended up in `sshd` through systemd. In every case the
technical precondition was the same: **the code was allowed to do things that
would never have been necessary for its job.**

For a browser this is the core question. An image decoder processes hostile
input from the network -- it must **never** be allowed to open a file.

### State of the art

* **Rust/Go/C++:** no rights model at all. `cargo` even runs the `build.rs` of
  arbitrary packages during the build, with full permissions.
* **Deno:** rights at **process level** (`--allow-net`), not per module. Better
  than nothing, but a single malicious module inherits all the rights of the
  process.
* **WASI / the WebAssembly component model:** capability-based and thought
  through correctly -- but it is a sandbox around foreign code, with marshalling
  costs at every boundary.
* **Osum itself:** capability-based. **This is exactly where Firn's
  opportunity lies.**

### Firn's approach

**One model for the language and the operating system.** A capability in Firn is
**the same thing** as a capability in Osum: an unforgeable handle that can be
owned or passed on, but not invented.

The trick: **Firn needs almost no new mechanism for it** -- the `Io` value from
section 1 and the `Allocator` from section 2 *are* capabilities already. Whoever
gets no `Io` cannot do I/O. That is not a check, it is a consequence of the type
system.

Two additions come on top:

**(a) The module declaration.** Every package declares in its description which
capabilities it may request at all:

```toml
# firn.toml of an image decoder
[paket]
name = "png"

[faehigkeiten]
# empty = this package can do NOTHING except compute.
# No Io, no file system, no network, no time, no randomness.

[bauzeit]
skript = false        # this package runs no code during the build
```

The compiler checks that **statically**: a module without the `netz` capability
may not even name the type `NetIo`. A violation is a compile error, not a
run-time error.

**(b) Restricted handles when passing them on.** Capabilities are **narrowed**
when passed through, never widened:

```firn
// The caller decides what the decoder may see:
let bild = try png.decode(daten, inout arena)   // no io, no global alloc
```

**(c) Build time is the most dangerous moment.** `cargo`'s `build.rs` is the
most convenient attack route there is. In Firn the following holds: build
scripts (`SPEC.md` 6.4) run **in a sandbox without network access and without
write permissions outside their output directory**, and a package that has a
build script has to declare it (`skript = true`). The package manager shows that
when the package is added.

**(d) Enforcement at run time.** Static checking covers everything the compiler
sees. For dynamically loaded components (Osum processes, browser tabs)
**Osum** enforces the same capabilities -- the language model and the kernel
model are congruent, there is no translation layer.

### Conflict of goals

* **`unsafe` undercuts all of it.** A module with `unsafe` and inline assembly
  can issue a syscall directly. The honest answer: **the capability check of the
  language only applies to safe code.** For unsafe code Osum has to enforce
  it. The package manager therefore has to count and show the `unsafe` blocks
  per package -- that is the only warning that carries weight.
* **Ergonomics.** Passing capabilities through is the same effort as passing
  `Io` through (section 1) -- it does not add up, because it is *the same
  thing*.
* **Collision with item 4 (a stable ABI):** a handle that crosses an ABI
  boundary needs a stable representation.

### Consequence for the compiler TODAY

* **The module system has to know a package boundary from the start**, not just
  files. Firn today has `import path.module` inside a project -- the extension
  by "which *package* is that, and what may it do" has to be thought through
  when the package manager is designed (phase 3), otherwise it is a break later.
* **No ambient capabilities in the standard library.** There must **never** be a
  `std.fs.open(path)` without an `Io` parameter. That is the same rule of
  discipline as in section 2 -- and just as irreversible.
* Nothing to build today; only nothing to get wrong.

### Priority and phase

**FOUNDATION (as a rule, not as code).** The rule "no ambient authority" has to
hold from phase 3 on. Declaration and checking in the package manager:
**phase 3**. Enforcement against `unsafe` through Osum: **phase 6**.

---

## 4. A stable ABI

### Problem

Rust has **no** stable ABI. `#[repr(Rust)]` may reorder fields, and between two
compiler versions anything may change. The consequences:

* No real plugins. Whoever wants extensions has to use `extern "C"` and model
  every structure as a C data type by hand -- losing generics, traits, `Option`
  and result types.
* No interchangeable system libraries. Swapping one Rust `.so` for another is
  not defined.
* `abi_stable`-style crutches exist, but they are libraries that work around the
  problem, not solve it.

### State of the art

**Swift is the only serious proof that it can be done.** Since Swift 5 (2019)
the ABI on Apple platforms has been stable -- which is why the Swift runtime
lives in the operating system instead of in every app. The mechanism is called
*library evolution* (internally "resilience") and is **opt-in per library**.

The price is well documented and considerable: with library evolution mode on,
callers have to access fields and enumeration cases **indirectly** -- through
function calls that cannot be inlined. The size and the field layout of a type
are then only known at **run time**. So there is no direct field access any
more, no inlining across the library boundary, no assumptions about layout.
Swift therefore introduced `@inlinable` and `@frozen`, with which a library
*freezes* parts of its layout and thereby wins performance back -- at the cost
of changeability.

### Firn's approach

**Firn gets a stable ABI -- but `opt-in` per component boundary, not as the
default.** That is Swift's split, only with a different default: Swift is fast
within a module and resilient at library boundaries; Firn is fast everywhere,
**except** where `#[abi_stable]` stands.

```firn
// An interchangeable Osum component:
#[abi_stable(version = 1)]
interface FensterManager {
    fn erzeuge(io: Io, breite: u32, hoehe: u32) -> !FensterId
    fn zerstoere(io: Io, id: FensterId)
}
```

The rules:

1. **The default is unstable** (`repr(firn)`): the compiler may reorder, inline
   and assume layout -- full performance. That applies to all the browser code,
   the kernel and everything linked statically.
2. **`#[abi_stable]`** switches Swift's model on for exactly this interface: a
   fixed calling convention, indirect field access, size at run time, a version
   number in the symbol.
3. **`#[frozen]`** freezes a layout and gives the performance back -- with the
   promise never to change it again. Like Swift's `@frozen`.
4. **`extern "C"`** stays available in addition (`L14`), for tools and test
   oracles.
5. **A Firn-to-Firn ABI across component boundaries** is already carried in
   `SPEC.md` 13 as an `L13` MUST -- so this item is not something new but the
   elaboration of it.

**Does Firn really need it?** Yes, but later than one thinks:

* **The browser does not need it.** It is linked statically (`R5`), blocks talk
  over IPC with serialized messages -- not over an ABI.
* **Osum needs it** as soon as system components are supposed to be
  interchangeable (drivers, window manager, services) or an app model with
  extensions loaded later comes into being.
* **IPC beats an ABI wherever it can.** A serialized message channel is more
  robust, can be versioned and fits the capability model (section 3). Firn
  should prefer that and use `#[abi_stable]` only where the cost of a channel is
  too high.

### Conflict of goals

* **Directly against performance.** Resilient types cost indirection at every
  field access and prevent inlining across the boundary -- Swift proves it.
  Hence opt-in.
* **Against items 5 and 8:** layout control (section 8) and a stable ABI are
  opposites -- whoever chooses an SoA layout cannot freeze it.
* **Against hot reload (section 9):** there a stable ABI is the *precondition*,
  not the opponent. See section 9.

### Consequence for the compiler TODAY

* **The symbol scheme has to have room for versions from the start.** If Firn
  emits `main` and `add` as bare symbols today and needs versioned ones later,
  that is a break for everything already built.
  **Done on 2026-08-14** (`compiler/src/modules.rs`):

  ```text
  _F0.add              an element of the root file
  _F0.helfer__quadrat  an element of a module
  _F0.add.v3           with an ABI version (later, #[abi_stable(3)])
  main                 the entry point, unchanged
  ```

  `SYMBOL_SCHEMA = 0` sits in **every** generated symbol: if the scheme changes,
  the linker reports a missing name instead of quietly linking two incompatible
  build states together. The prefix is reserved -- Firn identifiers may not
  contain a dot, so user code cannot produce it.
* **The real gain is the separation.** The *internal name* (type checker, IR,
  error messages) and the *linker symbol* are now two things; one becomes the
  other at **exactly one** place (`codegen_x86::label` -> `modules::symbol`). A
  first attempt to build that straight into name resolution promptly made
  `_F0.Str16` appear in type error messages -- which is exactly why the scheme
  belongs in the code generator and nowhere else.
* **Demonstrated:** `tools/symbole/run.sh` (section 8 of `test.sh`) builds a
  program with two modules that both contain the same function `hilf`, runs it
  and checks against the real symbol table (`nm`): both symbols exist
  separately, `main` is bare, and **no** Firn symbol stands there without the
  scheme prefix.
* **`SPEC.md` 13 has to record that the default layout is explicitly
  *unstable*.** Otherwise code will rely on it and it can never be changed.
* Otherwise: nothing. This is an item that really can be built later.

### Priority and phase

**CAN BE RETROFITTED** -- with cheap groundwork (the symbol scheme).
The groundwork in **phase 3**, the implementation in **phase 7/8**, when Osum
needs interchangeable components. Not before.

---

## 5. The speed of debug builds

### Problem

Rust debug builds are typically **10-50x slower** than release builds. The
consequences in practice:

* Game development in Rust is a perennial topic because of it: `bevy` recommends
  building dependencies with `opt-level = 3` and your own code with
  `opt-level = 1` -- a crutch that every project has to configure itself.
* A browser debug build that renders pages 30x more slowly is **unusable** for
  debugging layout or rasterization bugs, because the behaviour (timing,
  animations, timeouts) is a different one.
* Chromium solves it with `is_component_build` plus selective optimization;
  Firefox with `--enable-optimize --enable-debug`. **All the large projects have
  built the same makeshift** -- that is a signal about language design.

The cause is an **all-or-nothing switch**: `-O0` produces code in which every
variable lies in memory and every small call is a real call.

### State of the art

* **Rust/C++:** `-O0`/`-O2`/`-O3` as coarse levels; `-Og` in GCC/Clang is the
  right idea, but weakly developed and little used.
* **Zig:** four modes (`Debug`, `ReleaseSafe`, `ReleaseFast`, `ReleaseSmall`) --
  better, because `ReleaseSafe` optimizes *and* keeps the checks. The debug mode
  is slow all the same.
* **Go:** hardly any difference, because the compiler optimizes little in
  general. It solves this problem by having the other problem.

### Firn's approach

**Four stages, and the most important one is the default.**

| Stage | Optimization | Checks | Debugging | Purpose |
|---|---|---|---|---|
| `--dev` | **none** | all | perfect | only for hunting compiler bugs |
| **`--dev-fast`** *(default)* | **debug-preserving basic optimization** | all | **very good** | **the everyday mode** |
| `--release-safe` | full | all | moderate | shipping with a safety net |
| `--release-fast` | full | none | poor | measurements, the final product |

**What "debug-preserving basic optimization" means concretely** -- the selection
is the actual design:

**Allowed** (cheap, large effect, does not destroy the debug picture):
* `mem2reg` -- variables in registers instead of in stack slots. **That is by
  far the largest single gain** and the main reason why `-O0` is so slow. Firn
  already has it (`compiler/src/mem2reg.rs`) and measures a median of
  **~10x against `--no-opt`** for it (`bench/RESULTS.md`).
* Register allocation (linear scan) -- present.
* Constant folding, removal of obviously dead code, block merging.
* **Inlining only of trivial functions** (accessors, single-expression
  functions, wrappers). That is the limit: losing these functions in a backtrace
  bothers nobody -- they had no logic of their own anyway.

**Forbidden in `--dev-fast`** (because it destroys debugging):
* Aggressive inlining of larger functions -- the call stack becomes unreadable.
* Unrolling, interchanging or vectorizing loops -- line numbers become
  meaningless.
* Merging variables whose lifetimes do not overlap -- `gdb` then shows wrong
  values, which is worse than none at all.
* Reordering statements across line boundaries.

**The rule that holds it all together:** in `--dev-fast` **every named variable
has to show its correct value at every breakpoint**. An optimization that breaks
that does not belong in this stage. That is a checkable criterion, not a feeling
-- and it is run as a test (a `gdb` session, comparing values).

### Conflict of goals

* **Maintaining two optimization paths** costs compiler work. The remedy:
  `--dev-fast` is a **subset** of the release passes, not a chain of its own --
  only a selection.
* **The checks stay on.** `--dev-fast` therefore stays slower than
  `--release-fast`; that is intended.
* **A note to dampen expectations:** `--dev-fast` will not reach release speed.
  The target was a factor of **2-3x slower than release**, not 30x.

**Measured on 2026-08-14** (`bash tools/build_stages/run.sh 3`, the median over
the six microbenchmarks, AMD EPYC 7571):

| Benchmark | `dev` | `dev-fast` | `release-fast` | dev-fast/rel | dev/rel |
|---|---:|---:|---:|---:|---:|
| bubblesort | 1.408 s | 0.261 s | 0.111 s | **2.35x** | 12.67x |
| bytecount | 5.466 s | 0.926 s | 0.506 s | **1.83x** | 10.80x |
| fib | 0.147 s | 0.047 s | 0.048 s | **0.98x** | 3.06x |
| matmul | 2.057 s | 0.302 s | 0.132 s | **2.29x** | 15.61x |
| sieve | 1.237 s | 0.291 s | 0.120 s | **2.41x** | 10.28x |
| statemachine | 1.265 s | 0.311 s | 0.238 s | **1.30x** | 5.31x |

**Median `dev-fast`: 2.06x slower than `release-fast`** -- target reached.
For comparison: **`dev` lies at 10.54x**, that is, in the region of Rust's debug
builds. The entire difference between 10.5x and 2.1x comes from passes that do
**not** destroy the debug picture. That is exactly the thesis of this section,
and it stands up to a measurement.

### Consequence for the compiler TODAY

* **Every optimization pass has to be switchable on and off individually** and
  has to carry a label: *debug-preserving* or not. If the passes are wired into
  a fixed chain, this stage cannot be built later.
  **That is the only real foundation requirement of this item -- and it costs
  almost nothing today.**
  **Implemented on 2026-08-14** (`compiler/src/opt.rs`): a registry `PASSES`
  with name, scope, label and description; `--list-passes` prints it,
  `--no-pass=<name>` switches one off individually, `--opt-level=` chooses the
  stage. Of the nine passes exactly one is **not** debug-preserving: `inline`.
* **A side finding -- and the strongest argument for this stage:** the new
  `dev-fast` run over the test suite immediately uncovered a **real code
  generator bug** that 259 green tests had not found. `r8`/`r9` are both
  argument registers 5/6 **and** working registers of the allocator
  (`TEMP_REGS`); the prologue moved them one after another and destroyed the
  arguments 5 and 6 that had not been read yet. `tests/024_six_args.fi`
  returned **13 instead of 21** without inlining. It only became visible because
  `--dev-fast` does not inline -- with inlining the faulty function always
  disappeared. Fixed by a parallel register permutation
  (`regalloc.rs: parallele_reg_bewegungen`) that breaks cycles through `rax`;
  the same class of bug existed at the call site and at `syscall` and is fixed
  there as well. Regression test: `tests/025_argreg_shuffle.fi`.
* **Line information has to survive every pass.** Firn already has
  `.debug_line` (`docs/DEBUGGER.md`); the passes have to carry the mapping
  along instead of losing it. Retrofitting means touching every pass.
* Firn today has only `--no-opt` and "full". The four-way split comes in phase 3.

### Priority and phase

**Foundation: the pass architecture** (individually switchable, with a label,
preserving line information) -- **done on 2026-08-14**.
The four stages exist as switches and have been measured. What stays open is
building the forbidden passes from the list (aggressive inlining, unrolling,
variable merging) in the first place -- they do not exist yet. And
`--release-safe` is identical with `--release-fast` today, because there are no
run-time checks yet. **Phase 3.**

---

## 6. In-place initialization

### Problem

Rust builds a value on the stack first and then copies it to its destination.
The optimizer removes the copy most of the time -- but **most of the time** is
not a guarantee. Concretely:

```rust
// Rust: builds 8 MB on the stack, THEN into the Box. Stack overflow.
let b = Box::new([0u8; 8 * 1024 * 1024]);
```

`Box::new_uninit` exists, but it is inconvenient and `unsafe`. For
Rust-for-Linux this was a real blocker: kernel structures contain self-
referential parts (linked lists, locks with an address dependency) and are too
large for the kernel stack (typically 8-16 KB). The answer was the library
**`pin-init`**: `#[pin_data]`, `pin_init!`, `try_pin_init!` -- macros that
produce an *in-place constructor* as a value. From the documentation: *"[it]
allows in-place initialization of big structs that would otherwise produce a
stack overflow."*

That is a **crutch** -- brilliantly done, but a library that rebuilds a missing
language capability, with macros, a trait zoo of its own and a learning curve.

### State of the art

| Language | Behaviour |
|---|---|
| Rust | a copy plus hope in the optimizer; `pin-init` as a library |
| C++ | placement `new` and guaranteed copy elision since C++17 -- solves it, but only for return values |
| Zig | **result location semantics**: `var x: T = f();` gives `f` the address of `x`; it is built there directly. A language guarantee, not a macro |
| C | by hand with pointers; it works, it is unsafe |

Zig's solution is the right one: **the destination is part of the call
semantics**, not an optimization.

### Firn's approach

**Guaranteed construction at the destination as a language rule** -- Zig's
result location semantics, combined with Firn's fallible allocation (section 2).

1. **The result location rule:** with `let x: T = expression` and
   `return expression` the producing expression knows the destination address
   and writes there directly. For struct literals, array literals and function
   returns that is a **guarantee**, not an optimization -- it holds in `--dev`
   as well (section 5).
2. **An `init` expression for fallible construction at the destination:**

```firn
fn neuer_puffer(alloc: inout Allocator, n: usize) -> AllocError!Gross {
    // 'init at' builds directly in the freshly claimed memory, never on the stack:
    return try alloc.new_init(Gross, init {
        kopf:  try Kopf.neu(inout alloc),   // fallible in the middle of construction
        daten: [0; 8 * 1024 * 1024],        // 8 MB, straight at the destination
        ende:  0xDEAD,
    })
}
```

3. **Cleaning up after a partial failure is the compiler's work, not
   handwork.** If `Kopf.neu` fails, the compiler releases the fields already
   completed in reverse order and passes the error on. That is exactly the hard
   part that `try_pin_init!` models by hand in Linux.
4. **No `Pin`.** Firn does not need Rust's `Pin`, because values cannot be moved
   behind your back: moves are statically visible (`SPEC.md` 3.3) and `Gc[T]` is
   **never** moved because of the conservative stack scan (3.5.3).
   Self-referential structures stay `unsafe` all the same -- but they need no
   type zoo of their own.
5. **`#[no_move]`** marks types that may not be moved any more after
   construction (locks, list nodes, DMA buffers). A move is then a compile error
   instead of a run-time error.

### Conflict of goals

* **The code generator gets more complicated.** Result location semantics means
  that every expression is handed an optional destination pointer and that the
  calling convention for aggregate returns (System V `MEMORY` class, hidden
  pointer in `rdi`) is served correctly. That is real work in lowering.
* **Collision with section 2:** fallible construction at the destination needs
  the cleanup logic from item 3. Without it, it is unsafe; with it, it is
  compiler work.
* **Collision with section 8 (SoA):** a value whose fields lie in separate
  arrays has no single "destination". For SoA collections the guarantee holds
  field by field, not as a whole.

### Consequence for the compiler TODAY

* **The aggregate return has to go through the hidden pointer, not through a
  copy.** **Checked on 2026-08-14 -- that is already the case:**
  `compiler/src/abi.rs:66 ret_needs_sret()` classifies returns above 16 bytes as
  `MEMORY` and passes the hidden pointer in `rdi`; and
  `compiler/src/lower.rs:604` passes the **destination address** through:

  ```rust
  let target = if ret_agg {
      match dest {
          Some(d) => Some(d),                         // <- straight to the destination
          None    => Some(self.alloca(size, align)),  // an intermediate only without a destination
      }
  } else { None };
  ```

  So result location semantics for aggregate returns is **already present**,
  without ever having been stated as a language guarantee. The cornerstone is
  laid.
* **What is still missing:** (a) writing the guarantee **down** in `SPEC.md`, so
  that it does not get lost by accident; (b) extending it to struct and array
  literals as well as to `init` expressions; (c) a test showing that an 8 MB
  array does **not** go over the stack (check `--emit=asm`: no
  `sub rsp, 8388608`).
* **The IR has to be able to know a destination operand.** In FIR that is an
  additional operand on `call` and on aggregate construction. Retrofitting it
  later means touching every lowering path.
* This is the **most expensive foundation requirement** of this document after
  section 1.

### Priority and phase

**FOUNDATION -- but half of it is already done.** The result location for
aggregate returns: **present** (checked). Writing it down as a guarantee in
`SPEC.md` and extending it to literals: **phase 2**. The `init` expression with
partial cleanup and `#[no_move]`: **phase 3**, together with `drop` and the move
checker.

---

## 7. Metaprogramming at compile time

### Problem

* **Rust's `proc-macro`s are programs of their own.** They are compiled as a
  separate crate, run as a process and work on a token stream -- **without type
  information**. That is why `serde` needs a complete derivation apparatus of
  its own and `syn`/`quote` as a parser. Build time suffers massively: `syn` +
  `serde_derive` are the largest single items in many projects. And because
  macros run arbitrary code during the build, they are at the same time the hole
  from section 3.
* **C++ only gets reflection with C++26** (P2996, voted in in June 2025) -- with
  `^^` as the reflection operator and `[: ... :]` for splicing it back in. Until
  then there were 30 years of nothing but template metaprogramming and
  preprocessor tricks. Complete code generation (token injection, P3294) only
  comes after that.
* **For a browser this is not a comfort but a duty:** Ladybird has
  **697 `.idl` files**, Blink 2,235. On top of that 2,231 HTML entities, CSS
  property tables, Unicode tables from the UCD, CLDR data. That is hundreds of
  thousands of lines of **generated** code. Without a good mechanism you build
  an external tool chain in Python next to it -- and maintain it forever.

### State of the art

| Approach | Type information? | Build time | Safety |
|---|---|---|---|
| the C preprocessor | no | fast | none |
| C++ templates | partly | very slow | -- |
| C++26 `^^`/`[::]` | **yes** | good | good |
| Rust `proc-macro` | **no** (tokens only) | **poor** | arbitrary code |
| Rust `build.rs` | no | medium | arbitrary code |
| **Zig `comptime`** | **yes** | good | restricted |

Zig's model is the best one available: **the same language, only executed
earlier**, with `type` as an ordinary value.

### Firn's approach

**Two levels, clearly separated** -- `comptime` for everything inside the
program, build scripts only for external data.

**(a) `comptime` -- execution inside the compiler.**
Already fixed in `SPEC.md` 6.1: `comptime` parameters, `type` as a value,
monomorphization, `comptime if` instead of `#ifdef`. It is implemented as an
**interpreter over FIR** -- not over the AST. The reason: FIR is already typed
and desugared, the interpreter stays small, and `comptime` code goes through the
same type checking as run-time code.

**(b) Reflection over types** -- what Rust lacks and C++ lacked for 30 years:

```firn
// A Web IDL binding without an external tool:
fn erzeuge_getter[comptime T: type]() {
    comptime for feld in reflect.fields(T) {
        comptime if feld.hat_attribut("idl") {
            emit fn @[feld.name]() -> feld.typ { return self.@[feld.name] }
        }
    }
}
```

* `reflect.fields(T)`, `reflect.variants(T)`, `reflect.methods(T)` and
  `reflect.attributes(T)` deliver **typed** data at compile time -- not tokens.
  That is the decisive difference from `proc-macro`.
* `emit` inserts generated elements; `@[expression]` forms a name from a
  compile-time string. Deliberately kept narrow: **no arbitrary token stream**,
  only well-formed elements. That keeps the parser simple and the error messages
  usable.
* Generated code is **readable and can be written out** (`--emit=generated`) and
  keeps its relation to the source through `#line` (`G2`), so that the debugger
  shows something sensible.

**(c) Build scripts** (`SPEC.md` 6.4) remain for what `comptime` cannot do:
reading external files (UCD, CLDR, `.idl`, HTML entities). They run in the
sandbox from section 3 -- **no network, write permissions only in the output
directory**. For large tables the library provides perfect hashing and
compressed tries (`G4`).

**A limit drawn deliberately:** `comptime` may do **no** I/O. No file access, no
network, no process start. Otherwise it is the same security hole as
`proc-macro` and `build.rs`. Whoever needs files takes a build script -- and
that stands in `firn.toml` and is visible.

### Conflict of goals

* **Compile time.** A `comptime` interpreter that computes whole tables costs
  build time. The remedies: caching the results (`W10`), an evaluation budget
  (`--comptime-budget`) with a clear error message instead of hanging.
* **Error messages in generic code** stay worse than with real traits -- that is
  already in `SPEC.md` 6.1 and reflection does not make it better.
* **`emit` raises the complexity of the compiler considerably.** Inserting
  elements at compile time means that name resolution and type checking have to
  run more than once. That is why it is phase 3 and not phase 2.

### Consequence for the compiler TODAY

* **The compilation phases have to be re-entrant.** If name resolution and type
  checking are built as a single pass over a fixed AST, `emit` cannot be
  retrofitted later.
  **Done on 2026-08-14** (`compiler/src/sema.rs`): `Checker::add_items` checks
  **additional** declarations with the state that has already been built up --
  the same name table, the same type table, the same diagnostics. The table of
  expression types grows along with the new expression ids; the whole-program
  check (`main` present and correct) still runs exactly once and not once per
  addition.
* **Three tests demonstrate it**, because a capability without a producer would
  otherwise be only a claim: (a) a function that did not yet exist in the first
  pass calls a function from the first pass and is typed correctly; (b) an
  addition with an unknown name produces **the same** error as in the first
  pass -- an addition is not a back door; (c) an addition that declares `main` a
  second time is detected as a duplicate declaration.
* **The honest scope:** additions may contain structs, functions and constants.
  Enumerations are laid out only in the first pass, because their registration
  happens in the parser -- `enum`s generated later come with `comptime` itself.
* **FIR has to stay interpretable** -- that is, no instruction that only makes
  sense in the code generator. That holds today and has to stay that way.
* Firn today has monomorphization (`mono.rs`, `sema_generic.rs`) -- half the
  battle. `comptime` itself does not exist.

### Priority and phase

**Foundation: the re-entrancy of the checking phases** (phase 2, cheap if you
know about it). The `comptime` interpreter and reflection: **phase 3** -- they
are the precondition for acceptance item 6 (the UCD table) and for every Web IDL
binding.

---

## 8. Control over data layout -- AoS against SoA

### Problem

Languages practically always force an **array of structures** (AoS):

```
[{x,y,z,farbe}, {x,y,z,farbe}, {x,y,z,farbe}, ...]
```

The cache often wants a **structure of arrays** (SoA):

```
xs: [x,x,x,...]  ys: [y,y,y,...]  zs: [...]  farben: [...]
```

Whoever reads only `x` loads the whole structure into the cache with AoS -- with
a 64 byte cache line and a 32 byte structure, 75 % of the bandwidth is wasted.
With SoA the line is full of useful data. Factors of 2-5x on memory-bound loops
are normal.

**Directly relevant for this project:**
* **The rasterizer:** edge lists, scanline active lists -- pure number loops.
* **DOM nodes:** they are created by the million. Layout passes often read only
  a single field across all nodes (for example "all with `display: none`").
  `P7` from the browser requirements says so explicitly: *"A DOM node is created
  by the million. Every superfluous byte costs cache."*
* **The layout tree, style values, glyph lists** -- the same pattern.

The usual answer is to write SoA by hand: six parallel arrays and `u32` indices
instead of pointers. That works, but the code becomes unreadable and every
change to the structure has to be followed through in six places.

### State of the art

* **Zig's `std.MultiArrayList(T)`:** produces SoA automatically from an ordinary
  struct definition -- a single allocation, split into field arrays, the fields
  sorted by descending alignment, so **no padding byte between the fields**.
  Access through `items(.field)` or `get(i)`. That is the best approach a system
  language has so far -- but it is a **library type**, not a language feature,
  and access looks different from `ArrayList`.
* **Jai (Jonathan Blow):** `using` plus an SoA switch on the type -- the most
  radical version, but the language is not publicly stable.
* **Rust:** only by hand or through crates (`soa_derive`, `soa-rs`) with macros.
* **C++:** by hand; `std::experimental::simd` only helps with vectors.

### Firn's approach

**The layout is a property of the *collection*, not of the type -- and access
looks identical.**

```firn
struct Knoten {
    eltern:  u32,
    erstes:  u32,
    naechst: u32,
    flags:   u16,
    tag:     Atom,
}

// The same structure, two layouts. The code above it does NOT change:
var baum:  Vec[Knoten]      = ...  // AoS  -- the default
var baum2: SoaVec[Knoten]   = ...  // SoA  -- one array per field

// Access is the same in both cases:
baum2[i].flags = baum2[i].flags | SICHTBAR

// A field-wise pass only with SoA -- and then optimal for bandwidth:
for f in baum2.spalte(.flags) { ... }
```

1. **`SoaVec[T]` is generated by the compiler**, not by a macro library: it
   knows the field layout of `T` anyway and generates the column arrays, the
   accessors and the alignment sorting. That is exactly `MultiArrayList`, only
   without the break in usage.
2. **`baum2[i]` does not return a pointer to a `Knoten` structure** (which does
   not physically exist), but a **view value** -- a compiler-generated bundle of
   field references. Because Firn's references are second class
   (`SPEC.md` 3.3), that is unproblematic: the view cannot leave its call frame
   anyway. **A stroke of luck of the memory model** -- in Rust it would take
   lifetimes and a type of its own.
3. **`#[layout(soa)]`** can alternatively stand on the type, if *every*
   collection of that type is supposed to be SoA.
4. **Complementary layout means** (some of them already in `SPEC.md` 13):
   `#[packed]`, `#[align(n)]`, a fixed field order, `#[bitfeld]` for flag words,
   and -- important for DOM nodes -- `#[klein(N)]` for collections with inline
   storage (`B1`: "small vectors with inline storage").
5. **Switchable for measuring:** `SoaVec` and `Vec` are interchangeable, so the
   question "does SoA help here?" can be measured by changing **one word**
   instead of by rewriting a module. That is the main practical gain.

### Conflict of goals

* **There are no pointers to elements.** With SoA there is no contiguous element
  that one could point at. Whoever needs a `*Knoten` cannot use `SoaVec`. That
  effectively rules SoA out for `Gc[T]`-managed DOM nodes -- a **relevant
  restriction**, named honestly: SoA is for the rasterizer, the layout tree and
  edge lists, not for the GC heap.
* **Against section 4 (a stable ABI):** an SoA layout is by definition not
  freezable.
* **Compiler effort:** view values and generated column types are real work in
  the type checker.
* **Mismeasurement threatens:** SoA is not always better. Whoever reads all the
  fields of an element is faster with AoS. Which is why AoS is the default.

### Consequence for the compiler TODAY

* **Field access has to be separated from the storage location.** As long as
  `a.b` firmly means "base address plus offset", SoA cannot be retrofitted.
  **Done on 2026-08-14** (`compiler/src/layout.rs`): every field and element
  access in lowering goes through **four** accessors -- `field_addr` (a named
  field), `field_addr_at` (a known offset, for enumeration payloads),
  `elem_addr_const` and `elem_addr`. All the places in `lower.rs` and
  `lower_match.rs` were converted. Introducing a second arrangement now means:
  add a case distinction **in this one module**.
* **And the rule is enforced, not merely written down.**
  `tools/schichten/run.sh` (section 7 of `test.sh`) checks that `Op::PtrAdd`
  outside `layout.rs` is only built in the one helper function `ptradd_const`,
  that the direct calls to it are exclusively aggregate hand-overs marked
  `// ABI-Wortkopie` (not field accesses), and that no field offset is computed
  into an address by hand in lowering any more. **Counter-checked:** a
  deliberately introduced violation is detected and reported with file and line.
* **Layout computation has to be central**, not spread across the code generator
  and sema. Today it lies in `types.rs`/`abi.rs` -- that is the right place and
  has to stay that way.
* Otherwise nothing. It is a library-plus-lowering topic, not a syntax topic.

### Priority and phase

**Foundation: the separation of field access and storage location in lowering**
-- **done on 2026-08-14**, together with the architecture guard in the test
suite. The implementation of `SoaVec[T]` and `#[layout(soa)]`: **phase 3/4**,
when the rasterizer and the layout tree come into being and one can measure.
Then it is an extension of `layout.rs` and of a collection type -- not a rebuild
of lowering.

---

## 9. Hot reload -- swapping code without restarting

*(a question from Justin)*

### The problem and the appeal

When developing games, user interfaces and browsers, the cycle
"change -> build -> start -> navigate back to the bug" is the biggest time
eater. With a browser, "back to the bug" means: reload the page, log in, scroll
to the right element, restore the state. Whoever wants to correct a layout rule
by two pixels pays the full price every time.

Where it works, the effect is large: Erlang/Elixir swap modules in a running
system (they were built for it, with process isolation and immutable data).
Flutter's "hot reload" is a selling point of the platform. Game engines (Unreal
Live Coding, Unity domain reload, Jai's `#run` environment) have all retrofitted
it, because the need is real.

### What is technically needed for it -- completely

This is often underestimated. There are **four** requirements, and every single
one collides with another goal of this document:

1. **A separation of code and state.** The code that is swapped may not own any
   data. Everything that is supposed to survive has to lie in a state block that
   the new version finds again. In practice that means: global variables and
   function pointers in data structures are forbidden or have to be versioned.
2. **Stable symbol resolution.** After the swap the caller has to reach the
   *new* function. So either an indirection table on every cross-module call
   (costs performance, prevents inlining) or rebinding all the call sites at run
   time (complicated and platform dependent).
3. **State migration.** If the layout of a structure changes, the old data are
   interpreted wrongly. Either you forbid layout changes when reloading (then it
   is only "half" a hot reload) or you need a migration function per type --
   which somebody has to write.
4. **Dynamic loading.** You need loadable units (`.so`-like), a loader,
   relocations -- exactly what Firn and Osum explicitly **abolished**
   (`R5`: link statically, no `dlopen`; `SPEC.md` 4.5: "no dynamic libraries").

**The collision finding is unambiguous:** item 2 demands a **stable ABI**
(section 4) or indirection everywhere; item 4 demands **dynamic loading**, which
was deliberately removed from the Osum design; and both stand against the
performance target of <= 2x Rust (10.3 of the SPEC), because indirection at
module boundaries prevents exactly the inlining that `P1` demands.

### Firn's approach -- in stages, and the first step is the most important

**Stage A (the route that is actually right): a fast compiler plus a fast
restart.** If a full build of the engine stays under ten seconds and the program
state is serializable anyway (which a browser needs: session restore, tab
restore), then "restart and load the state" is **almost as fast as a hot reload
-- and always correct.** There are no half-migrated states, no phantom bugs from
old data layouts, no doubt whether a bug is real or comes from the reload. `W9`
demands incremental compilation anyway.

**Stage B (cheap, high benefit): reload data instead of code.** By far the
largest part of the need for iteration concerns no code at all: CSS rules,
layout parameters, colours, constants, shaders, tables. Moving those out as data
and re-reading them on change costs **no** language capability and solves an
estimated 80 % of the problem. For a browser that is the normal case anyway --
it already loads stylesheets at run time.

**Stage C (a real hot reload, only if stages A and B are not enough):**
tightly bounded, not general:
* only for modules marked explicitly: `#[hot]`
* `#[hot]` modules may hold **no** state of their own -- the compiler checks
  that (no global variables, no `static` data)
* calls **into** a `#[hot]` module go through an indirection table; all other
  calls stay direct and inlinable. That way only whoever orders it pays
  (guiding principle 4)
* layout changes to types that cross module boundaries are **rejected** on
  reload -- with a clear message "restart needed" instead of silent data
  corruption
* only in `--dev`/`--dev-fast`, **never** in shipping builds

### An honest assessment -- is it worth it?

**No, not as a language capability, and not in the next few years.**

The reasons:

* The **cost/benefit ratio is bad**: stage C demands dynamic loading, a stable
  ABI and a discipline about state -- three large building sites -- for an
  advantage that stages A and B deliver to ~80 % without any language change.
* It **collides directly** with two decisions that were taken for good reasons:
  static linking (`R5`, which saves Osum the entire loader) and inlining
  across module boundaries (`P1`, needed for <= 2x Rust).
* The languages where hot reload really works well (Erlang, Elixir) have
  **paid dearly** for it: immutable data, process isolation, message passing
  instead of shared memory, a heavyweight runtime. That is the opposite of a
  system language for a kernel.
* Where it has been retrofitted (Unreal, Unity), it is notorious for strange
  bugs after the reload -- in case of doubt developers restart anyway.

**What is done instead:** treating build time as a first-class goal (`W9`,
`W10`) and using stage B consistently. **What is not done:** anything in the
language design that makes hot reload *impossible* later -- the door stays open
through `#[hot]`, because the concept "a module with an indirection table" can
be retrofitted at any time, as soon as there is a module system with a symbol
scheme (section 4).

### Priority and phase

**CAN BE RETROFITTED, low priority.** Stage B (reloading data): phase 4, free of
charge. Stage A (a fast build): a perennial topic from phase 3 on. Stage C:
**no date**, only if A and B demonstrably do not suffice.

---

## 10. Prioritization -- what MUST go into the foundation now

**This is the most important section of this document.** Justin does not want
everything at once -- he wants nothing foreclosed. The dividing line runs
between things that concern the *foundation* (type system, IR, lowering,
irreversible rules) and things that come on top later.

### 10.1 The table

| # | Topic | What has to go into the foundation NOW | What comes later | Level | Phase |
|---|---|---|---|---|---|
| 1 | **Function colours / `Io`** | **Do not introduce an `async` keyword.** Codegen may not assume stack continuity (stack switching has to stay possible) | the `Io` interface, `Future`, `Io.Threaded`, `Io.Evented` | **FOUNDATION** (cheap) | 3-4 |
| 2 | **Fallible allocation** | `#[must_consume]` **done 2026-08-14** (attribute system + check); `!T` is still missing. **The rule: no infallible allocation function, ever** | `!T`, the `Allocator` interface, collections, fallible GC allocation | **FOUNDATION** (irreversible) | 2-3 |
| 3 | **Capability modules** | **The rule: no ambient authority in the library.** The module system has to know a *package* boundary | declaration in `firn.toml`, checking, the build script sandbox | **FOUNDATION** (as a rule) | 3 |
| 4 | **A stable ABI** | **done 2026-08-14:** the symbol scheme `_F0....` with a version slot (`modules.rs`), proof `tools/symbole/run.sh` | `#[abi_stable]`, `#[frozen]`, resilient calls | groundwork done, the rest can be retrofitted | done -> 7/8 |
| 5 | **The speed of debug builds** | **done 2026-08-14:** the registry `PASSES` with labels, `--list-passes`, `--no-pass=`, `--opt-level=`; measured **2.06x** | the forbidden passes do not exist at all yet; `--release-safe` = `--release-fast` as long as there are no run-time checks | **FOUNDATION** done | done / 3 |
| 6 | **In-place initialization** | **Write the result location down as a guarantee** -- already implemented for aggregate returns (`lower.rs:604`, checked), missing for literals and `init` | the `init` expression with partial cleanup, `#[no_move]` | **FOUNDATION** (expensive, but cheapest now) | 2 -> 3 |
| 7 | **Comptime + reflection** | **done 2026-08-14:** `Checker::add_items` + 3 tests; FIR stays interpretable | the `comptime` interpreter, `reflect.*`, `emit`, build scripts | **FOUNDATION** done | done / 3 |
| 8 | **Data layout / SoA** | **done 2026-08-14:** `layout.rs` with four accessors, the architecture guard `tools/schichten/run.sh` in `test.sh` | `SoaVec[T]`, `#[layout(soa)]`, `#[bitfeld]`, `#[klein(N)]` | **FOUNDATION** done | done / 3-4 |
| 9 | **Hot reload** | **nothing** -- only do not rule it out | stage B (reloading data), possibly `#[hot]` | can be retrofitted | 4 / no date |
| -- | *(already decided)* opt-in GC, WTF-16, constant time, unwinding | see `SPEC.md` 3, 8, 9, 5.3 | -- | **FOUNDATION** | 2-4 |

### 10.2 The short version

**Six things have to go into the foundation now** -- and five of them cost
almost nothing today, because they are architectural decisions and not features:

1. **No `async` keyword.** (item 1 -- costs nothing, saves everything)
2. **The error union `!T` plus the rule that every allocation is fallible.**
   (item 2 -- demonstrably not retrofittable, see Linux)
3. **No ambient authority in the standard library.**
   (item 3 -- pure discipline, but irreversible)
4. **Optimization passes individually switchable, with a label, preserving line
   information.** (item 5 -- cheap today, later a rebuild of every pass)
5. ~~**Checking phases re-entrant, FIR interpretable.**~~
   (item 7 -- **done on 2026-08-14**)
6. ~~**Field access separated from the storage location** (item 8) **and the
   result location as a guarantee** (item 6).~~ **Both done on 2026-08-14.** The
   result location was already present for aggregate returns and is now written
   down as a guarantee and secured with `tools/result_location/run.sh`; the
   separation of field access sits in `compiler/src/layout.rs` and is enforced
   by `tools/schichten/run.sh`. It was worth doing that while `lower.rs` has
   1,500 lines and not 15,000 -- the conversion came to about 30 lines.

**Four things can wait** -- they are additive:

* A stable ABI (item 4) -- needs only a symbol scheme as groundwork
* Hot reload (item 9) -- needs nothing at all, probably never worth it
* `comptime`/reflection as a *feature* (item 7b) -- only the architecture is
  foundation, the interpreter itself is not
* SoA as a *library* (item 8b) -- only the separation in lowering is foundation

### 10.3 The conflicts of goals at a glance

Where these goals hurt each other -- completely, so that nobody is surprised
later:

| Conflict | The resolution in Firn |
|---|---|
| A stable ABI (4) <-> performance <= 2x Rust | ABI stability is **opt-in** per interface; full freedom everywhere else |
| A stable ABI (4) <-> SoA layout (8) | An SoA type cannot be frozen -- mutually exclusive, documented |
| Hot reload (9) <-> static linking (`R5`) + inlining (`P1`) | Hot reload is **not** built; stages A/B instead of stage C |
| Constant time (`SPEC` 9) <-> an aggressive optimizer | `secret[T]` as a mark down into the IR; `#[constant_time]` checks in the code generator |
| The opt-in GC (`SPEC` 3.5) <-> hot paths | `#[no_gc]` checked transitively; no barrier outside `Gc[T]` fields |
| The opt-in GC <-> SoA (8) | GC objects need a contiguous location -> **no SoA for `gc class`** |
| Fallible allocation (2) <-> ergonomics | `try` as one word + capacity up front + arenas |
| Fallible allocation (2) <-> in-place init (6) | The compiler cleans up a partial construction itself -- that is the hard part |
| Passing `Io` through (1) <-> ergonomics | Pass `Io` and `Allocator` together in a `Ctx` |
| Debug stages (5) <-> maintenance effort | `--dev-fast` is a **subset** of the release passes, not a second chain |
| Comptime `emit` (7) <-> compiler complexity | Only well-formed elements, **no** free token stream |
| Second-class references (`SPEC` 3.3) <-> SoA view values | **No conflict -- a stroke of luck:** views cannot leave the frame anyway |

### 10.4 What follows from this for the next build round

Concrete and verifiable, in this order:

1. **Write the result location down and extend it** (item 6) -- the aggregate
   return already writes straight to the destination (`lower.rs:604`, checked
   2026-08-14). To do: take it into `SPEC.md` as a **guarantee**, extend it to
   struct and array literals and to `init`, and build the 8 MB test.
   *Considerably cheaper than feared.*
2. ~~**Separate field access from the storage location** (item 8)~~ --
   **done**, `compiler/src/layout.rs` + the guard.
3. **`!T`** (item 2) -- `#[must_consume]` is **done**, the error union itself is
   still outstanding. It can build on the existing enumeration machinery
   (`E!T` as a two-variant tagged union), which makes it considerably cheaper
   than feared.
4. ~~**A pass registry with labels** (item 5)~~ -- **done**,
   `--list-passes` / `--no-pass=` / `--opt-level=`, measured 2.06x.
5. ~~**Re-entrant checking phases** (item 7)~~ -- **done**,
   `Checker::add_items` with three tests.
6. ~~**A symbol naming scheme with a version slot** (item 4)~~ -- **done**,
   `modules::symbol`, proof `tools/symbole/run.sh`.

---

## Sources

* Zig 0.16.0 release notes -- `std.Io`, `io.async`/`io.concurrent`,
  `Io.Threaded`, `Io.Evented`, io_uring/Kqueue/Dispatch:
  <https://ziglang.org/download/0.16.0/release-notes.html>
* Zig 0.15.1 release notes -- the I/O rebuild ("Writergate"), buffered
  readers/writers: <https://ziglang.org/download/0.15.1/release-notes.html>
* Zig `std.MultiArrayList` (SoA, sorting by alignment):
  <https://github.com/ziglang/zig/blob/master/lib/std/multi_array_list.zig>
* Swift -- ABI Stability and More (Swift 5, Apple platforms):
  <https://www.swift.org/blog/abi-stability-and-more/>
* Swift -- Library Evolution (resilience, `@inlinable`, `@frozen`, the cost of
  indirect field access): <https://www.swift.org/blog/library-evolution/>
* Swift -- ABI Stability Manifesto:
  <https://github.com/apple/swift/blob/main/docs/ABIStabilityManifesto.md>
* C++26 reflection P2996 (`^^`, `[: :]`, accepted in June 2025):
  <https://stephenberry.github.io/glaze/p2996-reflection/> -
  <https://www.modernescpp.com/index.php/reflection-in-c26/>
* Rust-for-Linux `pin-init` (in-place initialization, stack overflow with large
  structures, `try_pin_init!`): <https://rust-for-linux.com/pin-init> -
  <https://github.com/Rust-for-Linux/pin-init> -
  <https://rust.docs.kernel.org/kernel/macro.try_pin_init.html>

---

*This document decides directions, not dates. What stands here as "foundation"
has to be taken into account at the next rebuild of the compiler -- everything
else may wait until it hurts.*

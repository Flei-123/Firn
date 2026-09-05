# Firn -- roadmap

**As of:** 2026-08-14 (v0.2) - **Related:** `SPEC.md`, `DESIGN_GOALS.md`, `ACCEPTANCE.md`,
`../osum-browser/FIRN-ANFORDERUNGEN.md`, `../osum-browser/PLAN-FIRN.md`
Time figures = the effort of one person with AI support, not calendar time.

---

## What has changed since v0.1

Since the browser decision (**B1**: every line of executable code in the Osum
browser engine is Firn), Firn is **critical path number 1** of the whole
ecosystem. That changes the roadmap in three places:

* **New mandatory parts:** opt-in GC, WTF-16 strings, constant-time primitives,
  unwinding for JS, compile-time code generation, debugger, package management.
* **New performance target:** <= 2x Rust on microbenchmarks. That moves work from
  language surface into the optimizer.
* **Dropped:** the aarch64 and WASM backends no longer have a date
  (`FIRN-ANFORDERUNGEN.md` 11 needs neither). That helps noticeably.

**Two acid tests decide everything** and have therefore been pulled forward:
the HTML5 tokenizer (100 % html5lib, <= 2x the reference) and a DOM prototype
with cycles (24 h without memory growth). If they fail, it is not the browser
that gets repaired but Firn.

---

## Foundation work from DESIGN_GOALS.md (new, 2026-08-14)

`DESIGN_GOALS.md` examines ten known weak spots of today's languages and
separates what has to go into the foundation **now** (impossible later) from
what can be **added on** afterwards. Six items concern the compiler directly and
are worked into the phases below:

| Foundation item | Why now | Phase |
|---|---|---|
| **No `async` keyword**, codegen without any assumption about stack continuity | the colour could not be removed later; `Io` as a parameter needs stack switching | 2 (rule), 3-4 (implementation) |
| **`!T` + `#[must_consume]`**, the rule "every allocation is fallible" | Rust-for-Linux proves it: cannot be added later | 2 -> 3 |
| **Result-location operand in FIR and lowering** | the most expensive foundation work -- today lowering is ~2,000 lines, later 20,000 | **2** |
| **Separate field access from storage location** (precondition for SoA) | as long as `a.b` firmly means "base + offset", SoA is dead | 2/3 |
| **Optimization passes individually switchable, with a "debug-preserving" label** | otherwise the `--dev-fast` stage later means rebuilding every pass | 2 -> 3 |
| **Re-entrant checking phases**, FIR interpretable | precondition for `comptime`/`emit` | 2 -> 3 |

Addable later and therefore **not** scheduled: a stable ABI (only a symbol
naming scheme as groundwork in phase 3), hot reload (no date, see
`DESIGN_GOALS.md` 9 -- the honest assessment is: not worth it).

---

## How realistic is this?

| Language | First compiler | Version 1.0 / stable | Duration |
|---|---|---|---|
| Rust | 2006 (Graydon, in OCaml) | May 2015 | **9 years** |
| Zig | 2015 | not yet (0.16, 2026) | **11+ years** |
| Go | 2007 | March 2012 | 5 years, with a Google team |
| Odin | 2016 | not stable yet | 10 years |

Firn will not be finished any faster just because an AI helps with the typing.
AI speeds up the typing, not the design decisions and not the finding of the
bugs that only show up at 50,000 lines of real code. The early phases (parser,
type checker, codegen for a subset) shrink from months to days. The late phases
(self-hosting, an optimizer within <= 2x of Rust, a GC in a soak test,
stability) hardly shrink at all.

**Honest expectation:** *usable for small Osum system programs* in 6-12
months. *Self-hosting compiler* in 1-2 years. *Acceptance according to
`FIRN-ANFORDERUNGEN.md` 13 passed* -- that is, ready for the first browser
library -- realistically **2-4 years**. `PLAN-FIRN.md` budgets 27 person-months
for phase F0 alone.

---

## Phase 0 -- specification (done)

* `SPEC.md` v0.1: profiles, ownership model, error handling, `comptime`,
  backend strategy, bootstrap, grammar.
* `SPEC.md` v0.2: memory model in three levels with an **opt-in GC**,
  inheritance for `gc class`, WTF-16, constant time, unwinding, performance
  target, traceability.
* `ACCEPTANCE.md`: the six checkpoints from `FIRN-ANFORDERUNGEN.md` 13 as a
  tickable list.

## Phase 1 -- `firnc0`: prototype in Rust (done)

The subset from `SPEC.md` 14, really all the way to a running binary.
Lexer, parser, type checker, FIR, constant folding + DCE, x86_64 codegen without
LLVM, `syscall`, 75 test programs x 2 runs + 15 negative tests, all green.
**Result:** a compilable language, not yet a tool.

## Phase 2 -- v0.2: language core for browser code `<- we are here`

What the browser demands of the *language core*, without a runtime and without
a library.

* **Sum types + `match`** with an exhaustiveness check, jump tables (`L4`, `P4`)
* **Generics** by monomorphization (`L5`)
* **Strings**: `Bytes`, `Str` (UTF-8), **`Str16` (WTF-16)**, `Atom`,
  correctly rounded `strtod`, shortest double output (`Z1`-`Z6`)
* **Optimizer**: inlining, real register allocation, DCE, removal of bounds
  checks -- with a **measured** comparison against Rust (`P1`-`P3`, `P5`, `P9`)
* **`secret[T]` + `#[constant_time]`**, `secure_zero`, `u128` (`C1`-`C3`, `C5`)
* **Memory model**: `Rc[T]`/`Weak[T]`, `Gc[T]`/`GcWeak[T]`, `gc class`,
  `#[no_gc]` (`S1`-`S3`, `S7`)
* `break`/`continue`, `for`, `defer`, `drop`, move checker, reference types
* **Acid test 1**: HTML5 tokenizer against html5lib
* **Acid test 2**: DOM prototype with cycles in a soak test
* Test runner with machine-readable output (`W2`)
* **Foundation work from `DESIGN_GOALS.md`** -- before everything else:
  * **Result location** (`DESIGN_GOALS.md` 6): an aggregate return writes
    straight to the destination, destination operand in FIR. First check what
    `compiler/src/abi.rs` does today
  * **Separate field access <-> storage location** (8): an intermediate layer in
    lowering instead of a hard-wired "base + offset"
  * **Pass registry** (5): every optimization pass gets a name, a switch and a
    label *debug-preserving yes/no*; line information survives every pass
  * **Re-entrant checking phases** (7): "check this newly created function" has
    to be possible
  * **Write down the rules**: no `async` keyword, no infallible allocation
    function, no ambient authority in the library
* **Effort:** months, not weeks. This is the real chunk of work.

**Interim status 2026-08-13 (round 2 merged), honestly:**

| Item from the list above | State |
|---|---|
| Sum types + `match` + jump tables | **done and verified** |
| Generics (monomorphization) | **done and verified** |
| Strings `Bytes`/`Str`/`Str16`/`Atom`, `strtod`, shortest output | **done** (without string literals in the lexer) |
| Optimizer + measured comparison against Rust | **done, target missed**: median 2.8x-3.4x instead of <= 2x |
| `secret[T]`, `#[constant_time]`, `u128` | **not started** |
| `Rc`/`Gc`/`gc class`/`#[no_gc]` | **not started** |
| `break`/`continue`, `for` | done; `defer`, `drop`, move checker, reference types: not started |
| Acid test 1 (HTML5 tokenizer) | **not started -- 0 of 6,810 cases** |
| Acid test 2 (DOM soak test) | **not started** |
| Test runner with machine-readable output (`W2`) | **done** (`tools/testrunner`, JSON) |

Pulled forward out of phase 3, because no tokenizer can be written without them:
the **module system** (`import`/`export`) and `.debug_line` for `gdb`.
The numbers and commands are in `ACCEPTANCE.md`, the reproduction in `RUN.md`.

## Phase 3 -- v0.3: modules, `comptime`, standard library

* Module system, `import`, `export` lists, separate compilation
* `comptime` evaluation (an interpreter over FIR), `interface` static + dynamic
* **Compile-time code generation** (`G1`-`G4`): build scripts, perfect hashing,
  compressed tries -- acceptance: a Unicode table from the UCD
* Standard library `B1`-`B11`: collections, I/O, time, formatting, sorting,
  randomness (CSPRNG separated from the fast generator)
* Concurrency `N1`-`N4`: threads, atomics, mutex/condvar/rwlock/channels,
  `#[sendable]`/`#[shareable]`
* **Package management + reproducible builds** (`W1`)
* **DWARF basics + debugger** (`W3`) -- without it every following task takes
  three times as long
* **Stage 1 begins:** lexer and parser are rewritten in Firn
* From `DESIGN_GOALS.md`:
  * **`Io` as a parameter** instead of `async` (1): the `Io` interface,
    `Future[T]` as `#[must_consume]`, `io.async`/`io.concurrent`, `Io.Threaded`
    and `Io.SingleThread` (the latter satisfies `N7`)
  * **Fallible allocation throughout** (2): `Allocator` as a parameter,
    `try v.push(inout a, x)`, `reserve` + `push_within_capacity` for hot paths
  * **Capability declaration in `firn.toml`** (3) + a build-script sandbox
    without network access
  * **Symbol naming scheme with a version slot** (4) -- cheap groundwork for a
    later stable ABI
  * **Four build stages** `--dev` / `--dev-fast` / `--release-safe` /
    `--release-fast` (5); target for `--dev-fast`: at most 2-3x slower than
    release, not 30x
  * **`init` expression** with partial cleanup, `#[no_move]` (6)
  * **`comptime` interpreter over FIR + `reflect.*` + `emit`** (7) --
    precondition for acceptance item 6 (UCD table) and for every Web IDL binding
  * **`SoaVec[T]` / `#[layout(soa)]`**, `#[bitfeld]`, `#[klein(N)]` (8)
* **Effort:** 3-6 months

## Phase 4 -- v0.4/0.5: self-hosting

* `firnc1` compiles `firnc2`, `firnc2` compiles itself, the result is
  bit-identical (fixpoint) -> `L1` and `ACCEPTANCE.md` item 1 satisfied
* Rust becomes a bootstrap archive, `firnc0` is frozen
* **Unwinding/`throw`** (`L8`) with tables in two phases
* Incremental GC with tri-colour marking (`S5`), pause times measurable (`S6`)
* Profiler with flame graphs (`W4`), fuzzing hookup (`W5`)
* `Io.Evented` with stackful coroutines (`DESIGN_GOALS.md` 1)
* Hot reload **level B** -- reload data instead of code (9); free of charge,
  estimated to solve 80 % of the iteration need without any language change
* **Effort:** 6-12 months - **from here on Firn is a real language**

## Phase 5 -- acceptance according to `FIRN-ANFORDERUNGEN.md` 13

All six items from `ACCEPTANCE.md` green. Only after that may block 1 start in
the browser project. **That is the actual goal of this roadmap.**

## Phase 6 -- runtime on Osum (`R1`-`R6`)

* The Firn runtime ported: memory, threads, files, time, I/O
* Separation of runtime <-> platform layer, cross-compiler to Osum in CI
* Firn's own test suite passes **on Osum** (`R6`)
* Runs in parallel with the Osum kernel work (K1-K10)

## Phase 7 -- Osum kernel modules in Firn

* Check the kernel profile against real osum code (ABI, inline assembly, MMIO)
* First Osum module in Firn (candidate: a small, isolated driver)
* Then step-by-step replacement -- **no big rewrite**

## Phase 8 -- v1.0: stability

* Language stability promise, backwards compatibility
* SIMD (`L16`), loop optimization, optional LLVM backend as a yardstick
* Formatter, linter, coverage measurement, compilation cache
* **Several years away at the earliest**

## Without a date (deliberately dropped)

* **aarch64 backend** -- only once Osum targets ARM
* **WASM backend** -- not needed for the browser; "Firn instead of JavaScript in
  the browser" stays a distant goal, but it blocks nothing
* **JIT**, dynamic libraries, C++ interop -- permanently excluded
* **Hot reload level C** (real code swapping) -- `DESIGN_GOALS.md` 9:
  collides with static linking (`R5`) and with inlining across module boundaries
  (`P1`). Honest assessment: not worth it. The door stays open through
  `#[hot]`, no more than that
* **Stable ABI** (`#[abi_stable]`, `#[frozen]`) -- only once Osum needs
  interchangeable system components, phase 7/8. Until then IPC is the better
  route

---

## How this project can fail

Named openly, so that it comes as no surprise:

1. **The optimizer does not reach <= 2x Rust.** That is the biggest single risk.
   A tokenizer runs over every character of every page; 10x too slow means a
   browser 10x too slow, and that cannot be optimized away afterwards.
   Countermeasure: measure early and honestly (`phase 2`), not at the end.
2. **The GC does not carry the DOM.** Conservative stack scanning rules out a
   compacting collector; fragmentation in a 24 h soak test is a real risk.
   Countermeasure: acid test 2 early, size-class allocator.
3. **Stamina.** The most dangerous point is phase 3/4 -- the thrill is gone and
   the work turns tough (error messages, edge cases, regressions).
4. **Self-reference.** A compiler that compiles itself is excellent at hiding
   its own bugs. Countermeasure: the fixpoint check and a test suite that is
   taken seriously.
5. **Three building sites at once.** Osum, Firn *and* the browser is a lot.
   Firn must not slow Osum down -- which is why Rust stays in the kernel
   until Firn demonstrably fits better.
6. **Foundations built shut.** If the foundation work from `DESIGN_GOALS.md`
   10 is skipped, the SoA layout, `comptime` `emit` and the `--dev-fast` stage
   can later only be reached by rebuilding the entire lowering.
   Countermeasure: begin phase 2 with it, do not end phase 2 with it.
7. **The optimizer <-> crypto conflict.** Section 9 of the specification
   resolves it on paper. Whether it holds in the implementation only the
   assembly inspection will show.

---

## Next concrete step

**As of 2026-08-14.** Out of the order given by `FIRN-ANFORDERUNGEN.md` 12
(**foundation work -> memory model -> optimizer/measurement -> language core ->
`comptime` -> package management -> self-hosting**) the following are done:

* **Foundation work** (`DESIGN_GOALS.md` 10.4) -- all six items, see
  `ACCEPTANCE.md`.
* **Memory model** -- an opt-in tracing GC built **and proven in a soak test**:
  100,000,000 DOM cycle sets (700 million objects) at a constant 1,364 KiB RSS,
  while the reference-counting counter-check leaks up to 750,080 KiB.
  `docs/reports/dom.md`. Still open are the 24 hour run and fragmentation with
  changing object sizes.
* **Acid test 1** (HTML5 tokenizer): the pass rate is reached (6,810/6,810),
  **the speed is missed** (5.7x-8.3x on real pages instead of <= 2x).
* **Acid test 2** (DOM soak test): passed, see above.

**Next, in this order:**

1. **The optimizer up to the speed target** -- that is the only target value
   that has been *measurably missed*. Starting points in the order of their
   expected benefit: remove bounds checks, real register allocation across block
   boundaries, jump tables in the tokenizer core, inlining across module
   boundaries. Without <= 2x the language does not carry a browser engine.
2. **Incremental collection** (`S5`) -- a longest pause of 3.54 ms is too much
   against a frame time of 16 ms.
3. **The rest of the language core**: `defer`, `drop`, move checker, reference
   types `&T`/`inout T`, `for`, floating point, string literals.
4. **`comptime` + reflection** -- the precondition (re-entrant checking phases)
   has been in place since the foundation work.
5. **Package management**, then **self-hosting** in three stages.

Constant time is built in along the way, not retrofitted. The foundation work
deliberately came **before** everything else: it was cheap and would not have
been affordable later.
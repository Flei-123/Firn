# Firn

**Firn** is a systems programming language with its own compiler. `firnc`
reads `.fi` source text and writes **real machine code** for **x86-64** and
**aarch64**: lexer → parser → type checker → its own IR (**FIR**) → optimizer
→ its own code generator → `as` → `ld` → a Linux binary **without libc**.

* **No LLVM, no Cranelift, no C as a backend.** The assembly text is produced
  in `compiler/src/codegen_x86.rs` and `compiler/src/codegen_a64.rs`
  themselves; `as` and `ld` are used exclusively as assembler and linker.
* **No external crates.** `compiler/Cargo.toml` has an empty `[dependencies]`
  section; Rust's `std` is enough.
* **No parser generator.** A hand-written lexer and a recursive descent parser
  with error recovery.
* **It compiles itself.** `firnc1`, the compiler written *in Firn*, compiles
  its own source text, and the result is a fixpoint — stage 2 and stage 3 are
  character-identical (`tools/fixpoint.sh`).

Specification: [SPEC.md](SPEC.md) (deviations of the implementation in 14.1).
IR: [docs/FIR.md](docs/FIR.md). Build and **measure** it yourself:
[RUN.md](RUN.md). Every figure below with the command behind it:
[docs/BENCHMARKS.md](docs/BENCHMARKS.md).

---

## Try it

Three commands after the clone. They were run exactly like this on
2026-08-23 to produce the output printed underneath.

```sh
cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
./compiler/target/release/firnc -o /tmp/tour examples/tour.fi && /tmp/tour
```

```
hello, Firn -- dist2 25, dist 5, box 12, sum 10
```

`FIRNLIB` tells the compiler where the standard library lives; a program that
imports nothing does not need it. `ld` prints one warning while linking --
`LOAD segment with RWX permissions` -- because the binary is freestanding and
carries no separate read-only segment yet; it is not an error and the program
runs. `bash test.sh` runs the whole acceptance suite — it builds the compiler,
compiles and runs every program in the tree at three optimisation levels, and
then works through about forty section proofs. It takes a while.

---

## What Firn can do today

Every line here is checked by a script in this repository; the tables behind
the numbers are in [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

| | state | proof |
|---|---|---|
| **Self-hosting** | `firnc1` is written in Firn, compiles itself, **stage 2 == stage 3 character-identical** | `tools/fixpoint.sh`, `tools/self_compare.sh` |
| **Two machines** | x86-64 and aarch64, same source, **304 of 304 comparable programs byte-identical output, 0 differing, 0 unsupported** (both build stages) | `tools/aarch64/run.sh` |
| **Freestanding** | `--target=x86_64-none` and `--target=aarch64-none`: no operating system underneath. Both images **boot in QEMU and print over the serial line**; the x86 build is octet-identical to the plain `profile kernel` build | `tools/freestanding/none.sh` |
| **Language** | structs, arrays, `enum` + `match` with exhaustiveness check, generics, interfaces, closures and function values, error unions `E!T`, `defer`/`errdefer`, `comptime` + `emit`, `f32`/`f64`, `str` with `f"…"` interpolation, threads, `extern fn` in both directions | `tests/` (three build levels each) |
| **Garbage collector** | opt-in, incremental mark-sweep, **longest pause 0.45 ms** at 120,000 live nodes; weak refs, finalizers, `GcVec`/`GcMap` | `tools/dom_soak/run.sh` |
| **Tooling** | formatter, DWARF line info + `gdb`, language server (`firnc --lsp`), package/project system, test runner with JSON output | `tools/fmt`, `tools/dwarf`, `tools/lsp`, `tools/packages` |
| **HTML** | tokenizer written in Firn, **6,810 / 6,810 html5lib cases (100.00 %)**; against html5ever **1.18x** on real pages and **0.80x** (ahead) on the pathological corpus | `tools/tokenizer/run.sh`, `throughput.sh` |
| **CSS + layout** | against Chromium: **1,087 / 1,087 boxes, deviation 0.00 %**, paint order 5,171 / 5,171 probe points; against the **official Web Platform Tests** (`css/`, self-describing layout tests): **97 / 186 (52.15 %)** — Chromium 141 reaches 138 / 186 on the same corpus through the same harness; round B6 added the logical properties, `aspect-ratio`, `margin-trim`, the replaced elements without a picture (`canvas`, `svg`, `video`) and the two-word alignment values | `tools/layout/run.sh`, `tools/layoutb2/run.sh` |
| **Painting** | the browser is VISIBLE: display list in the order of CSS 2.1 Appendix E, own scanline rasteriser with exact anti-aliasing, round corners, eight border styles, gradients, blurred shadows, blend modes, clipping, a TrueType reader with composite glyphs and kerning, PNG in and out. Against the **official WPT reference tests** — two documents that have to look the same — **202 / 541 (37.34 %)**, with the 32 pairs that match only because both sides are empty counted separately and NOT in the quota | `tools/paintb3/run.sh` |
| **A page that is ALIVE** | scripts really change the tree: the DOM bound into the JavaScript engine (`document`, `querySelector`, `createElement`, `innerHTML` through the fragment parsing algorithm, `classList`, `style`, `addEventListener` with all three phases of the event flow, `setTimeout`), `<script>` with the ordering rules of `async` and `defer`, and a per-node dirty mark that makes a change cost **103x less style work** than a full pass -- checked against a full layout after every single mutation, **0 boxes different**. Against the **official Web Platform Tests** for the DOM, through the unmodified `testharness.js`: **390 / 1,714 subtests (22.75 %)**, with the 169 files whose harness never finished counted separately and NEVER as passes | `tools/liveb4/run.sh` |
| **The network** | an HTTP/1.1 client in Firn: GET/POST, chunked transfer, gzip, redirects with the method rules, a cache with revalidation, cookies (RFC 6265) and persistent connections -- measured against Python's own `http.server` over a real socket, **28 / 28 rules, six of them counter-checks**. **`https://` is refused by name**: TLS is a round of its own and is not faked | `tools/liveb4/http_check.py` |
| **TLS** | TLS 1.3 in Firn -- record layer, key schedule, handshake, X.509 with a trust store: **647 / 647** primitive cases against Python's `cryptography` (49 of them counter-checks that MUST fail), **26 / 26** certificate cases (14 refusals, each with the RIGHT reason), **18 / 18** handshakes against `openssl s_server` and the public internet (7 refusals, one of them a man in the middle who flips a single bit), 512 KiB octet for octet | `tools/tlsb5/run.sh` |
| **Fingerprint defence** | chapter Z, on by default and with no switch: the canvas readback and the `navigator` fields are noised **per origin and per session** (Brave's method). Measured: 500 origins give **500 distinct** canvases and 500 sessions **500 distinct**, while twenty reads on one origin are **byte-identical**; the largest deviation of any colour channel is **1** and the alpha channel is never touched. The counter-check -- the same path with the farbling taken out -- gives **1** result over the same 500 origins | `tools/fpz/run.sh` |
| **Fonts** | 408 characters and 469 kerning pairs against fontTools: **0 deviations**; **393 / 393 glyphs** agree pixel for pixel with an independent rasteriser, largest deviation of any single pixel **0.049** | `tools/paintb3/font_check.py` |
| **JavaScript** | test262, **63,364 cases, nothing filtered**: parser **91.94 %**, engine **76.00 %** | `tools/js/run.sh` |
| **Cryptography, compression** | SHA-256, AES, DEFLATE — written in Firn, held against OpenSSL/zlib and the NIST vectors; behind by 1.38x–1.88x | `tools/stdlib81/run.sh`, `tools/bench82/run.sh` |
| **An operating system** | `profile kernel` produces a freestanding object file; `demos/kernel` boots in QEMU with tasks, address spaces, system calls and files | `tools/kernel/run.sh`, `tools/freestanding/run.sh` |
| **A userland** | osum reads `/bin/sh` off a disk and runs it: a shell with `>` `>>` `<` `|` `&` `$?`, a working directory and a line editor, plus **23 standard tools** as separate ELF files -- measured by comparing whole shell transcripts octet for octet | `tools/osum/run.sh`, `tools/userland/run.sh` |
| **Network** | sockets against `nc`/`curl`, NBT identical to Notch's `bigtest.nbt`, and a **Minecraft server a real vanilla client logs into** | `tools/net`, `tools/nbt`, `tools/mcserver` |
| **Speed against Rust** | six microbenchmarks, median **2.08x / 2.19x** of `rustc -O` (two passes of nine runs), range 1.43x–4.16x | `BENCH_RUNS=9 bash bench/run.sh` |

---

## A tour of the language

This is [`examples/tour.fi`](examples/tour.fi) verbatim, minus the
`// expect_exit: 0` line in which the test harness records what it expects. The
file is part of the acceptance suite, so it is compiled and run three times
(with the optimizer, without it, and at `dev-fast`) on every `bash test.sh`.

```firn
import std.io
import std.math

struct Point { x: int, y: int }

const LIMIT: int = 100

interface Area { fn area(*self) -> int }
impl Area for Point { fn area(*self) -> int { return (*self).x * (*self).y } }

fn dist2(p: *Point, q: *Point) -> int {
    let dx = (*p).x - (*q).x
    let dy = (*p).y - (*q).y
    return dx * dx + dy * dy
}

fn main() -> i32 {
    if !gc_init() { return 1 }
    var a: Point = Point { x: 3, y: 4 }
    var b: Point = Point { x: 0, y: 0 }

    let d = dist2(&a, &b)
    let root: double = math.sqrt(d as double)
    let box: int = a.area()

    var arr: [int; 4] = [1, 2, 3, 4]
    var sum = 0
    for i in 0..4 { sum = sum + arr[i as usize] }

    let name: str = "Firn"
    let hello: str = "hello, " + name
    io.fmt_print_line(f"{hello} -- dist2 {d}, dist {root}, box {box}, sum {sum}")

    if d == 25 && root == 5.0 && box == 12 && sum == 10 && sum < LIMIT { return 0 }
    return 1
}
```

Real output:

```
hello, Firn -- dist2 25, dist 5, box 12, sum 10
```

* **`int` is an alias for `i32`, not a second type** — and so are `long` =
  `i64`, `short` = `i16`, `byte` = `u8` (unsigned), `sbyte` = `i8`, `uint`,
  `ulong`, `ushort`, `double` = `f64`. Both spellings pass into each other
  without a cast, and `impl Ord for int` produces the very same method as
  `impl Ord for i32` — if you come from Rust, write `i32`, it is the same type.
  The widths are fixed on every platform (`int` *always* 32 bits, `long`
  *always* 64): the trap of C's `long`, avoided on purpose (SPEC 13,
  `tests/1334_type_aliases.fi`).
* **Type inference where the context says something**: `let dx = …` takes the
  type of the expression, `var sum = 0` falls back to `i32` when nothing says
  otherwise. `let x: i32 = 5` still works and still means the same.
* **No implicit conversions.** That is why the index says `arr[i as usize]` —
  an index is a `usize`, and the compiler will not quietly widen an `i32`; it
  says so with a note: `write e.g. 'a[i as usize]'`.
* **`let` is immutable, `var` is mutable**; parameters behave like `let`.
* **`str` is a language type**, two machine words; `f"…"` interpolates any
  type (integers, `f64`, `bool`, `str`), `+` concatenates, and concatenation
  allocates — which is why `gc_init()` stands at the top. `&&`/`||`
  short-circuit; output without libc also works raw, via `syscall(nr, a1..a6)`.

More: [`hello.fi`](examples/hello.fi) (one `write` syscall, no library at all),
[`fib.fi`](examples/fib.fi), [`structs.fi`](examples/structs.fi),
[`bubblesort.fi`](examples/bubblesort.fi), [`number_check.fi`](demos/number_check.fi).

## What Firn can NOT do — the honest list

State: **2026-08-23**, on `main`. Every entry below
was checked by handing a small program to *this* build of the compiler, not by
reading an old report. Where the compiler refuses, it refuses with a message
and a `line:column` — it does not crash and it does not pretend.

### Not in the language

* **Overflow is checked, the index is not.** `2147483647 + 1` **aborts** in
  `dev`, `dev-fast` and `release-safe` with the file, the line, the column,
  the operator and both operands (`panic: integer overflow in 'i32 + i32' at
  f.fi:3:9 (a=2147483647 b=1)`), and wraps only in `release-fast`, where the
  name says so. `+%` (wrapping) and `+|` (saturating) are there for the cases
  where wrapping is the intent, at every level. What is **not** checked:
  `a[9]` on an `[i32; 4]` reads past the end without a word, and division by
  zero is not caught by the compiler at all — the processor traps it
  (`SIGFPE`, exit 136) and no handler says what happened or where.

* **No global variables.** Only `const`, restricted to scalar integer and
  `bool` expressions that can be evaluated at compile time. `var G: i32 = 7`
  and `static G: i32 = 7` at the top level are both rejected with *"expected
  'fn', 'struct', 'const', 'comptime', 'import', 'export' or 'profile' at top
  level"*. The reasoning for leaving it out — a data section needs an
  initialisation order, a rule for the collector (is a `static Gc[T]` a root?)
  and one for threads — is SPEC 14.1 item 5.
* **No optionals.** `-> ?i32` gives *"expected a type, found '?'"*. Error
  unions `E!T` exist and cover the fallible case; the empty case does not have
  a type of its own.
* **No reference types.** `&T` and `inout T` are not types; stage 0 has raw
  pointers `*T` / `*mut T` only, and `mut` on a pointer is parsed but not
  checked. `fn f(a: &i32)` gives *"expected a type, found '&'"*.
* **No `drop`, no move checker.** `drop` is not a keyword
  (`compiler/src/lexer.rs`, `fn keyword`); there is no destructor that runs by
  itself and nothing stops you from using a value after you handed it on.
  `defer` and `errdefer` **do** exist (`tests/580_defer.fi`,
  `tests/581_errdefer.fi`) and are the tool for cleanup today.
  `#[must_consume]` catches the discarded-result case, and an escape checker
  refuses to let a local's address leave its frame.
* **No `secret[T]`, `u128`, `mul_wide`, `declassify`, `#[constant_time]`**
  (SPEC 9). `fn f(a: secret[u8])` reports *"'secret[T]' is not implemented in
  stage 0"*, `u128` is an *"unknown type"*, `#[constant_time]` reports
  *"attribute 'constant_time' is not implemented in stage 0"*. What **is**
  built are the three primitives `select` (becomes `cmov`, never a branch),
  `barrier` and `secure_zero` (`compiler/src/ct.rs`,
  `tests/430_ct_select.fi`–`tests/433_ct_secure_zero.fi`). Without
  `secret[T]` they are building blocks without a type check behind them.
* **No unwinding / `throw`** (SPEC 5.3). `#[unwinds]` is a known attribute
  that is deliberately rejected — `firnc --list-attrs` lists nine such
  attributes (`unwinds`, `packed`, `align`, `layout`, `no_move`, `abi_stable`,
  `frozen`, `hot`, `constant_time`): known, planned, and refused with a clear
  message instead of being ignored in silence.
* **`match` is a statement, not an expression**, there are no generic `enum`s,
  no alternative patterns `A | B` and no guards, and an `enum` may not sit by
  value inside a `struct` (SPEC 14.1.types, T1–T8).
* **Error unions have no inferred error set** and `catch |e|` binds to an
  expression, not to a block (SPEC 14.1.error_unions, F1–F10).

### Not in the toolchain

* **No WASM.** `--target=wasm32` answers *"unknown target 'wasm32' (allowed:
  x86_64-linux, aarch64-linux, x86_64-none, aarch64-none)"*.
* **No LLVM backend, and there will not be one** — that is the point of the
  project, not a gap. It is listed here because people ask.
* **No self-hosting on ARM.** `firnc0` (the Rust bootstrap) generates
  aarch64; `firnc1` (the compiler written in Firn) does not, and says so:
  *"error: firnc1 cannot generate aarch64 yet"*. What is missing is the A64
  code generator in Firn -- `lib/firnc1/codegen.fi` writes Intel-syntax
  strings. The system call table already exists on both sides and is
  compared entry for entry on every run (`tools/aarch64/syscall_table.sh`).
  docs/ROUND-ARM-FREESTANDING.md section 8.
* **No debug information and no register allocation on aarch64.** `.loc` and
  `.debug_info` are emitted for x86-64 only, and `regalloc.rs` is an x86
  pass -- the A64 backend uses the base path, so its code is correct and
  slow.
* **Aggregates across `extern fn` on aarch64 are untested and should be
  assumed wrong.** `abi.rs` classifies by the System V rules; AAPCS64
  classifies composites differently (homogeneous float aggregates, anything
  above 16 octets by reference). Scalars and up to ten integer / nine
  floating point words are proven against `aarch64-linux-gnu-gcc`
  (`tools/aarch64/machine.sh`).
* **No package registry, no lock file, no reproducible two-machine build.**
  There is a module system and a project manifest (`firn.package`,
  `firnc --package <dir>`), but `compiler/src/package.rs` and
  `package_world.rs` contain not one occurrence of "lock", "registry", "http"
  or "download": everything is resolved from the local file system, and it
  stays whole-program compilation — no separate object files, no version
  resolution. (`W1`, SPEC 14.1 item 15.)
* **No stack probing and no upper bound on the frame.** The prologue reserves
  the frame without a check; a function with very many live values can step
  past the guard page without a diagnostic. There is no `probe` and no guard
  page handling in `compiler/src/codegen_x86.rs`.
* **Instruction-accurate debug lines only with `--no-opt`.** With the
  optimizer the line of the `fn` declaration remains, because FIR carries no
  source positions (SPEC 14.1 item 16). `gdb` shows no local variables yet —
  there is `.debug_line` but no `.debug_info` for local names.
* **Error messages inside imported modules name the wrong file.** Line and
  column are right, the file name shown is that of the root file (SPEC 14.1
  item 18).
* **The build is not warning-free.** `cargo build --release` currently reports
  **11 warnings**, all of them dead code or unused imports. There is no
  blanket `#![allow(…)]` hiding anything — but the sentence "builds with zero
  warnings" that used to stand here was no longer true.

### Not yet proved, though it is built

* **The 24 hour GC run.** The soak test does 100,000,000 cycle sets =
  700,000,000 DOM objects at a constant 1,364 KiB RSS, and the
  reference-counting counter-check on the same object graph blows up to
  750,080 KiB (factor 550). What is still missing for a tick is the 24 hour
  run itself and fragmentation with **changing** object sizes — the soak
  always allocates the same set, which is the friendly case
  ([ACCEPTANCE.md](ACCEPTANCE.md) item 2).
* **The speed target of `<= 2x` Rust is missed — but only just.** Median
  **2.08x** and **2.19x** in two passes of nine runs each, range
  1.43x–4.16x. Three of the six programs are inside the target; `sieve` is
  the outlier that carries the median, and the distance is where LLVM
  vectorizes. The same target **is** met for the HTML tokenizer against
  html5ever (1.18x on real pages, 0.80x on the pathological corpus).
  Raw tables: [bench/RESULTS.md](bench/RESULTS.md).

---

## How it is built

```
.fi  →  lexer  →  parser  →  type checker  →  FIR  →  optimizer  →  codegen  →  as  →  ld  →  ELF
       (hand written)      (sema, monomorphization)   (mem2reg, CSE,   (x86-64 / aarch64)
                                                       inlining, LICM,
                                                       regalloc)
```

Look at every stage yourself:

```sh
firnc --emit=tokens  file.fi     # token stream
firnc --emit=ast     file.fi     # syntax tree
firnc --emit=fir-raw file.fi     # FIR straight after lowering
firnc --emit=fir-opt --stats file.fi   # FIR after the optimizer, with sizes
firnc --emit=asm     file.fi     # the assembly text, Intel syntax
firnc --list-passes              # the optimizer's pass register
firnc --list-attrs               # attributes: implemented / planned
```

A worked example — the optimizer folding `20 * 2 + 2` down to one instruction,
the generated assembly of `examples/fib.fi`, and the shape of the error
messages — is in [docs/MODULE_REPORTS.md](docs/MODULE_REPORTS.md).

Error messages look like this:

```
$ ./compiler/target/release/firnc tests/neg/implicit_cast.fi
error: operator '+' expects two operands of the same integer type, found i32 and i64
  --> tests/neg/implicit_cast.fi:5:18
   |
 5 |     let c: i64 = a + b
   |                  ^^^^^ here
   = note: there is no implicit conversion, use 'as'
```

The parser reports several errors per run (recovery at statement level). On
broken input the compiler exits with code 1 — no panic, no `unwrap` crash;
`test.sh` fails the suite if a negative test produces a Rust panic.

## Tests

```sh
bash test.sh
```

`test.sh` builds the compiler, runs the Rust module tests, then compiles
**every** program in `tests/`, `tests/opt/` and `examples/` **three times**
(`opt`, `--no-opt`, `--opt-level=dev-fast`; all three have to agree), links it,
**runs** it and compares the exit code and the standard output with the
expectation written in line 1. After that come the negative tests (the compiler
has to fail with the right message at the right `line:column`, and must not
panic) and about forty section proofs: the optimizer, the result-location
guarantee, the architecture guard, the symbol scheme, the HTML5 tokenizer, CSS,
the DOM soak run, five stage-0-against-stage-1 comparisons (lexer, parser,
layout/ABI, type checker, lowering), the self-hosting fixpoint, threads, the
freestanding kernel profile, JavaScript, packages, the formatter, DWARF, the
language server, the calling convention against `gcc`, sockets, NBT, the
Minecraft server, `extern fn` in both directions, and aarch64.

**The state of `bash test.sh` on this branch, 2026-08-23: `FAIL 6/1204`.**
Not one of the six comes from anything this round changed (`git diff main`
touches `README.md`, `bench/RESULTS.md`, `docs/`, `examples/tour.fi` and two
checker scripts — no compiler, no library, no test program):

* **two were real and reproducible** — `tools/aarch64/run.sh` in both build
  stages, on `tests/1613_crypto.fi`. Round 91 built the vector instructions
  for the second machine and both are green again (`simd_a64.rs`);
* **four are load flakes** on a machine that was running five copies of this
  suite at once, and every one of them was re-run on its own and passed:
  `tools/thread/run.sh` (the deliberate counter-check "the unlocked counter
  MUST lose increments" — with the cores oversubscribed the four threads do not
  overlap; the test program itself returned 0 in 12 of 12 direct runs),
  `tools/fixpoint.sh` (same test, reached through the corpus comparison; the
  fixpoint itself was **character-identical**, stage 2 == stage 3), and
  `tools/js/run.sh` + `tools/js/round66.sh` (the promise soak segfaulted under
  memory pressure and returned 0 on the re-run).

A machine-readable subset for CI, without the section proofs:

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json
```

## Command line

```
firnc [OPTIONS] file.fi
  -o <path>            output file
  --package <dir>      compile the project from <dir>/firn.package
  --emit=exe|asm|fir|fir-raw|fir-opt|comptime|tokens|ast|ast-canon|layout|types
  --target=<name>      x86_64-linux (default) | aarch64-linux
                       | x86_64-none | aarch64-none  (freestanding: no
                         operating system, ELF object, no syscall)
  --profile=<name>     kernel | app (SPEC 2)
  --opt-level=<lvl>    dev | dev-fast | release-safe | release-fast
  --no-opt             switch off the optimizer (= --opt-level=dev)
  --no-pass=<name>     switch off a single pass  ·  --list-passes
  --list-attrs         attributes and their state
  -c, --object         only assemble: ELF object file, no ld
  --lsp                language server over stdin/stdout
  --stats --timings --keep-asm --strlit=<lit> --version -h
```

## Where things live

| path | what |
|---|---|
| `compiler/src/` | the stage 0 compiler in Rust, 57 modules, no dependencies |
| `lib/firnc1/` | the same compiler **in Firn** — the one that reaches the fixpoint |
| `lib/std/`, `lib/str/`, `lib/num/`, `lib/rt/`, `lib/gc/` | the standard library, written in Firn |
| `lib/html/`, `lib/css/`, `lib/dom/`, `lib/layout/`, `lib/js/` | the browser stack: tokenizer, CSS, DOM, layout, JavaScript |
| `tests/`, `tests/opt/`, `tests/neg/` | the test programs (positive, optimizer, negative) |
| `examples/`, `demos/` | small programs; `demos/kernel` boots in QEMU, `demos/mcserver` |
| `bench/`, `tools/` | the benchmarks (Firn + Rust in duplicate) and every proof script |
| `testdata/` | html5lib-tests, eight saved real pages, `bigtest.nbt`, test262 |

## Documentation

| file | what is in it |
|---|---|
| [SPEC.md](SPEC.md) | the language specification; **14.1** lists every deviation of the implementation |
| [ACCEPTANCE.md](ACCEPTANCE.md) | the six acceptance items, ticked off only against a measurement |
| [ROADMAP.md](ROADMAP.md) · [PLAN.md](PLAN.md) | where this is going |
| [DESIGN_GOALS.md](DESIGN_GOALS.md) | ten foundation decisions and their reasons |
| [RUN.md](RUN.md) | build everything, run everything, measure everything |
| [docs/BENCHMARKS.md](docs/BENCHMARKS.md) | every number with the command that produced it, including the ones Firn loses |
| [docs/FIR.md](docs/FIR.md) | the IR: instructions, types, invariants |
| [docs/SELF_HOSTING.md](docs/SELF_HOSTING.md) | the bootstrap plan and how far it got |
| [docs/MODULE_REPORTS.md](docs/MODULE_REPORTS.md) | the long round-by-round reports that used to live in this file |
| [docs/DEBUGGER.md](docs/DEBUGGER.md) · [docs/ERROR_UNIONS.md](docs/ERROR_UNIONS.md) · [docs/RC.md](docs/RC.md) | debugger session, error unions, reference counting |
| `docs/ROUND*.md` | one report per round, newest [docs/ROUND86.md](docs/ROUND86.md) |
| [LOGBOOK.md](LOGBOOK.md) | the short log |

The language name lives in exactly three constants
(`compiler/src/config.rs`: `LANG_NAME`, `LANG_NAME_LOWER`, `FILE_EXT`).

## Licence

**Two licences, and which one applies depends on the directory.**

* **MIT** for the RUNTIME AND THE STANDARD LIBRARY -- `lib/std`, `lib/rt`,
  `lib/gc`, `lib/rc`, `lib/str`, `lib/num`, `lib/math`, `lib/mem`,
  `lib/test`, `lib/generated`, plus `compiler/src/panic_rt*.rs`, `demos/`
  and `examples/`. That is everything the compiler links into a program YOU
  write. **You may therefore ship a Firn program under any licence you
  like, including a closed one** -- compiling with `firnc` does not put your
  program under the GPL. Full text: [LICENSE.MIT](LICENSE.MIT).
* **GPL-2.0-only** for everything else: the compiler (`compiler/`,
  `lib/firnc1/`, `bin/`), the browser engine Certus (`lib/browser`,
  `lib/css`, `lib/dom`, `lib/font`, `lib/html`, `lib/js`, `lib/layout`,
  `lib/net`, `lib/paint`, `lib/tls`), the tools and the tests. Full text:
  [LICENSE](LICENSE).

**Version 2 ONLY, never "or later".** GPLv3 section 6 would force a device
maker to hand out the signing keys of a consumer device, which makes
binding firmware to its machine as a theft deterrent legally impossible.
Linux and Android are GPLv2-only for the same reason.

Every source file carries an `SPDX-License-Identifier:` line, which is the
authoritative answer for that file. The reasoning and the file-by-file
boundary are in [LICENSING.md](LICENSING.md); third-party material and its
own terms are in [THIRD_PARTY.md](THIRD_PARTY.md).

## Licence

MPL-2.0. Change a file of Firn and pass it on: publish that file. Write your
own program in Firn: it stays yours, closed or sold. See LICENSE, NOTICE and
THIRD_PARTY.md.

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
| **Two machines** | x86-64 and aarch64, same source, **296 of 301 programs byte-identical output**; 1 differs, and it is named below | `tools/aarch64/run.sh` |
| **Language** | structs, arrays, `enum` + `match` with exhaustiveness check, generics, interfaces, closures and function values, error unions `E!T`, `defer`/`errdefer`, `comptime` + `emit`, `f32`/`f64`, `str` with `f"…"` interpolation, threads, `extern fn` in both directions | `tests/` (three build levels each) |
| **Garbage collector** | opt-in, incremental mark-sweep, **longest pause 0.45 ms** at 120,000 live nodes; weak refs, finalizers, `GcVec`/`GcMap` | `tools/dom_soak/run.sh` |
| **Tooling** | formatter, DWARF line info + `gdb`, language server (`firnc --lsp`), package/project system, test runner with JSON output | `tools/fmt`, `tools/dwarf`, `tools/lsp`, `tools/packages` |
| **HTML** | tokenizer written in Firn, **6,810 / 6,810 html5lib cases (100.00 %)**; against html5ever **1.18x** on real pages and **0.80x** (ahead) on the pathological corpus | `tools/tokenizer/run.sh`, `throughput.sh` |
| **CSS + layout** | against Chromium: **1,087 / 1,087 boxes, deviation 0.00 %**, paint order 5,171 / 5,171 probe points | `tools/layout/run.sh` |
| **JavaScript** | test262, **63,364 cases, nothing filtered**: parser **91.94 %**, engine **76.00 %** | `tools/js/run.sh` |
| **Cryptography, compression** | SHA-256, AES, DEFLATE — written in Firn, held against OpenSSL/zlib and the NIST vectors; behind by 1.38x–1.88x | `tools/stdlib81/run.sh`, `tools/bench82/run.sh` |
| **An operating system** | `profile kernel` produces a freestanding object file; `demos/kernel` boots in QEMU with tasks, address spaces, system calls and files | `tools/kernel/run.sh`, `tools/freestanding/run.sh` |
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
  x86_64-linux, aarch64-linux)"*.
* **No LLVM backend, and there will not be one** — that is the point of the
  project, not a gap. It is listed here because people ask.
* **No vector instructions on aarch64 — and this currently makes `test.sh`
  red.** AES-NI, SHA-NI and SSE are emitted for x86-64 behind a `cpuid`
  check (`compiler/src/codegen_a64.rs`, the comment at the `Simd` arm). One
  program in the corpus, `tests/1613_crypto.fi`, therefore does not compile for
  the second machine at all: *"--target=aarch64-linux cannot emit the vector
  instruction CpuFeatures yet"*. `bash tools/aarch64/run.sh` reports
  **296 of 301 identical, 1 differing** and fails, in both build stages. The
  scalar path computes the same results everywhere, only slowly (35x–147x
  slower for the cryptography, docs/BENCHMARKS.md §1); what is missing is the
  aarch64 form of the instruction, not the algorithm.
* **No package registry, no lock file, no reproducible two-machine build.**
  There is a module system and a project manifest (`firn.pkg`,
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

* **two are real and reproducible** — `tools/aarch64/run.sh` in both build
  stages, on `tests/1613_crypto.fi`, for the reason given in the "can not"
  list above;
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
  --package <dir>      compile the project from <dir>/firn.pkg
  --emit=exe|asm|fir|fir-raw|fir-opt|comptime|tokens|ast|ast-canon|layout|types
  --target=<name>      x86_64-linux (default) | aarch64-linux
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

## License

MIT — see [LICENSE](LICENSE).

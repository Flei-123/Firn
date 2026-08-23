# firnc0 -- the stage 0 compiler for Firn

`firnc0` compiles a deliberately small subset of the language **Firn** (`.fi`)
into **real x86_64 machine code**: lexer -> parser -> type checker -> own IR
(**FIR**) -> optimizer -> own x86_64 code generator -> `as` -> `ld` ->
a freestanding Linux binary without libc.

* **No LLVM, no Cranelift, no C as a backend.** The assembly text is produced
  in `compiler/src/codegen_x86.rs` itself; `as` and `ld` are used exclusively as
  assembler and linker.
* **No external crates.** `compiler/Cargo.toml` has an empty `[dependencies]`
  section; `std` is enough.
* **No parser generator.** A hand-written lexer and a recursive descent parser
  with error recovery.

The binding scope is in [SPEC.md 14](SPEC.md) (deviations of the implementation
in 14.1), the IR is documented in [docs/FIR.md](docs/FIR.md).
How to build everything, run it and **measure it yourself**: [RUN.md](RUN.md).
The acceptance status with real numbers: [ACCEPTANCE.md](ACCEPTANCE.md).
The foundation decisions -- what has to go into the foundation now so that it
stays possible later, and what can wait:
[DESIGN_GOALS.md](DESIGN_GOALS.md).

---

## Numbers

Firn is written against measurements, not against opinions. Every figure
below was run on an AMD EPYC 7571 (8 vCPU, Debian 12) with the command named
in [docs/BENCHMARKS.md](docs/BENCHMARKS.md), where the full tables live —
including the places where Firn loses.

The cryptography and the compression are **written in Firn**, not bound to
OpenSSL or zlib. "Behind by" is the reference divided by Firn.

| workload | Firn | reference | behind by |
|---|---:|---:|---:|
| **SHA-256** | 968.3 MiB/s | OpenSSL 1372.6 MiB/s | **1.42x** |
| **AES-128-CBC** encrypt | 582.0 MiB/s | OpenSSL 1056.4 MiB/s | **1.82x** |
| **AES-128-CBC** decrypt | 691.8 MiB/s | OpenSSL 1056.4 MiB/s | **1.53x** |
| **AES-128-CFB8** | 26.9 MiB/s | OpenSSL 37.1 MiB/s | **1.38x** |
| **DEFLATE** level 6 | 11.2 MiB/s | `gzip -6` 21.0 MiB/s | **1.88x** |
| xxHash64 | 5,591 MiB/s | — | — |
| HTML tokenizer, `realweb` | 29.25 MB/s | html5ever (Rust) | **1.54x** |
| HTML tokenizer, `html5lib` | — | html5ever (Rust) | **0.95x — ahead** |

| what is checked | result |
|---|---:|
| **CSS layout against Chromium** | **1,087 / 1,087 boxes, deviation 0.00 %** |
| paint order against Chromium | 5,171 / 5,171 probe points |
| **HTML tokenizer**, html5lib-tests | **6,810 / 6,810 (100.00 %)** |
| **JavaScript**, test262 (63,364 cases, unfiltered) | parser **91.94 %**, engine **76.00 %** |
| **the same program on x86-64 and aarch64** | **290 / 294 identical output, 0 differing** |
| garbage collector, longest pause | **0.45 ms** at 120,000 live nodes |
| arena, 480,000 allocations | **one** system call, RSS drift **0 pages** |
| Minecraft server, 16 connections at once | 48.1 MiB/s payload |
| NBT against Notch's `bigtest.nbt` | 1,543 octets identical |
| `bash test.sh` | **PASS 1184 / 1184** |
| `tools/fixpoint.sh` | **stage 2 == stage 3, character-identical** |
| `tools/self_compare.sh` | 321 the same, **0 differing, 0 faulty** |

---

## Prerequisites

* `rustc` / `cargo` (tested with `rustc 1.99.0-nightly`, edition 2021)
* GNU binutils: `as` and `ld` (tested with binutils 2.40)
* Linux on x86_64 (the binaries produced use Linux syscalls directly)

## Building

```sh
cargo build --release --manifest-path compiler/Cargo.toml
```

The compiler is then at `compiler/target/release/firnc`.
The build goes through with **zero warnings** (no blanket `#![allow(...)]`
suppression).

## Quick start

```sh
./compiler/target/release/firnc -o /tmp/hello examples/hello.fi
/tmp/hello
```

Real output: the greeting of `examples/hello.fi`, written with a single
`write` system call. (The example still greets in German -- the text stands
in `examples/hello.fi`.)

```sh
./compiler/target/release/firnc -o /tmp/fib examples/fib.fi
/tmp/fib; echo "Exit: $?"
```

```
Exit: 89
```

## All tests

```sh
bash test.sh
```

The real result of this build state (excerpt; measured in person on 2026-08-14
after the merge of round 3):

```
== 1. build the compiler ==
== 2. module tests of the compiler ==
   cargo test: ok
== 3. positive tests (each with and without the optimiser) ==
   143 programs x 3 runs (opt / noopt / dev-fast)
== 4. negative tests (error messages) ==
== 5. proof of the optimiser ==
   PASS 41/41 (proof of the optimiser)
== 6. proof of the result-location guarantee (SPEC.md 13.1) ==
   OK: result-location guarantee kept (build 224 B, main 1048816 B, no bulk copy).
== 7. architecture: field access <-> memory location separated ==
   OK: field access and memory location separated (4 entry points in layout.rs, no bypass).
== 8. symbol naming scheme (DESIGN_GOALS 4) ==
   OK: symbol scheme kept (_F0. prefix, 'main' bare, modules free of collisions).
== 9. HTML5 tokenizer against html5lib (tools/tokenizer/run.sh) ==
   TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %
   TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %
   TOTAL                       6807 /  6810  99.96 %    6807 /  6810  99.96 %

PASS 485/485
```

(The three `TOTAL` lines are the main run, the same run with `--with-errors`
selected and the counter-check `--no-xml-mode`; the left column is the token
stream comparison, the right one additionally compares the parse error codes.)

`test.sh` builds the compiler, runs `cargo test` (122 module tests), compiles
**every** program from `tests/`, `tests/opt/` and `examples/` **three times**
(`opt`, `--no-opt`, `--opt-level=dev-fast`; all three have to deliver the same),
assembles, links, **runs** it and compares the exit code and standard output
with the expectation in line 1 (`// expect_exit: N` / `// expect_out: TEXT`).
After that it checks the 51 negative tests in `tests/neg/` (the compiler has to
abort with exit code != 0, print the expected message together with
`line:column` and the marker, and must **not** panic), the optimizer proof
(`test_opt.sh`), the result-location guarantee, the architecture guard, the
symbol scheme and finally the HTML5 tokenizer against the html5lib suite.

Inventory: 126 test programs in `tests/`, 13 optimizer programs in `tests/opt/`,
4 examples in `examples/`, 51 negative tests in `tests/neg/`. The tests from
rounds 1 and 2 are all still there and still pass -- no test was removed or
weakened (`tests/001...065`, `tests/opt/`, `tests/neg/`).

The same suite machine-readable (CI):

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json | python3 -m json.tool | head
# {"suite":"firn","total":337,"passed":337,"failed":0,"rate":1.0, "cases":[...]}
```

The test runner works without `test.sh` and counts every program individually in
both modes; it does not contain the optimizer proof (`test_opt.sh`, 41 checks)
and it does not contain sections 6-9, hence 337 instead of 485.

## Command line

```
firnc [OPTIONS] file.fi
  -o <path>          output file
  --emit=exe         produce an executable (default, calls as/ld)
  --emit=asm         write x86_64 assembler to the output
  --emit=fir         FIR text form (after optimization, if active)
  --emit=fir-raw     FIR right after lowering, without optimization
  --emit=fir-opt     FIR after the optimizer
  --emit=tokens      token stream (troubleshooting)
  --emit=ast         AST as debug text (troubleshooting)
  --no-opt           switch off the optimizer (= --opt-level=dev)
  --stats            print the size of the FIR (instructions/blocks)
  --keep-asm         keep the generated .s file
```

---

## A short tour of the language

```firn
struct Point {
    x: i32,
    y: i32,
}

const LIMIT: i32 = 10          // only scalar, constant-evaluable expressions

fn dist2(p: *Point, q: *Point) -> i32 {   // aggregates only by pointer (12.1)
    let dx: i32 = (*p).x - (*q).x
    let dy: i32 = (*p).y - (*q).y
    return dx * dx + dy * dy
}

fn main() -> i32 {
    var a: Point = Point{ x: 3, y: 4, }
    var b: Point = Point{ x: 0, y: 0, }
    var arr: [i32; 4] = [1, 2, 3, 4]      // fixed size, the index is usize
    var i: usize = 0
    var s: i32 = 0
    while i < 4 {
        s = s + arr[i]
        i = i + 1
    }
    if dist2(&a, &b) == 25 && s < LIMIT as i32 * 2 {
        return s as i32                    // only explicit conversion with 'as'
    }
    return 0
}
```

* `let` is immutable, `var` is mutable; parameters behave like `let`.
* **No implicit conversions**: `i32 + i64` is an error, `a as i64 + b` is
  right. Integer literals need a type that can be inferred from the context.
* `&&`/`||` short-circuit (visible as a branch in the FIR, no `and.bool`).
* Output without libc through `syscall(nr, a1..a6)`, for example
  `syscall(1, 1, &buf[0], 21)` = `write(1, buf, 21)`.

## Error messages

```
$ ./compiler/target/release/firnc tests/neg/implicit_cast.fi
error: operator '+' expects two operands of the same integer type, found i32 and i64
  --> tests/neg/implicit_cast.fi:5:18
   |
 5 |     let c: i64 = a + b
   |                  ^^^^^ here
   = note: there is no implicit conversion, use 'as'
```

The parser reports several errors per run (recovery at statement level), see
`tests/neg/two_errors.fi`. On broken input the compiler aborts with exit code 1
-- no panic, no `unwrap` crash.

## Checking the IR and the optimizer

```
$ ./compiler/target/release/firnc --emit=fir-raw tests/opt/fold_arith.fi
; FIR v0
fn @main() -> i32 {
bb0:
  %0 = alloca.ptr size=4 align=4
  %1 = const.i32 20
  %2 = const.i32 2
  %3 = mul.i32 %1, %2
  %4 = const.i32 2
  %5 = add.i32 %3, %4
  store.i32 %5, %0
  %6 = load.i32 %0
  ret %6
bb1:
  %7 = const.i32 0
  ret %7
}

$ ./compiler/target/release/firnc --emit=fir-opt --stats tests/opt/fold_arith.fi
profile:    app
fir (raw):  1 functions, 2 blocks, 9 instructions
fir (opt):  1 functions, 1 blocks, 1 instructions
; FIR v0
fn @main() -> i32 {
bb0:
  %5 = const.i32 42
  ret %5
}
```

`bash test_opt.sh` automatically checks for six programs that the instruction
count drops, that the folded constant really appears in the dump and that dead
blocks disappear (real result: `PASS 18/18`). That optimization does **not**
change behaviour is checked by `test.sh`, which runs every program with and
without `--no-opt` and demands the same result.

## Generated code (excerpt from `examples/fib.fi`)

```asm
.globl fib
fib:
    push rbp
    mov rbp, rsp
    sub rsp, 144
    mov qword ptr [rbp-8], rdi
.Lfib__bb0:
    lea rax, [rbp-132]
    mov qword ptr [rbp-16], rax
    ...
```

Register assignment is naive (one stack slot per FIR value, computed in
`rax`/`rcx`), but correct: System V argument registers, the return value in
`rax`, callee-saved registers (`rbx`, `r12`-`r15`) are never touched, the frame
is always 16 byte aligned. Look at it with `--emit=asm` or `--keep-asm`.

---

## What Firn (stage 0) still can NOT do -- the honest list

State **after round 3** (2026-08-14, merged). What the rounds delivered is
further down, per module; here stands only what is **not** there. Every item is
verifiable: the compiler reports an error with line/column for it, it does not
crash and it does not pretend it can do the job.

Round 3 built **error unions `E!T`** (SPEC 5.1) and the **HTML5 tokenizer in
Firn**, round 4 the **opt-in tracing GC including the DOM prototype and the soak
test** (SPEC 3.5) as well as the three **constant-time primitives**. What is
still missing:

* **`secret[T]`, `u128`, `mul_wide`, `declassify`, `#[constant_time]`**
  (SPEC 9) -- **not implemented.** `fn f(a: secret[u8])` reports
  `'secret[T]' is not implemented in stage 0`
  (`tests/neg/int_secret_not_implemented.fi`), `#[constant_time]` reports
  `attribute 'constant_time' is not implemented in stage 0`
  (`tests/neg/attr_not_implemented.fi`). **What has been implemented since
  round 4 are the three primitives** `select(condition, a, b)` (becomes `cmov`,
  never a conditional jump), `barrier(x)` (an opaque barrier) and
  `secure_zero(pointer, number_of_bytes)` (survives every optimization pass,
  becomes `rep stosb`) -- `compiler/src/ct.rs`, proofs
  `tests/430_ct_select.fi` to `tests/433_ct_secure_zero.fi`, five negative tests
  `tests/neg/ct_*.fi` and four codegen tests in `ct.rs`. Without `secret[T]`
  these are building blocks without a type check for secret data; the locks in
  FIR and in the optimizer (`fir::Func::secret`, `constant_time`,
  mem2reg/DCE/CSE/inlining) are in place, but they only get something to work on
  with `secret[T]` (SPEC 14.1 item 19).
* **`Gc[T]`, `gc class`, mark-sweep, DOM prototype (SPEC 3.5) -- BUILT and
  measured since round 4.** What works now: `gc class` with single inheritance
  and a prefix layout, `Gc[T]`/`GcWeak[T]`, checked `x.as?[C]`, fallible
  allocation `AllocError!Gc[C]`, the insertion barrier, `#[no_gc]`
  transitively, measurements (`gc_pause_ns_max` and friends). Soak test:
  **100,000,000 cycle sets = 700,000,000 DOM objects at a constant 1,364 KiB
  RSS**; the reference-counting counter-check with an identical object graph
  needs **750,080 KiB** after 2,000,000 cycles (factor 550). Section "Memory
  model" further down, report `docs/reports/dom.md`.
  **Since round 44** collection is incremental (longest pause 0.45 ms),
  **since round 47** there are **finalizers** (`S4`), weak fields are **really
  zeroed** on collection (`S3`), there are **external root ranges** and `Arc[T]`
  with an **atomic** counter (`lib/rc/arc.fi`, `docs/ROUND47.md`).
  Since round 53 there are **`GcVec[T]` and `GcMap[K,V]`** -- collections with a
  variable length that the collector really traces, even while they grow
  (`lib/gc/gcvec.fi`, `lib/gc/gcmap.fi`, `docs/ROUND53.md`). The DOM prototype
  uses them: 5000 children on one node, any number of attributes.
  **What stays open:** the 24 hour run from ACCEPTANCE.md item 2, fragmentation
  with changing object sizes, `virtual`, and, for the collections, the nominal
  type safety of the container.
  `Rc[T]`/`Weak[T]`/`Arc[T]` exist as **Firn modules** (`tests/modules/rc.fi`,
  `lib/rc/arc.fi`), not as language types; `Gc[module.Class]` cannot be written.
* **HTML5 tokenizer:** built and measured -- **6,810 of 6,810 (100.00 %)** in
  the token stream comparison and **6,809 of 6,810 (99.99 %)** with the parse
  error codes compared (`--with-errors`); the XML adaptation of the
  `xmlViolationTests` is implemented as an optional mode. What stays open is the
  speed target of <= 2x. Section "HTML5 tokenizer" further down, the numbers in
  ACCEPTANCE.md item 3.
* **`comptime`**, **interfaces**, **optionals**,
  **unwinding/`throw`** (SPEC 5.3). Error unions `E!T` have existed since
  round 3, but without an inferred error set, without `defer`/`errdefer` and
  with `catch |e| expression` instead of a block (SPEC 14.1.error_unions, F1-F10)
* **`defer`**, **`errdefer`**, **`drop`**, **the move checker**,
  **arenas/allocators**
* **Reference types `&T` / `inout T` as checked types** -- stage 0 has only raw
  pointers `*T`/`*mut T`; `mut` on pointers is parsed but not checked
* **A floating point type** (`f32`/`f64`) in the language -- `strtod`/`dtoa` in
  `lib/num/` compute on `u64` bit patterns (SPEC 14.1.str S2)
* **String and character literals in the source text** -- the literal path is
  finished in the compiler (`compiler/src/strings.rs`, checkable via
  `firnc '--strlit=u"a\uD800"'`), but **not hooked up to the lexer**
  (SPEC 14.1.str S1)
* **Global variables** (only `const`), **a panic handler**, run-time checks
  (overflow, division by zero, index bounds are unchecked)
* **Package management** (`W1`) -- there is a module system, but no registry, no
  lock file, no reproducible two-machine build
* **aarch64**, **WASM**, **an LLVM backend**, self-hosting (stages 1-3 of the
  ROADMAP; inventory in `docs/SELF_HOSTING.md`)

### Known weaknesses that round 2 did NOT fix

* **No stack probing, no upper limit on the frame.** The prologue reserves the
  frame without a check; a function with very many live values can run past the
  guard page without a diagnostic.
* **Instruction-accurate debug lines only with `--no-opt`.** With the optimizer
  the line of the `fn` declaration remains, because the FIR does not carry
  source positions (SPEC 14.1 item 16).
* **The performance target of <= 2x Rust is missed** -- a measured median of
  **2.8x-3.4x** (range 1.6x-6.0x), see below.

### What round 1 criticized and what is fixed now

* mem2reg, block merging, copy propagation, CSE and inlining exist
  (`compiler/src/mem2reg.rs`, `inline.rs`, `opt.rs`; `tests/opt/`, 41 checks in
  `test_opt.sh`).
* The slot-based assignment has been replaced by a **real register allocation**
  (linear scan with live intervals, `compiler/src/regalloc.rs`); the factor
  against `--no-opt` is a median of **~10x**.
* At most 6 parameters, no aggregates across function boundaries, no
  `break`/`continue`/`for`, no `[value; N]`, no module system: all lifted, each
  one recorded in SPEC.md 14.1 (items 1, 9, 11, 13, 15).

## Error unions `E!T` (module `fehlerunionen`, round 3)

SPEC 5.1 is implemented as a language feature: the `error` declaration, the type
syntax `E!T`, implicit conversion at `return`, `try`, `catch` and
`catch |e| expression`. A `!T` value is implicitly `#[must_consume]`.

```firn
error IoError { NotFound, Permission, Closed }

fn fetch(x: i32) -> IoError!i32 {
    if x == 1 { return IoError::NotFound }   // error
    return x * 10                            // success -- no ok(...)
}

fn chain(x: i32) -> IoError!i32 {
    let v = try fetch(x)                      // an error goes straight up
    return v + 1
}

fn main() -> i32 {
    let a = chain(5) catch 99                // 51
    let b = chain(1) catch 99                // 99
    return a - b + 48                        // 0
}
```

Representation: a two-variant tagged union as a struct with `__err: u32`
(0 = success, codes from 1 upwards in declaration order) and `__val: T` -- so
the aggregate ABI, register allocation and codegen apply unchanged.
Code: `compiler/src/errors.rs` (checking) and `compiler/src/lower_errors.rs`
(lowering to FIR, **without** a new FIR instruction).
Proofs: `tests/400...419_*.fi` (20 programs, all in three build stages) and
`tests/neg/err_*.fi` (11 negative tests). Example:

```
$ ./compiler/target/release/firnc -o /tmp/n tests/neg/err_try_outside.fi
error: 'try' is only allowed in a function with an error union return type, this one returns i32
```

The deliberate restrictions (no inferred error set, no `defer`/`errdefer`,
`catch |e|` binds to an expression instead of to a block, no `E!()`) are in
`SPEC.md` 14.1.error_unions as F1-F10 and in `docs/ERROR_UNIONS.md`.

## An HTML5 tokenizer in Firn against html5lib (round 3)

The tokenizer following WHATWG 13.2.5 is written **in Firn**
(`lib/html/*.fi`, 8,647 lines, 4,663 of them the generated name table for
character references). The state machine is an `enum` with **73 states** plus
`match`; the code generator turns that into a real jump table -- verifiable in
person:

```sh
./compiler/target/release/firnc --emit=asm -o /tmp/tok.s lib/html/tokenize_main.fi
grep -n "jmp qword ptr" /tmp/tok.s     # 11005:    jmp qword ptr [rdx + rax*8]
```

The harness is a **workbench** (Python, `tools/tokenizer/harness.py`, 295 lines)
and contains no tokenizer logic: it sends jobs over stdin (the protocol is in
`tools/tokenizer/LOG.md`) and compares the answer line.

```sh
bash tools/tokenizer/run.sh
```

Real output (2026-08-14, run in person):

```
file                        without error codes  with error codes
xmlViolation.test              4 /     4 100.00 %       3 /     4  75.00 %
TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %
```

**Throughput on TWO corpora** (`bash tools/tokenizer/throughput.sh`, real output
of 2026-08-14, the best of three runs each):

```
   -- corpus 'html5lib' (edge cases of the test suite, deliberately pathological)
      Firn      :     4.59 MB/s  (0.889 s for 4.08 MB, best of 3)
      html5ever :    11.22 MB/s  (0.363 s, best of 3)
      factor    : 2.45x slower than html5ever (acceptance goal <= 2.00x)

   -- corpus 'realweb' (eight real pages out of testdata/realweb/)
      Firn      :     7.44 MB/s  (0.632 s for 4.70 MB, best of 3)
      html5ever :    42.60 MB/s  (0.110 s, best of 3)
      factor    : 5.72x slower than html5ever (acceptance goal <= 2.00x)
```

Why two corpora: the corpus made from the html5lib inputs is **deliberately
pathological** (almost nothing but edge cases, very many state changes per byte,
hardly any long runs of text) and measures the worst case -- that is documented
as such in `tools/tokenizer/korpus.py`. The corpus `realweb` consists of eight
real pages saved on 2026-08-14 (Wikipedia x3, the WHATWG HTML standard, W3C,
rustdoc, Hacker News; 4.70 MB, `testdata/realweb/MANIFEST.md` names every URL).
That is exactly where html5ever is strongest: long runs of text are its best
case, while the Firn tokenizer keeps working code point by code point and writes
html5lib JSON on top of that.

The tally was identical in every run. Both corpora receive byte for byte the
same input on both sides.

**The test data are unchanged -- verifiable:**
`bash tools/tokenizer/verify_testdata.sh` compares the sha256 sums of the
14 `.test` files with the frozen set (`tools/tokenizer/testdata.sha256`,
upstream commit `224991ec10db04f056a89eed8b0bd8695fd2950e` of html5lib-tests)
and counts the 6,810 cases. `run.sh` runs that as step 0; with
`--against-upstream` the script downloads the files of that commit from GitHub
again and compares directly.

Named honestly:

* **The XML adaptation is an optional mode, not a special route**: the four
  `xmlViolationTests` demand the adaptations from "Coercing an HTML DOM into an
  infoset". The driver switches them on through a job flag (bit 0,
  `tools/tokenizer/LOG.md`), the harness sets it exclusively for the cases under
  the key `xmlViolationTests`. Counter-check (`run.sh` performs it itself):
  `python3 tools/tokenizer/harness.py <binary> --no-xml-mode` gives
  `6807 / 6810 (99.96 %)` -- so the pure HTML path is unchanged. Nothing is
  filtered and nothing is skipped.
* **Not <= 2x -- on neither of the two corpora.** Three complete measurements on
  2026-08-14 (the best run per side, html5ever with `--release`,
  `opt-level=3`, the same machine, the same bytes):
  corpus `html5lib` **2.25x / 2.45x / 2.79x**, corpus `realweb`
  **5.72x / 7.72x / 7.84x**; a fourth measurement gave **2.32x** and **7.35x**,
  a fifth (the rework of round 4) **3.09x** and **6.39x**, two more during the
  merge **2.59x** and **6.90x** and **2.42x** and **8.31x** -- the extreme
  values each lie above the range noted before; the range is therefore
  **2.25x-3.09x** (`html5lib`) and **5.72x-8.31x** (`realweb`) and not the most
  favourable run. The measurement varies by about 30 %; the jury's own number
  can lie inside these ranges. The worse value on real pages is not an outlier
  but the more honest one: that is where html5ever plays out its strength on
  long runs of text.
* **The `errors` entries of the suite (parse error codes with line/column) are
  compared** -- switch `--with-errors`, step 2a in `run.sh`. The tokenizer keeps
  track of line and column itself and prints a second JSON list behind the token
  stream (separated by a tab), for example
  `[{"code":"eof-in-tag","line":1,"col":6}]`; the code names are in
  `lib/html/error_codes.fi` (WHATWG 13.2 "Parse errors"). Result
  **6,809 / 6,810 (99.99 %)**. The single failure is `xmlViolation.test #0`:
  the input contains `U+FFFF`, for which the tokenizer correctly reports
  `noncharacter-in-input-stream`, but the file `xmlViolation.test` carries no
  `errors` lists at all and expects the empty list. The case is **counted as a
  failure**, not exempted.
* **The name table of the character references lies at no fixed address**:
  `mmap` without `MAP_FIXED`, the pointer is passed through in the
  `tokens.Sink`. If `mmap` fails, `entities.table()` returns 0, `char_ref`
  reports `REF_UNMOEGLICH` and the tokenizer sets `nicht_unterstuetzt` -- the
  case then counts as a failure instead of being tokenized wrongly in silence.
  Proof in Firn: `lib/html/entities_failure.fi` (step 1c in `run.sh`, which
  starts the program twice and demands different addresses).
* All three build stages (`opt`, `--no-opt`, `dev-fast`) deliver the same tally;
  `run.sh` aborts if they do not.

## Optimizer round 5: LICM, `lea` -- and what really slows the tokenizer down

**A new pass `licm`** (`compiler/src/licm.rs`): loop-invariant computations move
into the preheader. In `bench/firn/matmul.fi` the expression `r * n` sat in
every iteration of the innermost loop -- 240 x 240 x 3 times per run.

```
$ firnc --list-passes | grep licm
licm            function ja              hoist loop invariant computations into the preheader
```

**`lea` instead of `mov`+`add`** in the code generator: address computations
need one instruction instead of two or three, and the case "the second operand
is already in the destination register" no longer needs a detour through `rax`.
In the inner loop of `matmul` **14 of the 27 instructions were pure register
copies**; now there are 21 instructions in total.

**The inlining limit for the caller** raised from 4,000 to 24,000 FIR
instructions: the hottest function of the whole project -- `tokenizer__tokenize`
with 4,139 instructions -- previously got **not a single inlining**, although
`sink_emit_char` with 18 instructions lies far below any limit. A large function
is not automatically a cold one; with a state machine the opposite is the case.

### Measured -- and deterministically at that

On this machine the wall-clock time of the same binary varies by up to **40 %**
between runs. That makes a codegen change of 5 % impossible to judge: on the
first attempt the same improvement appeared once as -18 % and once as +6 %.
Since then `bench/instr.sh` measures the **executed instructions** with
`valgrind --tool=callgrind` -- reproducible down to the instruction.

| Program | before | after | change |
|---|---:|---:|---:|
| matmul | 1,668,312,681 | 1,376,734,921 | **-17.48 %** |
| bubblesort | 811,682,925 | 667,321,089 | **-17.79 %** |
| bytecount | 2,579,216,109 | 2,148,310,351 | **-16.71 %** |
| sieve | 825,458,961 | 708,292,727 | **-14.19 %** |
| statemachine | 1,847,172,267 | 1,721,343,055 | **-6.81 %** |
| fib | 338,351,740 | 338,353,992 | +/-0.00 % |

`fib` is pure recursion -- there is nothing for either pass to gain there.

### The tokenizer does NOT get faster from it -- here is the proof

| Corpus `realweb`, 4,931,819 bytes | instructions | per byte |
|---|---:|---:|
| Firn tokenizer | 4,033,688,605 | **818** |
| html5ever | 540,567,170 | **110** |

The ratio of **7.46x** matches the time factor of **7.04x** almost exactly.
That proves what the distance is **not** caused by: not by the quality of the
generated code. Firn executes seven and a half times as much work, and a perfect
code generator would not change that.

The causes lie in the tokenizer, not in the compiler: it first decodes the input
completely to UTF-32 (`mem.CpBuf`, four bytes per character) and then tokenizes
that buffer, it has no bulk path for runs of text (html5ever jumps to the next
`<`/`&` and emits everything in between as one block), and on top of that it
writes the html5lib JSON, which html5ever does not write.

**That is why the acceptance target "<= 2x the reference" is not reachable with
compiler work alone.** The next step belongs to the tokenizer and to a fair
measurement setup -- not to the optimizer. That is the real insight of this
round, and it is worth more than the 16 % of instructions.

## Tokenizer speed: from 7.0x to 5.0x -- and what it cost (round 6)

After the measurement had shown that the distance is **work executed** and not
codegen quality (818 against 110 instructions per byte), this round went exactly
there. Three interventions, each measured on its own:

**1. The fast path pulled up** (`lib/html/mem.fi`). `cp_reserve` and
`buf_reserve` together were **33 % of all instructions** -- not the growing, but
the check "is there still room?", which occurred as a function call for every
character. With 71 instructions and 14 blocks the full function is too large to
inline. Now `cp_push`/`buf_push` contain only the comparison (24 instructions,
3 blocks -> inlinable), the growing lives in `cp_wachse`/`buf_wachse`.

**2. A fair measurement setup** (`lib/html/tokenize_bench.fi`). The previous
driver wrote html5lib JSON per job, html5ever only counts tokens -- measured
**14.7 % of the instructions** for output that the other side does not produce
at all. The measuring version only counts as well. What is **not** subtracted is
the UTF-32 decoding (28 %), although html5ever does not need it: that is a real
disadvantage of Firn's design, not an unfairness of the measurement.

**3. `cmp` and the conditional jump merged** (`compiler/src/regalloc.rs`).
A comparison cost **seven** instructions: `cmp`, `setcc al`,
`movzx eax, al`, a copy into the destination register, `test`, `jnz`, `jmp` --
the bool value was produced, stored and immediately tested against zero again.
Now there are three. The conditions: the comparison is the **last** instruction
of the block (otherwise something in between could change the flags), its result
is read **exactly once**, and it is not a `secret` value. As a result
`decode` shrank from 583 to 503 instructions.

### The result, measured with callgrind (corpus `realweb`)

| State | instructions | against html5ever |
|---|---:|---:|
| before this round (with JSON) | 4,033,688,605 | 7.46x |
| fast path in `mem.fi` | 3,931,183,909 | 7.27x |
| fair setup + `cmp`/`jcc` | **2,655,479,880** | **4.91x** |

By the clock (the best of three runs):

| Corpus | before | after | target |
|---|---:|---:|---:|
| `html5lib` (pathological) | 2.70x | **1.98x** | <= 2.00x |
| `realweb` (real pages) | 7.02x | **4.99x** | <= 2.00x |

**Honestly:** on `html5lib` the target is reached, on `realweb` it is not -- and
`realweb` is the case that counts for a browser. The 1.98x lies so
close to the limit that it is within the noise of the clock; what is solid is
the instruction count. The rate stayed unchanged at **6,810/6,810**.

**What remains next:** `decode` is the biggest remaining item with 28 % and
needs 225 instructions per byte -- absurdly many for a UTF-8 decoding. The
reason is in the assembly: the inlined bounds check of `buf_at` produces a
branch of its own per access, and `decode` accesses up to five times per
character. Without bounds check elimination across loops (`bce` cannot do that
yet) it stays that way.

## Where the tokenizer really stands now (round 7, a measurement finding)

`decode` was the biggest item with 28 %. The obvious explanation --
`mem.buf_at` reloads `(*b).ptr` and `(*b).len` on every access, and without
alias analysis the optimizer is not allowed to combine them -- I checked by
pulling both values out in front of the loop once (`byte_bei`, in both drivers).

**The result: the explanation was only right in the smaller part.**
`decode` itself got smaller, from 1,110 to 814 million instructions (-27 %),
but the whole run only went from 2,655 to 2,631 million (-0.9 %). So the thesis
was right, but the item was smaller than the first calculation suggested. This
stands here because a refuted assumption belongs to the result just as much as a
confirmed one.

### The real reason, looked up in the assembly

A single byte access in `decode`:

```asm
mov rax, qword ptr [rbp-1672]    ; base    -- lies in the frame, not in a register
add rax, qword ptr [rbp-216]     ; + index
mov qword ptr [rbp-1304], rax    ; the address into a slot
mov rcx, qword ptr [rbp-1304]    ; and straight back out again
movzx eax, byte ptr [rcx]        ; the actual load
mov qword ptr [rbp-1312], rax    ; the result into a slot
movzx eax, byte ptr [rbp-1312]   ; and straight back out again
mov qword ptr [rbp-1320], rax    ; once more
mov eax, dword ptr [rbp-1320]    ; and once more
mov dword ptr [rbp-2488], eax
```

**Ten instructions for one byte load, eight of them pure stack shuffling.**
In the whole function **170 of 469 instructions are stack accesses**.

The cause is not laziness on the part of the register allocator -- it is a real
linear scan with live intervals and loop weights. It is simpler than that: in
`decode` nine long-lived values are alive at the same time (four parameters,
`base`, `total` and the cells `i`, `cp`, `width`), and there are eleven
registers. Nothing is left for the short-lived intermediate values -- so **each
of them gets a stack slot of its own**, even if it is read one instruction later.

### What follows from that

The right fix is that a value which is produced in the same block and read
**exactly once** needs no location at all: it stays in the working register and
is used directly. The counting for that has existed since the `cmp`/`jcc`
merging (`zaehle_lesezugriffe`).

The trap in it is real: the next instruction must not overwrite `rax` **before**
it reads the value passed through. With `d = x + passed_through` and `x` in the
frame the code generator first loads `x` into `rax` -- and the value passed
through would be gone.

### The attempt -- built, measured, discarded

Instead of going through `rax` I built it through a **register of its own**:
`r11` as `DURCHREICH_REG`, which nobody else may touch. That avoids the trap,
and the effect was visible in the assembly -- the stack accesses in `decode`
went down from **170 to 158**:

```asm
mov rax, qword ptr [rbp-1672]
add rax, qword ptr [rbp-216]
mov r11, rax                    ; instead of: mov [rbp-1304], rax
mov rcx, r11                    ; instead of: mov rcx, [rbp-1304]
movzx eax, byte ptr [rcx]
```

**And it was slower anyway.** Measured:

| | instructions | time (best of 4) |
|---|---:|---:|
| without passing through | 2,630,820,292 | 0.491 s |
| with `r11` reserved | 2,631,818,983 | 0.520 s |

The instruction count is practically identical (+0.04 %), the time is about
**6 % worse**. The reason is obvious once you see it: reserving `r11` for
passing values through takes **one of eleven registers** away from the linear
scan. What is saved on short-lived values is spent again by the long-lived ones
elsewhere -- and their accesses lie inside the loop.

The change has therefore been **taken back**. It stands here because a negative
result belongs to progress just as much: the next attempt has to manage without
reserving a register, that is, improve the allocation itself (serve short-lived
intervals first) instead of taking a register away from it.

## String literals (round 8) -- language core instead of optimization

Up to this point every piece of text in Firn had to be written as a list of
octets. This is what an error message in the GC runtime looked like:

```firn
var m: [u8; 48] = [
    102, 105, 114, 110, 45, 103, 99, 58, 32, 103, 99, 95, 105, 110, 105,
    116, 40, 41, 32, 119, 117, 114, 100, 101, 32, 110, 105, 99, 104, 116,
    32, 97, 117, 102, 103, 101, 114, 117, 102, 101, 110, 10, 0, 0, 0, 0, 0, 0,
]
```

And this is what the very same message looks like now -- one line instead of
six rows of octets:

```firn
var m: [u8; 34] = "firn-gc: gc_init() ..."
```

**Three forms**, all with complete escapes (`\n`, `\t`, `\\`, `\0`,
`\xNN`, `\uXXXX`, `\u{...}`):

| Form | Type | Contents |
|---|---|---|
| `"..."` | `[u8; N]` | UTF-8, **checked** |
| `b"..."` | `[u8; N]` | raw octets, unchecked |
| `u"..."` | `[u16; N]` | WTF-16, unchecked |

**WTF-16 holds unpaired surrogates** -- `u"a\uD800b"` is valid and gives
`[97, 55296, 98]`. That is not sloppiness but a duty: a language that only
allows well-formed Unicode cannot implement JavaScript
(`FIRN-ANFORDERUNGEN.md` 2). Proof in `tests/570_string_literals.fi`.

**How it is built -- and why so small:** the decoding had been sitting finished
in `compiler/src/strings.rs` since round 2, it was simply never hooked up to the
lexer. The parser converts a literal immediately into an **array literal**; the
type checker, lowering and the code generator never see a literal and did not
have to be touched. The price is in `SPEC.md` 14.1.str S8: the data end up in
the frame as a sequence of individual store instructions, not in `.rodata`. For
messages and paths that makes no difference, for large tables it would.

**Redeemed immediately:** the hand-written octet lists in `lib/gc/gc.fi`,
`lib/dom/meas.fi` and `lib/html/entities_failure.fi` are gone -- a 63 entry row
of numbers became one string literal with the failure message of the name
table.

## `defer` (round 9)

```firn
fn read(path: *mut u8) -> i32 {
    let fd: i32 = open(path)
    defer close(fd)          // runs on EVERY exit
    if fd < 0 {
        return -1                 // here as well
    }
    return process(fd)
}
```

* **Reverse order** of declaration, as with `drop`.
* **`return` clears all levels**, the innermost first. The return value is
  computed beforehand -- a `defer` sees it but cannot replace it.
* **`break`/`continue` clear exactly the levels that were declared inside the
  loop.** For that, `lower::loops` remembers the depth of the `defer` stack when
  the loop is entered.
* **Evaluation only on exit -- like Zig, not like Go.** Go evaluates the
  arguments immediately and stores them in hidden copies; that contradicts
  "nothing hidden". In Firn the following holds:

  ```firn
  var i: i32 = 5
  defer remember(i)    // remembers 9, not 5
  i = 9
  ```

* **A jump out of the body is an error:**

  ```
  error: 'return' is not allowed inside a 'defer'
    --> file.fi:6:9
     = note: the deferred body has to end normally; otherwise it would be
       undefined what happens to the remaining deferred statements
  ```

### `errdefer` (round 10)

```firn
fn work(x: i32) -> E!i32 {
    defer clean_up()        // always
    errdefer undo()  // only on the error path
    let w: i32 = try may_fail(x)
    return w + 1
}
```

Both share **one** list per block level and run in a common reverse order -- if
the `errdefer` stands behind the `defer`, it runs first. The error path is
propagation through `try` and a `return E::Variant`; an ordinary return value is
not, even if the function returns an error union.

**An honest limit:** if a *finished* error union is passed on (`return u`), it
is only known at run time whether this is the error path. Stage 0 **rejects
that** instead of silently ignoring `errdefer`:

```
error: 'errdefer' and passing on a finished error union do not go together
       in stage 0: ... write 'return try ...' or return the error with
       'return E::Variant'
```

Proofs: `tests/580_defer.fi` (five sections, all three build stages),
`tests/neg/defer_return.fi`, `tests/neg/defer_break.fi`.

## `comptime` -- functions run at compile time (round 12)

```firn
fn factorial(n: i64) -> i64 {
    var r: i64 = 1
    var i: i64 = 2
    while i <= n { r = r * i; i = i + 1 }
    return r
}

const FACT10: i64 = factorial(10)     // 3628800 -- computed by the compiler
const SIZE: usize = factorial(5) as usize
var field: [u8; 120] = [0 as u8; 120] // SIZE serves as an array length
```

Loops, branches, local variables, **recursion** (`fib(20)` in the test).
This is the first step towards acceptance item 6: the 697 Web IDL files, the
HTML entities, the CSS tables and the Unicode data of a browser are **generated
code**.

**Limits that are enforced:** at most 2,000,000 statements and 64 nested calls
-- a `comptime` must not hang the compiler:

```
error: comptime: more than 2000000 steps - endless loop?
  --> file.fi:5:5
```

**Not possible yet:** pointers, arrays, structs, `syscall`, floating point. All
of those need memory at compile time. The attempt is reported, not compiled
wrongly in silence.

### `emit` -- generated source text (round 13)

```firn
comptime {
    emit_raw("fn tab_gross(c: i64) -> i64 {\n")
    for c in 97..123 {
        emit_raw("    if c == ")
        emit_number(c)
        emit_raw(" { return ")
        emit_number(upper(c))
        emit_raw(" }\n")
    }
    emit_raw("    return c\n}\n")
}

fn main() -> i32 {
    return tab_gross(97) as i32   // 65 -- the function never existed in the source
}
```

The text is lexed, parsed and appended to the program in the **same run**.
`firnc --emit=comptime` shows what the compiler has in front of it:

```
fn tab_gross(c: i64) -> i64 {
    if c == 97 { return 65 }
    ...
```

This is exactly how the Unicode tables, the CSS properties and the Web IDL
bindings of a browser come about.

**One trick:** `emit_roh` needs no strings in the interpreter -- the parser has
already turned `"abc"` into an array of octets, and the interpreter reads it
back.

### Reading data while the compiler runs (round 14)

```firn
comptime {
    let n: i64 = file_size("data/upper_lower.txt")
    // ... parse the file byte by byte, generate code line by line ...
}
```

`tests/602_comptime_ucd.fi` reads a file in the format of `UnicodeData.txt` --
semicolon-separated fields, the code point in field 0, the upper case mapping in
field 12 -- and produces from it:

```
fn ucd_gross(c: i64) -> i64 {
    if c == 97 { return 65 }
    if c == 228 { return 196 }
    if c == 255 { return 376 }
    return c
}
const UCD_ZEILEN: i64 = 5
```

**That is acceptance item 6.** What is still missing is proving it against the
*real* UCD (1.9 MB, all categories) and a build script that fetches it.

**Security from the start:** file access at compile time is a way in for supply
chain attacks -- an included library could otherwise read `/etc/passwd` during
the build and write it into the generated code. Hence: only relative to the
source file, no `..`, no absolute path.

```
error: comptime: '/etc/passwd' is an absolute path - only paths relative to
       the source file are allowed
error: comptime: '../geheim.txt' contains '..' - access stays inside the
       directory of the source file
```

That is stricter than necessary. Once Firn gets the capability model from
`DESIGN_GOALS.md` 3, it becomes a permission that a module has to request.

## Floating point `f64` (round 11)

```firn
fn area(r: f64) -> f64 {
    return 3.14159265358979 * r * r
}

let x: f64 = 1.5e-1
let n: i64 = (2.99 as i64)      // 2 -- truncating towards zero
```

Literals (`1.5`, `1e3`, `1_000.25`), `+ - * /`, all six comparisons, `-x`,
conversions in both directions. **29 checks** in `tests/590_f64.fi`,
in all three build stages -- among them NaN, infinity and negative zero.

**The bug that IEEE 754 demands:** `ucomisd` sets `ZF=PF=CF=1` for NaN. The
unordered case therefore looks like "less than or equal", and on the first
attempt `nan < 1.0` returned **true**. Solved not by recomputing from the parity
flag, but by **swapping the operands**: `a < b` is emitted as `b > a` with
`seta` -- and `seta`/`setae` are unordered-safe by themselves.

**Two restrictions, clearly named:**

* **No register allocation for `f64`** -- the linear scan only knows the integer
  registers. Every function with `f64` goes through the baseline path: correct,
  but without register allocation and therefore slow.
* **An ABI of its own** -- `f64` is passed as a bit pattern in integer
  registers, not in `xmm0`-`xmm7`. Consistent within Firn; for foreign libraries
  it would be wrong. Firn does not call anything foreign today.

The two belong together and need the same SSE register class.

**No `f32`** -- which is why floating point literals are not typeless: `1.5` is
always `f64`. No `%` (that would be `fmod`), no bit operations on floating
point, no implicit conversion.

## Memory model: the opt-in tracing GC and the DOM soak test (round 4)

The most important open design question from `DESIGN_GOALS.md` is decided **and
demonstrated**: an **opt-in** tracing GC. Opt-in means that the tokenizer, the
rasterizer and the crypto code do not pay for it -- `#[no_gc]` turns that into a
checked promise instead of a declaration of intent.

```firn
gc class Node {
    parent: Gc[Node],        // strong, in BOTH directions -- a real cycle
    first_child: Gc[Node],
    listener: Gc[Listener],
}
gc class Element extends Node { attr_count: u32 }

fn build() -> AllocError!Gc[Element] {
    let e = try gc Element{ ... }   // the allocation may fail
    return e
}
```

Check it yourself:

```
$ bash tools/dom_soak/run.sh                    # default: 600 s per version
$ SOAK_SEK=12 SOAK_ZYKLEN=400000 bash tools/dom_soak/run.sh    # short
```

The run continuously builds **real DOM cycles** (parent<->child,
node<->listener, node<->JS wrapper, a live collection, a weak observer) and
measures the **real memory consumption of the process** from `/proc/self/statm`
-- not the runtime's own account of itself.

| | GC version | reference-counting counter-check |
|---|---|---|
| cycle sets | 100,000,000 | 2,000,000 |
| DOM objects | 700,000,000 | 14,000,000 |
| **RSS at the end** | **1,364 KiB** | **750,080 KiB** |
| RSS curve | constant over 1,001 samples | rising linearly |
| live objects | 8-12 | 12,000,000 |

The counter-check (`lib/dom/soak_leak.fi`) runs **on every test run** and
**has to** leak; if it stays green, `run.sh` aborts. A measurement that cannot
show a leak at all is not a measurement. Its counter is correct -- it frees the
one structure without a back reference every time and fails exclusively on the
cycles.

**Honestly about it:** the 24 hour run from the acceptance is outstanding,
fragmentation with changing object sizes is unchecked, and the conservative
stack scan has a measurable price -- an old pointer copy in a **live** frame
keeps its object alive. `docs/reports/dom.md` describes both with measurements.

## Directories

```
RUN.md                   how to build everything, run it and measure it
SPEC.md, ROADMAP.md      language specification and roadmap (the contract)
tools/build_stages/         measures dev / dev-fast / release against each other
tools/schichten/         architecture guard: field access <-> storage location
tools/result_location/       checks the result-location guarantee in the assembly
DESIGN_GOALS.md          10 foundation decisions (async colours, fallible
                         allocation, capabilities, ABI, debug build, in-place
                         init, comptime/reflection, SoA layout, hot reload)
ACCEPTANCE.md               the six acceptance items with real measurements
docs/FIR.md              the own IR: instructions, types, invariants
docs/DEBUGGER.md         .debug_line + a gdb session copied verbatim
docs/SELF_HOSTING.md    what could already be written in Firn today
compiler/src/            29 modules: config.rs main.rs lexer.rs ast.rs parser.rs
                         diag.rs types.rs sema.rs sema_match.rs sema_generic.rs
                         errors.rs attrs.rs mono.rs modules.rs abi.rs fir.rs
                         layout.rs lower.rs lower_match.rs lower_errors.rs
                         ct.rs opt.rs mem2reg.rs inline.rs regalloc.rs dwarf.rs
                         strings.rs codegen_x86.rs codegen_switch.rs
lib/str/, lib/num/       the Firn library: Bytes/Str/Str16/Atom, strtod/dtoa
lib/html/                the HTML5 tokenizer IN FIRN (8,647 lines of .fi)
tools/tokenizer/         workbench: harness against html5lib, throughput,
                         verify_testdata.sh (sha256 of the 14 .test files)
bench/tokenizer/         html5ever as a yardstick (a Cargo project of its own)
tests/                   122 programs + tests/opt (13) + tests/neg (46)
examples/                hello.fi fib.fi bubblesort.fi structs.fi
bench/                   6 microbenchmarks, in duplicate (Firn + Rust), run.sh
tools/testrunner/        test runner with --format=json (CI)
tools/strlib/            generator for lib/*.fi (produces tests/300...308)
tools/dtoa_vectors/      100,000 doubles round trip against Rust as a yardstick
testdata/html5lib-tokenizer/  tokenizer suite, unchanged (6,810 cases)
testdata/realweb/        8 saved real pages (~4.7 MB) -- measurement corpus B
test.sh                  the whole test suite (builds, runs, compares)
test_opt.sh              before/after proof of the optimizer
```

The language name is exclusively in `compiler/src/config.rs`
(`LANG_NAME`, `LANG_NAME_LOWER`, `FILE_EXT`) -- renaming = three constants.

## Sum types, pattern matching and generics (module `types`, round 2)

What is implemented: `enum` with payload, `match` with an exhaustiveness check at
compile time, jump tables in the code generator and generics by
monomorphization. The deliberate restrictions are in `SPEC.md` 14.1 under
`14.1.types` (T1-T8) -- in particular: `match` is a **statement**, enumerations
do not live inside structs by value, and only functions and structs are generic.

```firn
enum Value { Nothing, Number(i32), Pair(i32, i32) }

fn main() -> i32 {
    let w = Value::Pair(7, 35)
    var s: i32 = 0
    match w {
        Value::Nothing    => { s = 0 as i32 }
        Value::Number(x)  => { s = x }
        Value::Pair(x, y) => { s = x + y }
    }
    match s {
        0        => { s = 1 as i32 }
        1..10    => { s = 2 as i32 }
        10..=99  => { s = 3 as i32 }
        _        => { s = 4 as i32 }
    }
    return s
}
```

* **Layout of an enumeration:** `__tag: u32` at offset 0, the payload from
  `round_up(4, alignment)` on, the variants overlap (a real union).
  Proof: `cargo test --release --manifest-path compiler/Cargo.toml
  sema_match::tests::layout_tag_und_nutzdaten`.
* **Exhaustiveness is an error, not a hint.** `tests/neg/match_*.fi` shows: a
  missing variant (by name), a missing `_` case for integers, an unknown
  variant, an unreachable case -- each with line:column.
* **Jump table:** from 8 tags and a density of >= 40 % on,
  `compiler/src/codegen_switch.rs` produces a `.rodata` table with
  `jmp qword ptr [rdx + rax*8]` instead of a comparison chain.
  Check it yourself:

  ```bash
  compiler/target/release/firnc --emit=asm -o /tmp/zm.s tests/230_state_machine.fi
  grep -c "jmp qword ptr" /tmp/zm.s     # 1  (state machine with 32 states)
  grep -c "^	cmp"        /tmp/zm.s     # 0  (no comparison chain)
  ```

  Checked automatically by
  `codegen_switch::tests::jump_table_at_30_states`.
* **Generics:** `fn f[T: Int](..)`, `struct Vec[T] { .. }`, the call `f[i32](..)`,
  the type `Vec[i32]`, the literal `Vec[i32]{ .. }`. Monomorphization produces
  names following the contract `name__T1_T2` (for example `vec_push__i32`,
  `Map__u32_i32`). Examples: `tests/210_generic_fn.fi`,
  `tests/211_generic_struct.fi` (Vec[T]), `tests/212_generic_map.fi` (the hash
  map `Map[K, V]`, open addressing). Unmet requirements, a wrong number of type
  arguments and generic names without `[..]` are errors with line:column
  (`tests/neg/generic_*.fi`).

Test programs of this module: `tests/200_enum_basic.fi`,
`tests/201_enum_payload.fi`, `tests/202_match_int_range.fi`,
`tests/203_match_nested.fi`, `tests/204_match_bool.fi`,
`tests/210..212_generic_*.fi`, `tests/230_state_machine.fi`;
negative tests `tests/neg/match_*.fi`, `tests/neg/generic_*.fi`.
All of them run with **and** without `--no-opt` with the same result.


## Strings and numbers <-> text (module `str`, round 2)

What is implemented is SPEC 8 (`Z1`-`Z6`) -- the four separate types, WTF-16
**without any check at all**, correctly rounded `strtod` and shortest double
output with a round-trip guarantee. The library lies in `lib/str/` and
`lib/num/` and is written in **Firn**; only the literal path sits in the
compiler (`compiler/src/strings.rs`).

| Type | Contents | checked? | File |
|---|---|---|---|
| `Bytes` | raw octets | no | `lib/str/bytes.fi` |
| `Str` | UTF-8 | yes, at the boundary (`bytes_is_str`) | `lib/str/bytes.fi` |
| `Str16` | `u16` code units (WTF-16) | **nothing** | `lib/str/str16.fi` |
| `Atom` | `u32`, interned | -- | `lib/str/atom.fi` |

The layout is as fixed in SPEC 8.1: `{ ptr, len, cap }`, with `len`/`cap` in
elements. Conversions are explicit and their fallibility shows up in the result
(`lib/str/utf8.fi`): `str16_to_utf8` -> `bool`,
`str16_to_utf8_lossy` -> U+FFFD, `str16_to_wtf8`/`wtf8_to_str16` -> lossless.

### Unpaired surrogates -- check it yourself

```bash
compiler/target/release/firnc -o /tmp/t300 tests/300_str16_surrogate.fi && /tmp/t300
# 3 97 55296 98 0 0 5 97 239 191 189 98 5 97 237 160 128 98 1 55296
#   |  |     |  |  |  |                  |                    |  ^ 0xD800 again after the WTF-8 round trip
#   |  |     |  |  |  ^ to_utf8_lossy: 'a' EF BF BD 'b'        ^ WTF-8: 'a' ED A0 80 'b'
#   |  |     |  ^ to_utf8() returns false and an EMPTY target
#   |  ^ the lone 0xD800 (55296) is preserved
#   ^ length 3
```

The literal path in the compiler can be checked without a source file:

```bash
compiler/target/release/firnc '--strlit=u"a\uD800"'   # Str16: 0061 D800, to_utf8 nothing
compiler/target/release/firnc '--strlit="a\uD800"'    # error: unpaired surrogate in Str
compiler/target/release/firnc '--strlit=b"AB\xff"'    # Bytes: 41 42 FF
```

The API contract with the module `tok` (`str16_new`, `str16_push`, `str16_len`,
`str16_at`, `atom_intern`) is in `tests/308_str16_api.fi`. `str16_new()`
returns an aggregate as its return value; whoever has to do without it takes
`str16_init(&s)`.

### `strtod` / `dtoa`

Both live in `lib/num/` and compute in **exact big number arithmetic**
(`lib/num/bignum.fi`): `strtod` scales the fraction `D * 10^exp` so that the
quotient has exactly 53 bits, and rounds from the remainder to the nearest --
at exactly half distance to the even -- mantissa. `dtoa` is Dragon4 in free
format (Ryu/Grisu class: the same digit sequence, a different route) with
ECMAScript notation.

**The language has no floating point type yet** -- which is why both work on the
`u64` **bit pattern** of the `binary64`. That is not a shortcut (the computation
is integer anyway), but a deviation carried honestly: SPEC 14.1.str S2.

Measured on 2026-08-13 (this machine, `cargo build --release`):

| Check | Command | Result |
|---|---|---|
| 26 `strtod` hard cases (0.1, 1e23, 5e-324, 9007199254740993, 2.2250738585072011e-308, ...) | `tests/304_strtod_hardcases.fi` | **26/26 bit-exact** |
| 28 `dtoa` hard cases including +/-0, +/-Infinity, NaN | `tests/305_dtoa_hardcases.fi` | **28/28 as in ECMAScript** |
| 100,000 random doubles: f64 -> text -> f64 | `bash tools/dtoa_vectors/run.sh 100000 12345` | **100,000/100,000 bit-identical** |
| the same 100,000 against Rust's shortest representation | ditto, step 4 | **100,000/100,000 identical**, 13.9 s |

`tools/dtoa_vectors/gen.rs` is a **yardstick, not a dependency**: the compiler
itself still has not a single foreign crate.

### How the test programs come about

Stage 0 has no module system and no string literals. `tools/strlib/expand.py`
resolves `//#include lib/...` and `//#str name text` and produces the
self-contained programs `tests/300..307_*.fi`, `tests/neg/str*.fi` and
`tools/dtoa_vectors/dtoa_stream.fi` from them. The generated files lie in the
tree, so `test.sh` does not need the tool:

```bash
python3 tools/strlib/expand.py --check   # are the generated files up to date?
python3 tools/strlib/expand.py --all     # regenerate them
```

### What is missing (honestly)

* String literals are finished in the compiler, but **not wired into the
  lexer** -- they do not exist in `.fi` source text yet (SPEC 14.1.str S1).
* No `f64` in the language (S2), no `Wtf8` type of its own (S5), no `Rope`
  (S6), atom numbers only at run time instead of at build time (S7).

## Optimizer, register allocation and performance (module `opt`, round 2)

Files: `compiler/src/opt.rs` (control, folding, DCE, CSE, bounds checks),
`compiler/src/mem2reg.rs` (memory to value, copy propagation, block merging),
`compiler/src/inline.rs` (inlining),
`compiler/src/regalloc.rs` (register allocation + register-aware emission),
`tests/opt/**`, `test_opt.sh`, `bench/**`.

### What the optimizer does now

| Pass | Effect | check it yourself |
|---|---|---|
| constant folding | as in round 1, unchanged | `tests/opt/fold_*.fi` |
| **mem2reg** | an `alloca` that is written **once** and whose `store` **dominates** all `load`s disappears | `tests/opt/mem2reg_single_store.fi`: `load.i32` 3 -> 0 |
| **dead store** | an `alloca` that is never read from, together with all its `store`s | `tests/opt/dead_store.fi`: `store.i32` 3 -> 0, `alloca` 1 -> 0 |
| local store forwarding | `store p,v; ... ; load p` -> `v` (within a block, conservative around calls/stores) | `mem2reg::tests::load_nach_store_*` |
| copy propagation | identity `cast`, `x+0`, `x*1`, `x*0`, `ptradd p,0`, ... | `mem2reg::tests::algebraic_identities` |
| **CSE** along the dominator tree | the same pure expression is computed once | `tests/opt/cse_common.fi`: `mul.i32` 2 -> 1 |
| **block merging** + jump threading | empty `br` blocks gone, chains merged | `tests/opt/block_merge.fi`: 8 blocks -> 1 |
| **inlining** with a size heuristic | <= 40 instructions, <= 8 blocks, no recursion, not into or out of `#[constant_time]` | `tests/opt/inline_call.fi`: `call @square` 1 -> 0 |
| **repeated conditions** | a `brcond` on an already decided condition -> `br` | `tests/opt/redundant_check.fi`: `brcond` 3 -> 2 |
| **register allocation** (linear scan) | live intervals, weighted spilling, callee-saved correctly saved | `tests/opt/regalloc_loop.fi`, see below |

`Op::Select`, `Op::Barrier`, `Op::SecureZero` and every value in `f.secret` are
changed, replaced or removed by **no** pass; a `select` never becomes a branch
(SPEC 9.2). There are tests of their own for that
(`mem2reg::tests::secret_values_stay_untouched`,
`select_stays_select`, `regalloc::tests::select_stays_cmov_also_with_registers`).

### Register allocation -- the proof

```
firnc --emit=asm -o /tmp/ra.s tests/opt/regalloc_loop.fi
sed -n '/^\.Lsumme__bb2:/,/^\.Lsumme__bb3:/p' /tmp/ra.s
```

```
.Lsumme__bb2:
    mov r11d, r10d
    mov r15d, r9d
    add r11d, r15d
    mov r10, r11
    mov r11d, r9d
    add r11d, 1
    mov r9, r11
    jmp .Lsumme__bb1
```

Not a single `[rbp-...]` access in the loop body; the counter and the sum lie in
`r9`/`r10`. `bash test_opt.sh` checks exactly that automatically (and that every
callee-saved register in use is saved **and** restored).

The method: liveness analysis per block (`live_in`/`live_out`), from it one
interval per value, a **linear scan** with an active list; if the supply is not
enough, the active interval with the smallest weight (uses x loop depth) gives
up its register. Handed out are `rbx`, `r12`-`r15` (callee-saved, across calls)
and `r8`-`r11` (only for intervals that do not enclose a `call`/`syscall`).
`rax`, `rcx`, `rdx`, `rsi`, `rdi` stay working registers. In addition the
allocator keeps non-escaping `alloca` cells (<= 8 bytes, uniform access width)
permanently in a register -- that replaces the phi nodes that FIR does not have
(SPEC 14.1.opt O3).

### Performance against Rust -- honestly measured, target **missed**

`bash bench/run.sh` (6 microbenchmarks, each **in duplicate**: `bench/firn/*.fi`
and `bench/rust/*.rs` with `rustc -O` and `black_box`; both print their result,
and the measurement aborts if the outputs do not match).
Median of 7 runs, AMD EPYC 7571, rustc 1.99.0-nightly, 2026-08-13:

| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Factor Firn/Rust |
|---|---:|---:|---:|---:|
| fib (recursive) | 0.049 s | 0.143 s | 0.031 s | **1.57x** |
| sieve (5 million) | 0.117 s | 1.115 s | 0.029 s | **4.08x** |
| matmul 240^3 | 0.122 s | 2.061 s | 0.025 s | **4.95x** |
| bytecount 16 MiB | 0.509 s | 5.244 s | 0.181 s | **2.81x** |
| bubblesort 6000 | 0.102 s | 1.304 s | 0.038 s | **2.68x** |
| statemachine 8 MiB | 0.225 s | 1.247 s | 0.083 s | **2.70x** |

**A re-measurement during the merge** (`BENCH_RUNS=5 bash bench/run.sh`, the same
machine, the evening of 2026-08-13, a shared machine): fib **1.57x**,
sieve **3.97x**, matmul **6.04x**, bytecount **1.77x**, bubblesort **5.19x**,
statemachine **2.76x** -> **median 3.36x**. The spread between two runs of the
same suite is therefore considerable (2.8x-3.4x in the median); whoever
re-measures gets a number inside that range, not exactly the table above. The
most recent measurement is always in `bench/RESULTS.md`.

**A median of 2.75x-3.36x slower than Rust `-O`** (individual values 1.57x -
6.04x). The performance target from SPEC 10.3 (`P1`, <= 2x) is therefore **not
reached** -- that number stands the same way in `ACCEPTANCE.md` and in `SPEC.md`
14.1.opt O4. Against `--no-opt` the optimizer itself brings a median of
**9.9x**. The remaining distance lies above all where LLVM vectorizes (sieve,
matrix multiplication): Firn produces scalar code exclusively, SIMD (`L16`) is
open.

Every run writes the raw table to `bench/RESULTS.md`.

### What the optimizer does NOT do (honestly)

* **No loop optimization**: no unrolling, no hoisting of invariant computations,
  no induction variables, no vectorization.
* **No interval splitting** in register allocation: a value lies either
  entirely in a register or entirely on the stack. Under high register pressure
  that costs.
* **No global PRE/GVN** -- CSE works only along the dominator tree and never
  combines `load`s (memory counts as opaque).
* **No alignment or ordering of blocks**, no branch prediction heuristic.
* Stage 0 cannot remove bounds checks at all, because it does not produce any
  (SPEC 14.1 item 3); instead the pass removes provably repeated conditions
  (SPEC 14.1.opt O5).

## Build stages (DESIGN_GOALS.md 5)

Instead of an all-or-nothing switch there are four stages. `--list-passes`
shows which pass runs in which stage and whether it is **debug-preserving**.

```
firnc --opt-level=dev          # no optimization at all (= --no-opt)
firnc --opt-level=dev-fast     # only debug-preserving passes
firnc --opt-level=release-safe # all passes
firnc --opt-level=release-fast # all passes (identical to -safe today)
firnc --no-pass=inline file.fi
```

Measured with `bash tools/build_stages/run.sh 3` (median over six benchmarks):

* **`dev-fast`: 2.06x slower than `release-fast`**
* `dev`: 10.54x slower -- the same order of magnitude as Rust's debug builds

Of nine passes exactly one is not debug-preserving: `inline`.
`--release-safe` is currently identical with `--release-fast`, because there are
no run-time checks yet that one could keep.

### A bug that only this stage found

The `dev-fast` run over the test suite immediately uncovered a real code
generator bug that **259 green tests** had not found: `r8` and `r9` are both
argument registers 5 and 6 **and** working registers of the register allocator.
The prologue moved them one after another and overwrote the arguments 5 and 6
that had not been read yet -- `tests/024_six_args.fi` returned **13 instead of
21** without inlining. It was invisible because the affected function was always
inlined in the release stages.

Fixed by a parallel register permutation (`regalloc.rs:
parallele_reg_bewegungen`) that resolves cycles through `rax`; the same class of
bug existed at the call site and at `syscall` and is fixed there as well.
Regression test: `tests/025_argreg_shuffle.fi`.

## The result-location guarantee (SPEC.md 13.1)

`let g = build(...)` passes the address of `g` to `build`; a large aggregate comes
into being **exactly once**, straight at its destination -- not first on the
stack of the producing function. The proof is in the generated assembly:

```
$ bash tools/result_location/run.sh
frame build: 224 bytes   frame main: 1048816 bytes   rep-movs: 0
OK: result-location guarantee kept (build 224 B, main 1048816 B, no bulk copy).
```

The structure is 1 MB in size; `build` still has only a 224 byte frame.

## The architecture layer: field access != storage location (DESIGN_GOALS.md 8)

In Firn `a.b` does **not** firmly mean "base address plus offset". Every field
and element access in lowering goes through `compiler/src/layout.rs`:

| Accessor | for what |
|---|---|
| `field_addr(base, sidx, name, span)` | a named struct field |
| `field_addr_at(base, offset)` | a known offset (payload of an `enum` variant) |
| `elem_addr_const(base, esz, i)` | an element with a constant index (literals) |
| `elem_addr(base, esz, i, ty)` | an element with a computed index |

The reason: the planned SoA arrangement (`SoaVec[T]`, for the rasterizer and the
layout tree) does not have the contiguous value physically at all -- there the
address is `column_f + i * size(f)`. Introducing a second arrangement now means
adding a case distinction **in this one module**, instead of hunting down thirty
call sites.

The rule is **enforced**, not merely written down:

```
$ bash tools/schichten/run.sh
OK: field access and memory location separated (4 entry points in layout.rs, no bypass).
```

The guard runs as section 7 in `test.sh` and checks that `Op::PtrAdd` outside
`layout.rs` is only built in the one helper function `ptradd_const`, that the
direct calls to it are exclusively aggregate hand-overs marked
`// ABI-Wortkopie`, and that no field offset is computed into an address by hand
in lowering any more. A deliberately introduced violation is reported with file
and line (counter-checked).

## Attributes (SPEC.md 14.2)

Firn has an **attribute registry** -- `compiler/src/attrs.rs` is the only truth
about which attributes exist, where they belong and whether stage 0 implements
them:

```
$ firnc --list-attrs
Attribute

NAME            TARGET       ARGS  STAGE 0     PURPOSE
must_consume    fn, struct   0     implemented result must not be discarded (SPEC 3.3, 5.1)
no_gc           fn           0     implemented no collection run in this call tree (SPEC 3.5.4)
constant_time   fn           0     error       no jump on secret data, checked in the code generator (SPEC 9.2)
...
```

**The most important property: nothing is silently ignored.** A known but not
yet implemented attribute is a compile error with line, column and a note about
its intended purpose. A `#[constant_time]` standing there without effect would
be the most dangerous bug this language can have.

Four kinds of error, all with a source excerpt:

```
error: unknown attribute 'must_consum'
  --> file.fi:3:1
   = note: did you mean 'must_consume'? '--list-attrs' shows all

error: attribute 'constant_time' is not implemented in stage 0
   = note: geplant: no jump on secret data, checked in the code generator (SPEC 9.2)

error: attribute 'packed' does not belong before a function
error: attribute 'align' expects 1 argument(s), found 2
```

### `#[must_consume]`

In front of `fn` or `struct`. The result must not be discarded as a statement:

```firn
#[must_consume]
struct Guard { fd: i32 }

fn open(fd: i32) -> Guard { return Guard{ fd: fd, } }

fn main() -> i32 {
    open(7)        // error: the result must not be discarded
    return 0
}
```

**The honest scope:** what is checked is the subset decidable without a move
checker -- *the result of a call must not be discarded as a statement*. The full
form from SPEC 3.3 (*the value has to be passed to a consuming function*) comes
with the move checker. `#[must_consume]` deliberately does not promise more
here than it delivers.

## The symbol naming scheme (DESIGN_GOALS.md 4)

Generated linker symbols carry a reserved prefix with the scheme version and
have room for a later ABI version:

```text
_F0.add              an element of the root file
_F0.helper__square  an element of a module
_F0.add.v3           with an ABI version (later, #[abi_stable(3)])
main                 the entry point, unchanged
```

`SYMBOL_SCHEMA = 0` sits in every symbol: if the scheme changes, the linker
reports a missing name instead of quietly linking two incompatible build states
together. Firn identifiers may not contain a dot -- so user code cannot hit the
prefix.

What matters is the **separation**: the *internal name* (type checker, IR, error
messages) and the *linker symbol* are two different things. One becomes the
other at exactly one place: `codegen_x86::label` -> `modules::symbol`.

```
$ bash tools/symbole/run.sh
OK: symbol scheme kept (_F0. prefix, 'main' bare, modules free of collisions).
```

The proof builds a program out of two modules that both contain a function
`help`, runs it and checks against the real symbol table (`nm`).

## Re-entering the checking phases (DESIGN_GOALS.md 7)

`Checker::add_items` checks **additional** declarations with the state that has
already been built up -- the same name table, the same type table, the same
diagnostics. The table of expression types grows with it; the whole-program
check (`main` present and correct) still runs exactly once.

This is needed by `comptime`/`emit`: there, elements come into being *during*
compilation -- Web IDL bindings, CSS tables, Unicode data. A type checker built
as a single pass over a fixed AST cannot learn that afterwards. That is why the
capability sits there before there is a generator for it -- and it is backed by
three tests instead of claimed:

* a function that only came into being later calls one from the first pass and
  is typed correctly
* an addition with an unknown name produces **the same** error as in the first
  pass -- an addition is not a back door
* an addition that declares `main` again is detected as a duplicate declaration

---

## License

MIT — see [LICENSE](LICENSE).

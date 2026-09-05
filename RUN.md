# RUN.md -- build it, run it, measure it yourself

Everything here **has been run** exactly as it stands (2026-08-14, AMD EPYC
7571, Linux x86_64, rustc 1.99.0-nightly, binutils `as`/`ld`). Relative paths
only, everything inside this directory.

## 0. Prerequisites

* `cargo`/`rustc` (only to build the compiler and the yardsticks -- the
  compiler itself has **no** external crates)
* GNU `as` and `ld` (assembler and linker, **no** C compiler as a backend)
* `python3` (only for the workbenches: benchmarks, generators)
* `gdb` (only for the debugger proof)

## 1. Build the compiler

```sh
cargo build --release --manifest-path compiler/Cargo.toml
```

Expected: **zero warnings**, binary at `compiler/target/release/firnc`.

## 2. Compile and run a program

```sh
compiler/target/release/firnc -o /tmp/hello examples/hello.fi
/tmp/hello ; echo "exit=$?"
```

Further modes:

```sh
firnc --no-opt -o /tmp/a file.fi      # without the optimizer (same result!)
firnc --emit=asm file.fi              # x86_64 assembly (Intel syntax)
firnc --emit=fir file.fi              # own IR, readable
firnc --help
```

Several files into **one** binary (module system): name the root file,
`import path.module` resolves relative to its directory:

```sh
compiler/target/release/firnc -o /tmp/mod tests/110_module.fi
/tmp/mod ; echo "exit=$?"     # exit=60, as stated in line 1 of the file
```

## 3. The whole test suite

```sh
bash test.sh
```

Measured result for this state: **PASS 485/485**
(143 programs x 3 build stages `opt` / `--no-opt` / `--opt-level=dev-fast` = 429,
51 negative tests, plus one section proof each for the optimizer (`test_opt.sh`,
41 checks in its own right), the result-location guarantee, the architecture
guards, the symbol scheme and the HTML5 tokenizer against html5lib; on top of
that 122 Rust module tests, which do not count individually towards PASS).
Runtime about 4 minutes.

Machine-readable (CI, goal 9 / ACCEPTANCE item 4 A):

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
./tools/testrunner/target/release/testrunner --format=json > /tmp/firn.json
python3 -c "import json;d=json.load(open('/tmp/firn.json'));print(d['total'],d['passed'],d['failed'],d['rate'])"
# 337 337 0 1.0
```

(337 instead of 485: the runner contains neither the optimizer proof
`test_opt.sh` nor sections 6-9 of `test.sh`.)

## 4. The proofs one by one -- what the jury checks

| What | Command | Measured result |
|---|---|---|
| **Exhaustiveness check for `match`** | `firnc -o /tmp/m tests/neg/match_missing_variant.fi` | `error: 'match' is not exhaustive: ... not covered` **with line:column**, exit != 0 |
| **Jump table for 32 states** | `firnc --emit=asm -o /tmp/zm.s tests/230_state_machine.fi && grep -c "jmp qword ptr" /tmp/zm.s` | `1` -- one indirect jump through a `.quad` table, no comparison chain |
| **WTF-16, unpaired surrogate** | `firnc -o /tmp/s tests/300_str16_surrogate.fi && /tmp/s` | `3 97 55296 98 0 0 5 97 239 191 189 98 5 97 237 160 128 98 1 55296` -- `0xD800` is preserved, `to_utf8()` returns nothing, `to_utf8_lossy()` returns `EF BF BD` |
| **strtod/dtoa hard cases** | `firnc -o /tmp/h tests/304_strtod_hardcases.fi && /tmp/h` | 26 bit patterns, starting with `4591870180066957722` (= `0.1`); the expected values are given as `// expect_out:` in line 1 of the same file |
| **100,000 doubles there and back** | `bash tools/dtoa_vectors/run.sh 100000 4242` | `OK: 100000/100000 bit-identical on the way back, 100000/100000 shortest form like Rust` (7.9 s) |
| **Benchmarks against Rust `-O`** | `BENCH_RUNS=5 bash bench/run.sh` | median **3.36x** slower (range 1.57x-6.04x), table in `bench/RESULTS.md`. **Target <= 2x missed** |
| **The optimizer has an effect** | `bash test_opt.sh` | `PASS 41/41` (FIR before/after) |
| **The debugger shows `.fi` lines** | `firnc --no-opt -o /tmp/gdbdemo docs/gdb_example.fi && gdb -batch -ex "break summe" -ex run -ex bt /tmp/gdbdemo` | `Breakpoint 1, summe () at docs/gdb_example.fi:2` and `#1 ... main () at docs/gdb_example.fi:11` |
| **The generated Str tests are current** | `python3 tools/strlib/expand.py --check` | `expand.py: 0 files out of date` |
| **Cleanliness** | `grep -rn "todo!\|unimplemented!" compiler/src` | no hits |


## 4a. HTML5 tokenizer and error unions (round 3)

```sh
bash tools/tokenizer/run.sh
```

Builds the tokenizer from `lib/html/*.fi` in **three** build stages, runs all
**6,810** html5lib cases, checks that all three build stages produce the same
balance, and measures throughput against html5ever. **Two** rates are
reported: token stream only (left column) and, in addition, with the parse
error codes compared (right column, `harness.py --with-errors`). Measured
result (2026-08-14):

```
TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %
   noopt: 6810 without / 6809 with error codes -- equal
   devfast: 6810 without / 6809 with error codes -- equal
   -- corpus 'html5lib' (edge cases of the test suite, deliberately pathological)
      Firn      :     4.59 MB/s  (0.889 s for 4.08 MB, best of 3)
      html5ever :    11.22 MB/s  (0.363 s, best of 3)
      factor    : 2.45x slower than html5ever (acceptance goal <= 2.00x)
   -- corpus 'realweb' (eight real pages out of testdata/realweb/)
      Firn      :     7.44 MB/s  (0.632 s for 4.70 MB, best of 3)
      html5ever :    42.60 MB/s  (0.110 s, best of 3)
      factor    : 5.72x slower than html5ever (acceptance goal <= 2.00x)
```

Measurements are taken on **two** corpora: `html5lib` (the inputs of the test
suite, deliberately pathological -- almost nothing but edge cases, the worst
case) and `realweb` (eight saved real pages, `testdata/realweb/MANIFEST.md`).
Two further complete runs gave 2.25x / 2.79x (html5lib) and 7.72x / 7.84x
(realweb), a fifth 3.09x and 6.39x respectively; the range is therefore
2.25x-3.09x (html5lib) and 5.72x-8.31x (realweb). The balance was identical in
every run and in all three build stages, throughput varies by about 30 %.

Step 0 of `run.sh` proves that the expectations were not touched:

```sh
bash tools/tokenizer/verify_testdata.sh              # sha256 against the repo set
bash tools/tokenizer/verify_testdata.sh --against-upstream   # additionally against GitHub
```

Step 2b of `run.sh` is the **counter-check without the XML adaptation**:

```
python3 tools/tokenizer/harness.py .tokenizer-work/tokenize --no-xml-mode
TOTAL                            6807 /   6810    99.96 %
```

The XML adaptation (`xmlViolationTests`) is an optional mode of the driver
(job flag bit 0, `tools/tokenizer/LOG.md`); the harness enables it only for the
four cases from `xmlViolation.test`, the HTML path stays the same.

The html5ever yardstick has to be built once for this (a Cargo project of its
own, **not** a dependency of the compiler):

```sh
cargo build --release --manifest-path bench/tokenizer/Cargo.toml
```

Without it `run.sh` keeps running and reports the missing yardstick.

Individual proofs:

| What | Command | Measured result |
|---|---|---|
| **The tokenizer is Firn** | `wc -l lib/html/*.fi tools/tokenizer/harness.py` | 8,647 lines of `.fi` against 295 lines of harness; the state machine sits in `lib/html/tokenizer.fi` (1,516 lines) |
| **Jump table over 73 states** | `firnc --emit=asm -o /tmp/tok.s lib/html/tokenize_main.fi && grep -c "jmp qword ptr" /tmp/tok.s` | `1` -- indirect jump through `.Ltbl_tokenizer__tokenize_0` |
| **Character references one by one** | `python3 tools/tokenizer/check_entities.py` | `bestanden: 4657 / 4657` |
| **Error union: `catch` delivers the fallback** | `firnc -o /tmp/e tests/403_catch_replacement.fi && /tmp/e; echo $?` | `0` |
| **Error union: `try` propagates** | `firnc -o /tmp/e tests/401_try_chain.fi && /tmp/e; echo $?` | the value entered in line 1 as `// expect_exit:` |
| **A discarded `!T` is an error** | `firnc -o /tmp/e tests/neg/err_discarded.fi` | `error: the result must not be discarded: the type 'E!i32' is marked with #[must_consume]` with line:column |
| **`try` outside an error-returning function** | `firnc -o /tmp/e tests/neg/err_try_outside.fi` | `error: 'try' is only allowed in a function with an error union return type, this one returns i32` with `8:13` |

## 4b. Freestanding compilation: `profile kernel` (round 52)

```sh
bash tools/freestanding/run.sh
```

Measured result (2026-08-19): **41 passed, 0 failed** -- among them a real QEMU
boot of the kernel example with **both** compilers.

| What | Command | Measured result |
|---|---|---|
| **ELF object instead of a binary** | `firnc -o /tmp/k.o demos/freestanding/core.fi && readelf -h /tmp/k.o \| grep Type` | `REL (Relocatable file)` -- no `ld`, no `_start` |
| **No undefined symbols (except `osum_panic`)** | `nm -u /tmp/k.o` | empty, or exactly `osum_panic` if the program uses checked arithmetic (round 72, SPEC section 13) -- `demos/freestanding/start.s` defines it, resolved when the object is actually linked (next row) |
| **No system call in the code** | `objdump -d /tmp/k.o \| grep -c syscall` | `0` |
| **It boots** | `ld -n -T demos/freestanding/linker.ld --defsym=KERN_START=_F0.kern_start -o /tmp/k.elf /tmp/start.o /tmp/k.o && objcopy -O elf32-i386 /tmp/k.elf /tmp/k.mb && qemu-system-x86_64 -kernel /tmp/k.mb -serial stdio -display none` | `FIRN: profile kernel ist` / `freestanding.` |
| **`syscall` in the kernel profile** | `firnc -o /tmp/x tests/neg/free_syscall_in_kernel.fi` | `error: 'syscall' does not exist in profile 'kernel'` with line:column |
| **Floating point without `#[allow_fp]`** | `firnc -o /tmp/x tests/neg/free_float_without_allow_fp.fi` | `error: floating point (the type f64) is allowed in profile 'kernel' only with #[allow_fp] ...` |
| **`#[interrupt]` cannot be called** | `firnc -o /tmp/x tests/neg/free_interrupt_call.fi` | `error: 'ih' is an interrupt entry point and cannot be called` |
| **volatile holds** | `firnc --emit=fir tools/freestanding/volatile.fi \| grep -c 'asm.void "pause"'` | `3` -- three literally identical blocks, no CSE |

In detail in `docs/ROUND52.md`.

## 5. What does NOT work, because it was not built

Honestly and completely (in detail in `ACCEPTANCE.md`):

* **Constant time -- partly implemented, the item stays open.** Built are the
  three primitives (`compiler/src/ct.rs`): `select(b, a, c)` -> `cmov` without a
  conditional jump, `barrier(x)`, `secure_zero(p, n)` (survives the
  optimizer). Proof: `tests/430_ct_select.fi` ... `tests/433_ct_secure_zero.fi`
  in three build stages, `tests/neg/ct_*.fi` (5 negative tests).
  **Not** implemented: `secret[T]`, propagation of the marking, `declassify`,
  `u128`, `mul_wide`, any effect of `#[constant_time]`. Without `secret[T]`
  there is no type check for secret data. Verifiable:
  `firnc -o /tmp/x tests/neg/int_secret_not_implemented.fi` reports
  `error: 'secret[T]' is not implemented in stage 0` with line/column.
  See `ACCEPTANCE.md` item 6.
* **GC, `Rc`/`Gc`, DOM prototype, RSS soak test** -- not implemented. Verifiable:
  `tests/neg/int_gc_not_implemented.fi`.
* **HTML5 tokenizer: built.** html5lib cases passed:
  **6,810 of 6,810 (100.00 %)** in the token stream comparison and
  **6,809 of 6,810 (99.99 %)** when the `errors` entries of the suite (parse
  error code, `line`, `col`) are compared as well
  (`harness.py --with-errors`, step 2a of `run.sh`). The single failure is
  `xmlViolation.test #0`. The XML adaptation of the four `xmlViolationTests` is
  implemented as an optional mode (counter-check `--no-xml-mode`: 6,807).
  The speed target of <= 2x is **missed**: range 2.25x-3.09x (corpus
  `html5lib`) and 5.72x-8.31x (corpus `realweb`). See section 4a.
* **`defer` / `errdefer`, inferred error set `!T`, `catch |e| { block }`**
  -- not implemented, see `SPEC.md` 14.1.error_unions F1-F10.
* **Self-hosting, package management, `comptime`/UCD table** -- open,
  see `docs/SELF_HOSTING.md` and `ACCEPTANCE.md` items 1, 5, 6.

## 6. Cleaning up

All working directories are disposable and listed in `.gitignore`:

```sh
rm -rf .test-work .opt-work .strwork .dtoa-work .testrunner-work .tokenizer-work bench/.work
```
# PLAN.md -- build plan for `firnc0` (stage 0)

**Related:** `SPEC.md` (10.1 grammar, 11 ABI/numbers, 12 + 12.1 binding scope),
`ROADMAP.md` phase 1. **SPEC.md and ROADMAP.md are not rewritten in substance**
-- 12.1 was extended and records the deliberate restrictions of the
implementation.

The scaffold is already in place and **builds** (`cargo build --release`). The
shared data structures (`config.rs`, `diag.rs`, `ast.rs`, `types.rs`, `fir.rs`,
`main.rs`) are DONE and count as a **frozen interface**. The remaining files are
stubs that report a clean error (no `todo!()`).

---

## 1. File ownership -- who touches what

| Module | exclusively these files |
|---|---|
| **frontend** | `compiler/src/lexer.rs`, `compiler/src/parser.rs` |
| **sema** | `compiler/src/sema.rs` |
| **lowering** | `compiler/src/lower.rs`, `docs/FIR.md` |
| **opt** | `compiler/src/opt.rs`, `tests/opt/**`, `test_opt.sh` |
| **codegen** | `compiler/src/codegen_x86.rs` |
| **suite** | `tests/*.fi`, `tests/neg/*.fi`, `examples/*.fi`, `test.sh`, `README.md` |

**Nobody** changes `config.rs`, `diag.rs`, `ast.rs`, `types.rs`, `fir.rs`,
`main.rs`, `Cargo.toml`, `SPEC.md`, `ROADMAP.md`. If an interface turns out to
be insufficient: work around it inside your own module and name it in the report
back -- do not change the shared file on your own authority (a merge conflict
means the build stops).

No new files in `compiler/src/` (the module list in `main.rs` is fixed:
`ast, codegen_x86, config, diag, fir, lexer, lower, opt, parser, sema,
types`). Submodules inside your own file are allowed.

---

## 2. Hard rules for everybody

* No external crates. Only `std`. `Cargo.toml` stays without dependencies.
* **Zero warnings** with `cargo build --release`. No blanket
  `#![allow(...)]` suppression; individual, justified `#[allow]` only in
  exceptional cases.
* No `todo!()`, `unimplemented!()`, `panic!("...")`, no `unwrap()`/`expect()`
  on values that depend on the input. Anything unsupported reports
  `dg.error(span, "... is not supported in stage 0")`.
* No crash on broken input: no endless loop (every parser loop must provably
  make progress), no recursion explosion (a depth counter with a clear error
  beyond 200 levels of nesting).
* The language name and file extension come from `config.rs` only (`LANG_NAME`,
  `LANG_NAME_LOWER`, `FILE_EXT`). Do not hard-code "Firn"/"fi" anywhere else --
  not in text either.
* Error messages in lower case after `error: `.
* Self-check before reporting completion: actually run `cargo build --release`
  plus your own tests.

---

## 3. Shared semantics (applies to sema, lowering and codegen alike)

* **Type mapping `types::Type` -> `fir::FTy`:**
  `i8..i64 -> I8..I64`, `u8..u64 -> U8..U64`, `usize -> U64`, `isize -> I64`,
  `bool -> Bool` (1 byte, values 0/1), `*T`/`*mut T` -> `Ptr`.
  Arrays and structs are **not** FIR values: they exist only as an address
  (`alloca` + `ptradd` + `load`/`store` on fields, `copymem` when copying).
* **No implicit conversions.** Only `as`. `bool` <-> integer only with `as`.
  `as` between pointers and `usize`/`isize` is allowed, and between pointers of
  different target types as well; `as` to or from aggregates is an error.
* **Signedness:** extension in `as` follows the SOURCE type
  (signed -> `movsx`, unsigned/bool -> `movzx`), narrowing truncates.
  `/` `%` and `>>` follow the OPERAND type (signed: `idiv`/`sar`,
  unsigned: `div`/`shr`).
* **Literals:** see SPEC 12.1 item 2 -- no default type, otherwise the error
  "the type of the integer literal cannot be inferred, write for example
  `5 as i32`".
* **Aggregates across function boundaries are forbidden** (SPEC 12.1 item 1):
  parameters and return types have to be scalar; `sema` reports that with a
  clear error.
* **`main`:** has to exist, no parameters, return type `i32`.
* **`syscall(nr, a1..a6)`:** 1 to 7 arguments, each of integer or pointer type;
  each is extended to `i64` (signed -> sign extension, otherwise zero
  extension). The result type is `i64`. More than 7 arguments = error.
* **Namespaces:** functions, structs and constants each live in their own global
  namespace; local names shadow constants. A duplicate declaration in the same
  namespace = error. Shadowing inside the same block = error, in an inner block
  it is allowed.

---

## 4. Frozen interfaces (already in the tree)

```rust
// lexer.rs      pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token>
// parser.rs     pub fn parse(toks: &[Token], dg: &mut Diags) -> ast::Program
// sema.rs       pub fn check(prog: &ast::Program, dg: &mut Diags) -> Option<TypeInfo>
// lower.rs      pub fn lower(prog: &ast::Program, info: &TypeInfo, dg: &mut Diags) -> Option<fir::Module>
// opt.rs        pub fn optimize(m: &mut fir::Module) -> OptStats
// codegen_x86.rs pub fn emit(m: &fir::Module) -> String
```

`diag::Diags` provides the prescribed error format (file, line, column, source
line, `^^^` marker) -- modules do not build formats of their own.

`main.rs` already offers: `-o`, `--emit=exe|asm|fir|fir-raw|fir-opt|tokens|ast`,
`--no-opt`, `--keep-asm`, `--version`, `--help`; it assembles with `as --64` and
links with `ld -n` (as assembler and linker only).

---

## 5. The module assignments at a glance

1. **frontend** -- lexer + recursive descent parser with error recovery.
2. **sema** -- type checker, struct layout, `TypeInfo` for lowering.
3. **lowering** -- AST -> FIR (basic blocks, short-circuiting, lvalues) + `docs/FIR.md`.
4. **opt** -- constant folding + dead code elimination, with proof tests of its own.
5. **codegen** -- FIR -> x86_64 assembly, System V ABI, freestanding without libc.
6. **suite** -- >= 40 test programs, >= 8 negative tests, examples, `test.sh`, README.

The dependency order is 1->2->3->5, but every module is **testable on its own**:
opt and codegen build their test FIR programmatically (`fir::Func::new`,
`push`, `set_term`) in `#[cfg(test)]` tests and check with `cargo test` and with
`as`/`ld` respectively; the suite writes its `.fi` programs against SPEC 10.1 and
runs them against the finished compiler at the end.

## 6. Acceptance criterion

`cargo build --release` without a warning, `bash test.sh` really reporting
`PASS n/n` (with AND without `--no-opt`), `examples/hello.fi` printing visible
text, `examples/fib.fi`, `examples/bubblesort.fi`, `examples/structs.fi`
delivering verifiable results, negative tests aborting with exit code != 0 and a
comprehensible message -- without a Rust panic.

---
---

# PLAN -- round 2 (phase 2 of the ROADMAP): the language core for the browser engine

**State at the start of round 2:** `firnc0` runs, 7,760 lines of Rust,
`bash test.sh` reports **PASS 166/166** (75 programs x 2 runs, 15 negative
tests, 18 optimizer proofs), `cargo build --release` without warnings.

**Related:** `SPEC.md` v0.2 (3 memory model, 4.4 inheritance, 6.3 match,
8 strings/WTF-16, 9 constant time, 10.3 performance target, 14/14.1),
`ROADMAP.md` phase 2, `ACCEPTANCE.md` (six items),
`../osum-browser/FIRN-ANFORDERUNGEN.md` (read only).

**Principle of this round:** `SPEC.md` is the contract. Whoever deliberately
builds something narrower records it in **14.1** as a numbered item and does NOT
rewrite the SPEC. Whoever lifts an item from 14.1 strikes it there with a
justification. What is not achieved goes honestly into `ACCEPTANCE.md` and
`README.md`.

## 0. What the lead architect has already built in this round (it stands, do not rebuild)

The shared IR was extended **centrally and once**, so that the six modules can
then work in parallel and without conflicts:

| New in `compiler/src/fir.rs` | Meaning | who wires it up |
|---|---|---|
| `Term::Switch { val, ty, cases, default }` | multi-way branch, `cases` sorted ascending and free of duplicates | `types` |
| `Op::Select { cond, a, b }` | data-independent selection, **always** becomes `cmov` | `ct` |
| `Op::Barrier { val }` | opaque barrier, never optimizable away (`is_pure() == false`) | `ct` |
| `Op::SecureZero { addr, size }` | `secure_zero`, NEVER counts as dead code | `ct` |
| `Func::secret: HashSet<Val>` + `set_secret`/`is_secret` | the `secret` marking down into the IR (SPEC 9.2) | `ct` |
| `Func::constant_time: bool` | switches the codegen check on | `ct` |

Already working and tested (`cargo test`, 67 tests green):
* `codegen_x86.rs` produces a `cmov` for `Select`, a `rep stosb` for
  `SecureZero` and an opaque pass-through for `Barrier`.
* `codegen_x86.rs` aborts with an **error** if a `brcond`/`switch` inside a
  `constant_time` function depends on a `secret` value.
* `opt.rs` knows `Term::Switch` (reachability, renumbering, folding with a
  constant tag) -- the optimization stays behaviour-preserving.
* New file `codegen_switch.rs`: `emit_switch()` as a **comparison chain**
  (correct, but linear) -- `types` puts the jump table in exactly there.
* `Emitter`, `Frame`, `load_full`, `load_ext`, `store_dst`, `reg`, `size_word`,
  `block_label`, `ARG_REGS` are `pub(crate)`: other files may contribute their
  own `impl` blocks and emission functions without touching `codegen_x86.rs`.

The five `#[allow(dead_code)]` markings on exactly these new elements are
**temporary**. Whoever wires an element up **removes the marking**. A blanket
suppression (`#![allow(...)]`) is and remains forbidden.

## 1. File ownership in round 2 -- who touches what

Two modules **never** touch the same file. Where a module needs a function in a
foreign file, it writes it into a **file of its own** (Rust allows
`impl ForeignType { }` inside the same crate).

| Module | exclusively these files |
|---|---|
| **kern** | `compiler/src/lexer.rs`, `parser.rs`, `ast.rs`, `diag.rs`, `sema.rs`, `lower.rs`, `codegen_x86.rs`, new: `abi.rs`, `modules.rs`, `dwarf.rs`; `tests/1xx_*.fi`, `tests/neg/kern_*.fi`; `tools/testrunner/**`; `docs/SELF_HOSTING.md` |
| **types** | new: `compiler/src/sema_match.rs`, `sema_generic.rs`, `mono.rs`, `lower_match.rs`; the existing `codegen_switch.rs`; `tests/2xx_*.fi`, `tests/neg/match_*.fi`, `tests/neg/generic_*.fi` |
| **str** | new: `compiler/src/strings.rs` (literals/types in the compiler), `lib/str/**.fi`, `lib/num/**.fi`; `tools/dtoa_vectors/**`; `tests/3xx_*.fi`, `tests/neg/str_*.fi` |
| **opt** | `compiler/src/opt.rs`, new: `regalloc.rs`, `inline.rs`, `mem2reg.rs`; `tests/opt/**`, `test_opt.sh`; `bench/**` |
| **ct** | new: `compiler/src/ct.rs`, `int128.rs`; `lib/ct/**.fi`; `tests/4xx_*.fi`, `tests/neg/ct_*.fi` |
| **tok** | `tokenizer/**.fi`, `tools/html5lib_harness/**`, `bench/tokenizer/**`; `tests/5xx_*.fi` |

Shared files to which lines are **only appended** (never restructured):
`compiler/src/main.rs` (exactly one `mod` line per module and at most one CLI
option), `README.md`, `ACCEPTANCE.md`, `SPEC.md` 14.1 -- every module writes
there in **its own section, headed with the module name**. `config.rs`,
`fir.rs`, `Cargo.toml` stay **unchanged** (a need to change `fir.rs` is
reported, not acted on unilaterally -- it breaks all the other modules).

## 2. Order and dependencies

```
kern  (aggregates/ABI, stack arguments, break/continue/for, [v;N], modules)
  |          \
  |           \--> types (enum/match/jump table, generics)
  |                   \
  |                    \--> str (Bytes/Str/Str16/Atom, strtod, dtoa)
  |                            \
  |                             \--> tok (HTML5 tokenizer in Firn)
  +--> opt (register allocation, inlining, mem2reg, block merging, bench/)
  +--> ct  (secret, select/secure_zero/barrier, u128, #[constant_time])
```

`opt` and `ct` do **not** depend on `types`/`str` and start right away.
`tok` begins with the harness (Rust) and with the tokenizer states that can
already be written with today's language surface, and follows up as soon as
`types`/`str` deliver.

**If a module is blocked:** it delivers the part that works without the missing
input and writes the rest honestly into `ACCEPTANCE.md` as open. Half-finished
work is NOT reported as finished.

## 3. Fixed interfaces between the modules

### 3.1 kern -> everybody: the call boundary and modules

* `abi.rs`: `pub enum ArgClass { Integer(u8 /*number of 8 byte words*/), Memory, Sse }`
  and `pub fn classify(ty: &types::Type, tcx: &TypeCtx) -> ArgClass` according to
  System V AMD64. A return value > 16 bytes = `Memory` (hidden pointer in `rdi`,
  `rax` returns it). This function is the **only** truth about the calling
  convention; `opt` (inlining) and `types` (monomorphization) call it instead of
  inventing rules of their own.
* Stack arguments: arguments from the 7th INTEGER word on lie at `[rsp+8*k]`
  before the `call`, 16 byte alignment is preserved. Items 1 and 9 of 14.1 are
  struck from `SPEC.md` afterwards (with a line of justification).
* `modules.rs`: `pub fn resolve(root: &Path) -> Result<Vec<SourceFile>, Diag>`;
  `SourceFile { id: u32, path: PathBuf, src: String }`. `diag::Span` gets a
  field `file: u32`; `Diags` holds a `SourceMap`. **That is the only change to
  `diag.rs` and it belongs to kern** -- all other modules keep using `Span`
  through the existing constructors only.
* Syntax (fixed, frozen afterwards): `import path.module`, `export { a, b }`,
  namespace access `module.name`. Resolution relative to the root file.
* `for i in a..b { }`, `break`, `continue`, `[value; N]` -- desugared
  exclusively in `lower.rs`.

### 3.2 types -> everybody: sum types, patterns, generics

* AST and parser extensions are **not** made by kern but by `types` -- yet
  exclusively in files of its own: parser entry points are placed as
  `impl<'a> crate::parser::Parser<'a> { pub(crate) fn parse_enum_decl(..) }`
  in files next to `sema_match.rs`; kern keeps exactly **one** call line per
  construct free in `parser.rs` (`enum`, `match`, generic parameter list `[T]`)
  and marks it with `// HOOK types`.
* Exhaustiveness check: `pub fn check_exhaustive(...) -> Result<(), Diag>`
  in `sema_match.rs`. A missing case is an **error** with line/column and the
  name of the missing variant -- not a warning.
* Lowering produces `Term::Switch`; the jump table is created in
  `codegen_switch.rs` under the conditions documented there
  (`MIN_TABLE_CASES = 8`, density >= 40 %). Proof: `--emit=asm` of a state
  machine with >= 30 states shows `jmp qword ptr [...]` through a `.rodata`
  table, not 30 `cmp`.
* Monomorphization in `mono.rs`: produces function names following the fixed
  scheme `name__T1_T2`. That scheme is a contract (debugger, inlining, tests).

### 3.3 str -> tok: strings

* Compiler side (`strings.rs`): literals `"..."` (UTF-8, `Str`), `b"..."`
  (`Bytes`), `u"..."` (`Str16`), escapes including `\uXXXX` **with** unpaired
  surrogates. The types as built-in structs with a fixed layout:
  `Bytes { ptr: *mut u8, len: usize, cap: usize }`,
  `Str` = the same layout, checked; `Str16 { ptr: *mut u16, len, cap }`,
  `Atom = u32`.
* `Str16` checks NOTHING. Mandatory test: a lone `0xD800` is preserved,
  `to_utf8()` returns nothing, `to_utf8_lossy()` returns U+FFFD.
* Firn library `lib/str/`: `str16.fi`, `utf8.fi`, `atom.fi`;
  `lib/num/`: `strtod.fi`, `dtoa.fi` (shortest output with a round-trip
  guarantee). Test vectors: 0.1, 1e23, 5e-324, 9007199254740993,
  2.2250738585072011e-308, round-to-even plus a random test over >= 100,000
  doubles (f64 -> text -> f64 bit-identical).
* The API that `tok` relies on (the names are a contract):
  `fn str16_new() -> Str16`, `fn str16_push(inout s: Str16, u: u16)`,
  `fn str16_len(s: &Str16) -> usize`, `fn str16_at(s: &Str16, i: usize) -> u16`,
  `fn atom_intern(b: &Bytes) -> Atom`.

### 3.4 opt -> everybody: the optimizer

* The entry point stays `pub fn optimize(m: &mut fir::Module) -> OptStats`.
  `OptStats` may **only be extended** (new fields), never renamed.
* `regalloc.rs`: `pub struct Alloc { ... }`, `pub fn allocate(f: &Func) -> Alloc`
  with `pub fn loc(&self, v: Val) -> Loc` (`Loc::Reg(&'static str)` or
  `Loc::Slot(u64)`). `codegen_x86.rs` asks through exactly this function --
  **the one change `opt` is allowed to make in `codegen_x86.rs`**, and it is
  agreed with kern (one line, marked `// HOOK opt`).
* Hard rule: `Op::Select`, `Op::Barrier`, `Op::SecureZero` and every value in
  `f.secret` are changed or removed by **no** pass; a `select` **never** becomes
  a branch (SPEC 9.2).
* `bench/`: at least 6 microbenchmarks, each once in Firn and once in Rust
  (`rustc -O`, the result is used / `black_box`, so that nothing is optimized
  away), the same machine, several runs, **median**. Output as a table
  Firn / Rust / factor. The factor is entered **even if it is 4x**.

### 3.5 ct -> everybody: constant time

* `ct.rs`: `pub fn check_fn(...)` -- forbids branching on `secret`, indexing
  with `secret`, `/` and `%` on `secret`, implicit declassification.
  `declassify(x)` is the only way out. Every rejection is an error with
  line/column.
* `int128.rs`: `u128`/`i128` for `+ - *`, comparisons, shifts and
  `mul_wide(u64, u64) -> (u64, u64)`. Representation: two 64 bit words,
  aggregate ABI from kern (3.1).
* Proof in the test: compile `ct_eq` and search the generated assembly of the
  function body for conditional jumps (the test fails if there are any); the
  negative test `if secret_bool { }` has to be a **compiler error**;
  `secure_zero` has to stay visible in the assembly.
* Memory model (item 7 of the goal): `Rc[T]`/`Weak[T]` first. `Gc[T]`,
  `gc class`, mark-sweep and the DOM soak test **only if the rest stands** --
  otherwise it is carried honestly in `ACCEPTANCE.md` as *postponed*. A
  half-built GC is explicitly unwanted.

### 3.6 tok: the HTML5 tokenizer

* The tokenizer is **Firn** (`tokenizer/*.fi`). The harness may be Rust or
  Python (`tools/html5lib_harness/`).
* The harness reads **all** 14 `.test` files, honours `doubleEscaped: true`
  (an additional `\uXXXX` decoding of `input` AND `output`) and the key
  `xmlViolationTests`. **Skipped or unsupported cases count as a FAILURE**,
  never silently as a success.
* Output: the overall rate `n / 6810` plus a breakdown per `.test` file, along
  with the list of implemented states. An honest 31 % rate is the goal of this
  round, not a dressed-up one.
* Throughput comparison against `html5ever` in `bench/tokenizer/` --
  `html5ever` is a **yardstick in a separate directory**, never a dependency of
  `compiler/Cargo.toml`.

## 4. Non-negotiable rules for all modules

1. `bash test.sh` has to pass at the end of every module. No test is removed or
   weakened. New tests are added.
2. Every program runs with **and** without `--no-opt` and delivers the same.
3. `cargo build --release`: **zero warnings**, no blanket suppression.
4. No `todo!()`, `unimplemented!()`, `panic!("...")` on the required paths.
   Anything not implemented = a clean compiler error with line/column.
5. The compiler without external crates. No LLVM/Cranelift/C compiler. `as`/`ld`
   as assembler and linker only.
6. The language name still exclusively in `config.rs`.
7. Measure yourself before reporting "done": run the test suite, really measure
   the benchmarks, really run the harness. **Real numbers** in `README.md` and
   `ACCEPTANCE.md`.

---

# PLAN -- round 3 (acid test 1): error unions `E!T` + HTML5 tokenizer in Firn

State at the start: `bash test.sh` **PASS 393/393**, `cargo build --release`
without warnings. The goals of this round:

1. **Error unions `E!T`** as a language feature (SPEC 5.1) -- `error`, `try`,
   `catch`, implicit conversion at `return`, `!T` is `#[must_consume]`.
2. **An HTML5 tokenizer in Firn** under `lib/html/`, measured against the
   official html5lib test suite (**6,810 cases**), with an honest rate per
   `.test` file and an honest speed comparison against `html5ever`.

## 0. What the lead built before this round -- it stands, it runs, do not rebuild

Re-measured with `bash test.sh` -> **PASS 397/397** and
`bash tools/tokenizer/run.sh --schnell` -> **2,854 / 6,810 (41.9 %)**.

| File | Contents | State |
|---|---|---|
| `lib/html/mem.fi` | mmap heap, `Buf` (u8 vector), `CpBuf` (u32 vector), `read_all_stdin`, `write_all` | finished, contract |
| `lib/html/tokens.fi` | token model (`Sink`, `Attr`), merging of the `Character` tokens, duplicate attributes, **html5lib JSON output in Firn** (ASCII, `\uXXXX`) | finished, contract |
| `lib/html/tokenizer.fi` | `enum State` + `match`; implemented: Data, PLAINTEXT, TagOpen, EndTagOpen, TagName, BeforeAttributeName, AttributeName, AfterAttributeName, BeforeAttributeValue, AttributeValue x3, AfterAttributeValueQuoted, SelfClosingStartTag, BogusComment | **scaffold -- this is where the work happens** |
| `lib/html/tokenize_main.fi` | root file: read the job protocol, decode WTF-8, `\r\n`->`\n`, write one line per job | finished |
| `tools/tokenizer/harness.py` | workbench: `doubleEscaped`, `xmlViolationTests`, `initialStates`, `lastStartTag`, tally per file, JSON report | finished |
| `tools/tokenizer/run.sh` | builds, runs three build stages against the same tally, measures throughput, checks the regression floor | finished |
| `tools/tokenizer/throughput.sh`, `korpus.py` | 4 MB corpus + MB/s; calls `bench/tokenizer/.../html5ever_bench` when it is built | the Firn side is finished, the reference side is open |
| `tools/tokenizer/LOG.md` | the contract between Firn and the harness | finished |
| `test.sh` section 9 | the tokenizer run as part of the suite, floor in `tools/tokenizer/minquota.txt` | finished |
| `compiler/src/modules.rs` + `sema_match.rs` | **bug fix:** `match` arm bodies live in the registry, not in the AST -- the module system had not been rewriting names inside them. `match` in an imported module was unusable. Proof: `tests/231_module_match.fi` + `tests/modules/state.fi` | done |

**Known limits (honest, they belong in SPEC 14.1, not to be argued away):**
* Enumeration names are program-wide, not per module: `Ampel::Rot`, not
  `zustand.Ampel::Rot`.
* Error messages in imported modules show line and column, but the file name of
  the root file (the module `fehlerunionen` may take this on if time is left --
  otherwise record it as open).
* `lib/html/mem.fi` duplicates around 80 lines from `lib/str/alloc.fi`, because
  `lib/str` is pulled in through `//#include` and not through `import`.

## 1. File ownership in round 3 -- two modules NEVER touch the same file

| Module | May write | May only read |
|---|---|---|
| **fehlerunionen** | `compiler/src/errors.rs` (new), `compiler/src/lower_errors.rs` (new), hook lines in `parser.rs`, `ast.rs`, `lexer.rs`, `sema.rs`, `lower.rs`, `types.rs`, `attrs.rs`; `tests/4??_*.fi`, `tests/neg/err_*.fi`; `SPEC.md` 14.1, `docs/ERROR_UNIONS.md` | everything else |
| **tokenizer-kern** | `lib/html/tokenizer.fi` | `lib/html/mem.fi`, `tokens.fi` |
| **tokenizer-text** | `lib/html/entities.fi` (new), `lib/html/entities_data.fi` (generated), `tools/tokenizer/gen_entities.py` (new) | `lib/html/mem.fi` |
| **tokenizer-tokens** | `lib/html/tokens.fi`, `lib/html/tokenize_main.fi`, `lib/html/mem.fi` | -- |
| **harness-bench** | `tools/tokenizer/harness.py`, `run.sh`, `durchsatz.sh`, `korpus.py`, `minquota.txt`, `bench/tokenizer/**` (new), `bench/RESULTS.md`, `ACCEPTANCE.md`, `README.md` | everything else |

**Nobody** touches `test.sh` except the lead during the merge.
`tools/tokenizer/minquota.txt` is written only by **harness-bench** -- and
only upwards, never downwards.

## 2. Interfaces -- exact, so that work can proceed without consultation

### 2.1 `mem` (fixed, changed only by `tokenizer-tokens`)
```
heap_alloc(n: usize) -> *mut u8      heap_free(p: *mut u8, n: usize)
mem_copy(dst, src, n)                write_all(fd: i64, p: *mut u8, n: usize)
Buf   : buf_neu() -> Buf, buf_init/free/clear/reserve/push/len/at/ptr/
        buf_set_len, buf_push_dec, buf_flush(b, fd), read_all_stdin(b)
CpBuf : cp_neu() -> CpBuf, cp_init/free/clear/push/len/at/set/truncate,
        cp_copy_from(dst, src), cp_eq_cp(a, b), cp_eq_ascii_lower(a, b)
```

### 2.2 `tokens` (fixed, changed only by `tokenizer-tokens`)
```
sink_neu() -> Sink        sink_init(s)     sink_free(s)
sink_begin(s)             sink_end(s)      sink_abort(s)   sink_flush_out(s)
sink_emit_char(s, cp)     sink_flush_chars(s)
tok_start_tag(s) tok_end_tag(s) tok_comment(s) tok_doctype(s)
tok_name_push(s, cp)      tok_comment_push(s, cp)  tok_doctype_name_push(s, cp)
tok_pubid_start(s) tok_pubid_push(s, cp)  tok_sysid_start(s) tok_sysid_push(s, cp)
tok_set_force_quirks(s, b)                tok_set_self_closing(s, b)
tok_attr_start(s) tok_attr_name_push(s, cp) tok_attr_value_push(s, cp) tok_attr_finish(s)
tok_emit(s)               tok_is_appropriate_end_tag(s) -> bool
sink_set_last_start_tag_cp(s, cpbuf)
```
Whoever needs a function that does not exist (for example access to the current
tag name for RCDATA) requests it from `tokenizer-tokens` -- **nobody builds a
replacement for it inside `tokenizer.fi`.**

### 2.3 `entities` (delivered by the module `tokenizer-text`, called by `tokenizer-kern`)
```
// Processes a character reference. `pos` points just past the '&'.
// `in_attr` = true inside an attribute value (special rule of the standard).
// Return value: the new position in the input.
// The code points produced are appended through `out`:
//   out == 0  -> tokens.sink_emit_char,  otherwise tokens.tok_attr_value_push
fn char_ref(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
            s: *mut tokens.Sink, in_attribut: bool) -> usize
```
The name table is generated by `tools/tokenizer/gen_entities.py` from
`python3 -c "import html.entities"` (the official WHATWG list of the standard
library) into `lib/html/entities_data.fi` -- **not** derived from the test data.
The generator lies in the tree and is repeatable.

### 2.4 Error unions (module `fehlerunionen`)
The route recommended by the task description: `E!T` as a two-variant tagged
union in the `TypeCtx` (`__err: u32`, `0` = success, `__val: T`) plus a side
table like `enum_by_struct`. That way aggregate returns, the System V ABI,
register allocation and codegen work without any change.

## 3. Order, dependencies

* `fehlerunionen` is independent of everything else (compiler + `tests/` only).
* `tokenizer-kern` can start immediately; as long as `entities` is missing it
  keeps calling `sink_abort` for `&`.
* `tokenizer-text` can start immediately (files of its own).
* `harness-bench` can start immediately; the numbers for
  `ACCEPTANCE.md`/`README.md` are measured in person **last** and only then
  entered.

## 4. Non-negotiable

1. `bash test.sh` stays green -- all nine sections. No test is removed,
   rewritten or weakened.
2. `cargo build --release` without warnings, no external crates in the compiler,
   no `#![allow(...)]`, no `todo!()`/`unimplemented!()`.
3. Every new `tests/*.fi` runs in three build stages with the same result.
4. SPEC.md is not rewritten; deviations go into 14.1.
5. No dressed-up numbers. An unsupported case is a **failure** -- the harness
   filters nothing.

---

# PLAN -- round 4 (acid test 2): memory model SPEC 3 + DOM prototype with cycles

The goal of this round is **acceptance item 2** from `ACCEPTANCE.md`: really
build the memory model from `SPEC.md` 3 (`Rc`/`Weak`, opt-in tracing GC,
`#[no_gc]`) and use a **DOM prototype in Firn** to demonstrate that cyclic
object graphs are carried without a leak -- measured, not claimed.

Order of importance if time runs short:
**GC core > `#[no_gc]` > DOM prototype + soak test + negative test > `Rc`.**
An honestly measured ten minute run beats a claimed 24 hour run.
Whatever does not get finished goes into `SPEC.md` 14.1 as an open item --
never as a claim in `README.md`.

## 0. What the lead built before this round -- it stands, it runs, do not rebuild

The state before the round, measured in person: `bash test.sh` **PASS 485/485**,
exit 0, `cargo build --release` **zero warnings**, `cargo test --release`
122/122.

New in the tree (the skeleton of this round, already green):

| File | Contents |
|---|---|
| `compiler/src/gc.rs` | **new, empty apart from the contract.** Three queries that other modules may use: `ist_gc_alloc_aufruf(name) -> bool`, `ist_gc_zeiger(&Type) -> bool`. In the skeleton version they return `false`. The module `gckern` fills the file in. |
| `compiler/src/nogc.rs` | **new, functional.** Carries `hat_no_gc(&FnDecl)` and `hook_check(ck, prog)`: it walks all `#[no_gc]` functions, checks **rule 2** (calling a function without `#[no_gc]`) completely and **rules 1/3** through the two queries from `gc.rs`. The module `nogc` hardens the file. |
| `compiler/src/sema.rs` | exactly **one** line inserted: `// HOOK nogc` + `crate::nogc::hook_check(self, prog)` in `Checker::run`, after `add_items_inner`, before `check_main`. |
| `compiler/src/main.rs` | `mod gc;` and `mod nogc;` entered. |
| `lib/gc/`, `lib/rc/`, `lib/dom/`, `tools/dom_soak/`, `docs/reports/` | empty directories for the modules of this round. |

**No module therefore has to touch `sema.rs` or `main.rs` any more to get its
hook** -- that was the purpose of the skeleton.

## 1. File ownership in round 4 -- two modules NEVER touch the same file

| Module | May write | May **not** touch |
|---|---|---|
| **gckern** | `compiler/src/gc.rs`, `compiler/src/gc_lower.rs` (new), `lib/gc/*.fi`, and, as the **only** module, the existing compiler files `parser.rs`, `lexer.rs`, `ast.rs`, `sema.rs`, `sema_generic.rs`, `types.rs`, `layout.rs`, `lower.rs`, `lower_errors.rs`, `errors.rs`, `mono.rs`, `modules.rs`, `abi.rs`, `codegen_x86.rs`, `fir.rs`, `opt.rs`, `mem2reg.rs`, `regalloc.rs`, `main.rs`; `tests/50*_gc_*.fi` ... `tests/53*_gc_*.fi`, `tests/neg/gc_*.fi`; `docs/GC.md`, `docs/reports/gckern.md` | `compiler/src/nogc.rs`, `compiler/src/attrs.rs`, `lib/rc/`, `lib/dom/`, `tools/`, `test.sh`, `SPEC.md`, `ACCEPTANCE.md`, `README.md` |
| **nogc** | `compiler/src/nogc.rs`, `compiler/src/attrs.rs`, `lib/html/*.fi`, `tests/54*_no_gc_*.fi`, `tests/neg/nogc_*.fi`, `docs/reports/nogc.md` | all other compiler files (the hook is already there), `lib/gc/`, `lib/dom/`, `test.sh`, the documents |
| **rclib** | `lib/rc/*`, `tests/modules/rc.fi`, `tests/55*_rc_*.fi`, `tests/neg/rc_*.fi`, `docs/RC.md`, `docs/reports/rclib.md` | the compiler sources, `lib/dom/`, `lib/gc/`, `test.sh`, the documents |
| **dom** | `lib/dom/*.fi`, `tests/modules/dom.fi` (symlink), `tests/56*_dom_*.fi`, `docs/reports/dom.md` | the compiler sources, `lib/rc/`, `lib/gc/`, `tools/`, `test.sh`, the documents |
| **mess** | `tools/dom_soak/*`, `test.sh` (**appending** section 10 only), `ACCEPTANCE.md`, `README.md`, `SPEC.md` 14.1, `RUN.md`, `PLAN.md`, `docs/reports/mess.md` | every source file in `compiler/`, `lib/` |

New `tests/*.fi` are picked up by `test.sh` automatically -- nobody has to touch
`test.sh` for that. **Only `mess`** may extend `test.sh`, and only by appending
a new section; the existing sections stay byte-identical.

## 2. The language surface of the GC -- a binding contract

`gckern` implements exactly this, `dom` writes exactly against it. Deviations
only with an entry in `SPEC.md` 14.1 (made by `mess`, after the module reports).

### 2.1 Declaration

```firn
gc class Node {
    eltern:      GcWeak[Node],
    erstes_kind: Gc[Node],
    naechstes:   Gc[Node],
    kennung:     u32,
}

gc class Element extends Node {
    tag:   u32,
    attrs: Gc[Attribut],
}
```

* `gc class` is the only way to declare GC-managed values. A `gc class` value
  lives **only** on the GC heap: no value on the stack, no field of a `struct`,
  no return type -- every attempt is a compiler error with line/column.
* Permitted field types: scalar types (integers, `bool`, `*T`/`*mut T`),
  `Gc[T]`, `GcWeak[T]` and arrays of those (`[u8; 32]`). Anything else is a
  compiler error. `GcVec`/`GcMap` from SPEC 3.5.2 are **not** part of this
  round (entry in 14.1); `dom` builds lists out of `Gc` fields.
* `extends`: one base, base fields lie **at the front** (prefix layout), so that
  the upcast costs nothing. Field names of the base may not be given out again.
  No multiple inheritance. `virtual`/`override` are **not** part of this round
  (14.1) -- the language has no methods in stage 0.

### 2.2 Expressions

| Form | Type | Meaning |
|---|---|---|
| `gc Name{ f: v, ... }` | `AllocError!Gc[Name]` | allocation on the GC heap. **All** fields have to be given. On exhaustion: first a collection, then `AllocError::OutOfMemory`. The result is `#[must_consume]` (it comes from the error union) -- discarding it is a compiler error. |
| `gc_null[Name]()` | `Gc[Name]` | null value |
| `weak_null[Name]()` | `GcWeak[Name]` | null value |
| `weak(g)` | `GcWeak[T]` | a weak reference to `g: Gc[T]`; does not keep it alive |
| `stark(w)` | `Gc[T]` | upgrade; the **null value** if the target has been collected |
| `g.feld` | field type | read with an implicit deref, inherited fields as well |
| `g.feld = v` | -- | write; the insertion barrier sits exactly here |
| `g.as?[Element]` | `Gc[Element]` | checked downcast; the null value if the type tag does not lie in the ancestor chain. (`?Gc[T]` from SPEC 4.4 is the nullable `Gc[T]` in stage 0 -- 14.1) |
| `g == h`, `g != h` | `bool` | identity comparison, against `gc_null[T]()` as well |
| `Gc[Element]` -> `Gc[Node]` at `let`/assignment/argument/`return` | -- | free upcast |

`AllocError { OutOfMemory }` is declared program-wide by the GC runtime (error
set names are program-wide, see `tests/414_module_error.fi`) and is available in
every program that uses the GC. `rclib` uses **the same** set; as long as
`gckern` does not provide it yet, `rclib` declares it itself in its module and
`mess` records at the end which of the two versions is in the tree.

### 2.3 The collector interface (`gc.stats()` in stage 0 form, 14.1)

Callable without an `import`, provided by the compiler:

| Call | Type | Meaning |
|---|---|---|
| `gc_init()` | `bool` | once as the very first thing in `main`: create the heap, remember the stack bottom. `false` = `mmap` failed -- the caller **has to** fail visibly. |
| `gc_collect()` | `u64` | forces a collection, returns the pause time in ns |
| `gc_collections()` | `u64` | number of collections |
| `gc_live_objects()` | `u64` | live objects after the last run, **counted** |
| `gc_heap_bytes()` | `u64` | bytes the collector has taken from the operating system |
| `gc_live_bytes()` | `u64` | bytes occupied by the live objects |
| `gc_pause_ns_last()`, `gc_pause_ns_max()`, `gc_pause_ns_total()` | `u64` | pause times, really measured (`clock_gettime`) |

Promises that `gckern` keeps (SPEC 3.5.3):
mark-sweep, **precise** heap tracing through compiler-generated `trace` tables
derived from the field layout; a **conservative** stack **and register** scan
(the registers are spilled to the stack before the run, otherwise the promise
would be false); **no compaction**; a size-class allocator against
fragmentation; collection **only** at `gc Name{...}` sites and at
`gc_collect()`; one heap per thread; no `MAP_FIXED`, no fixed address, `mmap`
failures visible.
Incremental collection (`S5`) and finalizers (`S4`) are **not** part of this
round and are carried in 14.1 as open.

The recommended route for the runtime, so that there are no module path
problems: the collector runtime sits as **readable Firn** in `lib/gc/gc.fi`, is
embedded by the compiler via `include_str!` and pulled in as an additional
module as soon as a `gc class` occurs in the program. The state of the collector
lies in a data block created by the code generator; an intrinsic returns its
address (stage 0 has no global variables). A program needs **no** `import` and
no additional command line option.

### 2.4 `#[no_gc]` (module `nogc`)

Forbidden inside a `#[no_gc]` function, transitively, an error with
line/column: (i) GC allocation or a call to a collector function, (ii) a call to
a function without `#[no_gc]`, (iii) writing into a `Gc[T]`/`GcWeak[T]` field.
`attrs.rs`: set `no_gc` to `umgesetzt: true` and bring the test
`nur_must_consume_ist_umgesetzt` along (`vec!["must_consume", "no_gc"]`).
Proof: the existing HTML5 tokenizer in `lib/html/` is marked with `#[no_gc]` and
still compiles -- the rate in `tools/tokenizer/run.sh` must **not** get worse
(6,810/6,810 and 6,809/6,810 respectively).

## 3. The DOM prototype (module `dom`) -- the contract

`lib/dom/dom.fi` is **one** module file without `import` lines of its own
(imports are resolved relative to the root file; a symlink
`tests/modules/dom.fi` makes the same module reachable for `tests/*.fi` without
duplicating the code). Root programs lie next to it and pull it in with
`import dom`.

These kinds of cycle **have to** really occur -- a tree without back references
does not count:

1. `gc class Node` with a **parent reference AND a child list**; at least one
   variant with a **strong** parent reference (`Gc[Node]`), so that the cycle is
   real and not defined away with `GcWeak`.
2. `gc class Element extends Node` with attributes (atom tag -> text).
3. **Listeners as objects of their own** that hold their node while the node
   holds the listener (a cycle across two objects).
4. a **live `HTMLCollection`**-like structure that holds its root node.
5. an **observer through `GcWeak`** that does not keep its target alive, with a
   test that the target really is collected and that `stark(w)` returns the null
   value afterwards.
6. a simulated **JS wrapper**: the wrapper holds the node, the node holds the
   wrapper.

Mandatory functions in the module `dom` (the names are fixed, `mess` and the
tests depend on them):

```
fn dom_zyklus_bauen() -> AllocError!u64      // builds ONE complete set of cycles
                                             // (1..4 and 6), returns the number of
                                             // objects created while doing so
fn dom_zyklus_verwerfen()                    // lets the last set become unreachable
fn dom_selbsttest() -> i32                   // 0 = all kinds of cycle checked, otherwise an error code
```

## 4. Soak test and measurement (module `mess`, programs from `dom`)

`dom` delivers two **root programs** with an identical structure:
`lib/dom/soak_gc.fi` (the GC version, which must **not** leak) and
`lib/dom/soak_leak.fi` (the deliberately leaking version: the cycle over
reference counts instead of `Gc`, self-contained, **without** importing
`lib/rc`).

Both contain these three lines verbatim, so that `tools/dom_soak/run.sh` can
rewrite them into a working copy with `sed`:

```firn
const BUDGET_MS: i64 = 600000  // SOAK_BUDGET_MS
const ZYKLEN_MAX: i64 = 100000000  // SOAK_ZYKLEN_MAX
const STICHPROBE: i64 = 1000  // SOAK_STICHPROBE
```

The output protocol on standard output (TSV, binding):

```
# firn-dom-soak v1 variante=gc
# spalten: t_ms  zyklen  rss_kib  lebende  sammellaeufe  heap_bytes  pause_max_ns
0       0       1234    0       0       0       0
150     1000    1560    412     3       262144  180000
...
# fertig zyklen=123456 t_ms=600001 sammellaeufe=987 rss_kib=1560
```

* One data line per `STICHPROBE` cycles, fields separated by tabs.
* `rss_kib` comes from `/proc/self/statm` (field 2 x page size), while `lebende`,
  `sammellaeufe`, `heap_bytes` and `pause_max_ns` come from the `gc_*` calls.
  The leaking version may write `0` in the GC columns, but **never** in
  `rss_kib`.
* Stop when `t_ms >= BUDGET_MS` or `zyklen >= ZYKLEN_MAX`. Exit 0 only after a
  complete run; every error (including `mmap`) ends with exit != 0 and a line
  `# fehler ...`.

`tools/dom_soak/run.sh` (module `mess`):
1. builds both programs in **all three build stages** (`opt`, `--no-opt`,
   `--opt-level=dev-fast`) and checks that the short run delivers the same
   result everywhere;
2. runs the GC version with `SOAK_SEK` (600 s by default, considerably shorter
   in `test.sh`) and at least 100,000 cycles;
3. runs the leaking version in the **same** setup;
4. evaluates: the warm-up phase is the first quarter of the samples, after which
   the medians of the second and of the last quarter are compared for `rss_kib`
   and `lebende`. **Verdict:** the GC version is `PASS` if there is no monotonic
   rise (the threshold is documented in the file, for example < 5 % and no
   continuously rising sequence); the leaking version has to **trigger** -- if it
   stays green, `run.sh` ends with exit != 0 and the message that the
   measurement is worthless;
5. writes the measurement series as a table to `tools/dom_soak/messung-*.tsv`
   and a summary to standard output;
6. exit 0 only if (4) delivers the expected verdict for both versions.

`mess` then enters the **real** values into item 2 of `ACCEPTANCE.md`: runtime,
number of cycles, the RSS curve, collections, live objects, plus explicitly the
sentence that the 24 hour run from the acceptance is **still outstanding**, and
what was not built (incremental collection, `virtual`, `GcVec`/`GcMap`,
finalizers).

## 5. `Rc[T]`/`Weak[T]` (module `rclib`) -- the contract

Pure Firn, no compiler change. The implementation exists exactly once in the
tree: `tests/modules/rc.fi` (module `rc`), reachable from `tests/*.fi` through
`import modules.rc`; `lib/rc/rc.fi` is a **symlink** to it, so that the library
path exists without duplicating code.

* `Rc[T]` is **always immutable** -- no interior mutability, no `RefCell`
  equivalent. A reading accessor, no writing one.
* Fallible allocation: `rc_neu[T](inout alloc, wert) -> AllocError!Rc[T]`
  (stage 0 has no methods -- `Rc[T].neu` from SPEC 3.4 is written as a function,
  entry in 14.1).
* `weak_von`/`aufwerten` for `Weak[T]`, `rc_klonen`, `rc_freigeben`.
* `Arc[T]` (an atomic counter) may be dropped if time runs short -- then
  honestly into 14.1.
* **Mandatory test:** an `Rc` cycle **leaks** and that is made visible (a test
  that shows the counter never reaches 0 and the memory stays held) plus a
  sentence in `docs/RC.md` saying that this is intended.

## 6. Order and dependencies

* `gckern` starts immediately and is the critical path. It delivers **first**
  the language surface from 2 in the smallest viable form (declaration,
  allocation, field access, collection, statistics) and **only then** `extends`
  and `as?`.
* `nogc` is independent: rule 2 can be built and tested completely without
  `gckern`; rules 1 and 3 become sharp through the two queries from `gc.rs` as
  soon as `gckern` fills them in. The `lib/html/` marking can be done right away.
* `rclib` is completely independent -- the fallback position of the round.
* `dom` writes against 2 and keeps checking against whatever state has been
  built. If `gckern` does not finish in parts, `dom` still delivers the self
  test, the leaking version and the report -- and writes honestly what does not
  compile.
* `mess` builds `run.sh` against the protocol from 4 and can have it ready long
  before `dom` (a trial with a hand-written TSV file). The numbers in
  `ACCEPTANCE.md`/`README.md` are measured in person **last**.

## 7. Non-negotiable

1. `bash test.sh` stays green, all sections, 485 tests as the floor.
   No test is removed, rewritten or weakened.
2. `cargo build --release` **without warnings**; no external crates; no
   `#![allow(...)]`, no `#[allow(dead_code)]` on fields that are not wired up,
   no `todo!()`/`unimplemented!()`. Anything not implemented reports a clean
   compiler error with line/column.
3. Every new `tests/*.fi` runs in three build stages with the same result.
4. No fixed memory address, no `MAP_FIXED`; a failed `mmap` fails visibly.
5. `SPEC.md` is not rewritten -- deviations go into 14.1.
6. Every module writes its report to `docs/reports/<module>.md`: what was
   built, what was measured (real output), what is open. `mess` folds that
   together into `ACCEPTANCE.md` and `README.md`.
7. A GC that cannot be shown to collect in the test is worthless. Every GC test
   case backs its claim with `gc_collections()` and `gc_live_objects()`.
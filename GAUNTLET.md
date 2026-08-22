# Gauntlet log -- firn
**Goal:** ACID TEST 1 for the Firn language: write an HTML5 tokenizer IN FIRN and measure it against the official html5lib test suite. Build the error unions as a language feature along the way, because a tokenizer without error handling is not an honest one.

=== STARTING POINT -- READ THIS FIRST ===
The firnc compiler works and is mature enough for this job. `bash test.sh` currently reports **PASS 393/393**, and `cargo build --release --manifest-path compiler/Cargo.toml` builds with ZERO warnings. Read this before you touch anything:
- README.md -- what the language can do TODAY, with real examples. In particular: the language tour, strings, attributes, build stages.
- SPEC.md -- 5.1 error unions (the goal), 8 strings, 14 what stage 0 can do, 14.1/14.2 the deliberate restrictions.
- ACCEPTANCE.md -- the six acceptance items. Item 3 (tokenizer) stands at **0 of 6,810 (0.0 %)**. This round is supposed to turn that into a real number.
- DESIGN_GOALS.md 2 (fallible allocation) and 10 (which foundations are in place).
- docs/FIR.md, docs/SELF_HOSTING.md.
READ ONLY: ../osum-browser/TODO-FIRN.md block 0 (task 0.8), ../osum-browser/FIRN-ANFORDERUNGEN.md 13 item 3.

PRESENT and usable -- do not rebuild:
- **Module system**: `import path.module`, `export { ... }`, several .fi files linked into one binary.
- **enum + match** with an exhaustiveness check and a real jump table (`tests/230_state_machine.fi` has 32 states). THAT IS EXACTLY WHAT THE TOKENIZER NEEDS.
- **Generics** by monomorphization, `Vec[T]`, `Map[K,V]`.
- **Strings**: `Bytes`, `Str` (UTF-8), `Str16` (WTF-16, holds unpaired surrogates), `Atom` (interned). Library under `lib/str/`, numbers under `lib/num/`.
- **Attributes**: `compiler/src/attrs.rs`, `firnc --list-attrs`. `#[must_consume]` is implemented.
- **Test data**: `testdata/html5lib-tokenizer/` -- 14 `.test` files, **6,810 test cases**, counting command in `testdata/README.md`.
- **Reference**: `cargo add html5ever` works (verified). Only in a separate `bench/` subdirectory as a yardstick, NEVER as a dependency of the compiler.

=== TASK 1: ERROR UNIONS `E!T` (SPEC 5.1) ===
Build them in a MODULE OF THEIR OWN (for example `compiler/src/errors.rs` + `compiler/src/lower_errors.rs`), analogous to `sema_match.rs`/`lower_match.rs`. The hook pattern is already present in the parser (`// HOOK types`) -- do it the same way.
**Recommended route, saves you half the work:** represent `E!T` as a two-variant tagged union, exactly the way `enum` already does it -- a struct in `types::TypeCtx` with `__err: u32` (0 = success) and `__val: T`, plus a side table like `enum_by_struct`. Then aggregate returns, the System V ABI, register allocation and codegen work IMMEDIATELY, without you having to touch them.
Scope:
  1) `error IoError { NotFound, Permission, Closed }` -- error set, codes from 1 upwards.
  2) Type syntax `IoError!Buf` as a **return type** and as the type of local variables.
  3) Implicit conversion at `return`: `return value` yields success, `return IoError::NotFound` yields an error. No `ok(...)` ceremony.
  4) `try expression` -- on error, return from the function immediately with the same code, otherwise the value. Only allowed in functions that themselves return a matching error union; otherwise a clear error with line and column.
  5) `expression catch fallback` -- fallback value on error. If `catch |e| { ... }` is within reach as well: by all means, but items 4 and 5 come first.
  6) A `!T` value is implicitly `#[must_consume]`: discarded as a statement is an error (the check for that already exists in `sema.rs`, `check_discard`).
  7) `defer { ... }` and `errdefer { ... }` if time is left -- otherwise leave them out and record them as open in SPEC 14.1.
At least 15 test programs under `tests/` plus 6 negative tests (`try` outside an error-returning function, a discarded `!T`, an unknown error variant, a type error in the `catch` fallback, a duplicate error variant, mismatched error sets).

=== TASK 2: HTML5 TOKENIZER IN FIRN (the actual acid test) ===
A tokenizer following the WHATWG HTML standard, **written in Firn** (`.fi`), under `lib/html/`. The test driver may be Rust or Python (workbench, not product) -- the tokenizer itself must be Firn.
- States as `enum` + `match` with a jump table. Start with the core states: Data, TagOpen, EndTagOpen, TagName, BeforeAttributeName, AttributeName, AfterAttributeName, BeforeAttributeValue, AttributeValue(Double/Single/Unquoted), AfterAttributeValueQuoted, SelfClosingStartTag, BogusComment, MarkupDeclarationOpen, CommentStart, Comment, CommentEnd, the doctype states, RCDATA, RAWTEXT, ScriptData, CharacterReference.
- Output: token stream (DOCTYPE, StartTag with attributes and self-closing flag, EndTag, Comment, Character, EOF) in the html5lib JSON format, so that the comparison can be done mechanically.
- **WATCH OUT with the harness** -- this is where corners are usually cut, do not do it: `"doubleEscaped": true` means `input` AND `output` have to be \uXXXX-decoded as well. Some files use the key `"xmlViolationTests"` instead of `"tests"`. `initialStates` and `lastStartTag` have to be honoured. Cases you do not support count as a **FAILURE**, never as a success and never as "skipped".
- **Result: the exact number of cases passed out of 6,810**, broken down per `.test` file, in `ACCEPTANCE.md` and `README.md`. An honest rate such as "2,145 / 6,810 (31.5 %) -- states X, Y, Z are implemented, A and B are not" is the DESIRED result. A number that has been dressed up is worthless.
- **Speed**: throughput in MB/s on an input corpus, next to `html5ever` on the same corpus, with the factor stated honestly. The target from the acceptance document is <= 2x; if it is missed, the real factor gets documented.
- A script `tools/tokenizer/run.sh` that builds everything, runs it and prints the balance. Wire it into `test.sh` as a new section.

=== HARD RULES ===
- **Break nothing that exists.** `bash test.sh` must stay green, including the eight existing sections: 393 tests, optimizer proof, result-location guarantee, architecture guard for field access <-> storage location, symbol scheme. Removing or weakening tests in order to go green is cheating.
- **ZERO compiler warnings** with `cargo build --release`. No blanket `#![allow(...)]` suppression.
- No external crates in the compiler. No LLVM, no Cranelift, no C compiler as a backend.
- No `todo!()`/`unimplemented!()` on the required paths. Anything not implemented reports a clean compiler error with line and column.
- All test programs keep running in THREE build stages (`opt`, `--no-opt`, `--opt-level=dev-fast`) and produce the same result everywhere.
- **SPEC.md is the contract** and does not get rewritten to fit the code -- deviations go into 14.1.
- **Update ACCEPTANCE.md with the real numbers at the end**, item 3 in particular.
- Before you report completion, run `bash test.sh` and `bash tools/tokenizer/run.sh` YOURSELF and write the real output into the documents.
- Better task 1 complete and task 2 at an honest 30 % than both half done and claimed.
**The bar:** the jury runs these ITSELF: `cargo build --release --manifest-path compiler/Cargo.toml`, `bash test.sh`, `bash tools/tokenizer/run.sh`. No number from README.md or ACCEPTANCE.md counts without the jury reproducing it.

(a) TOKENIZER PASS RATE: the jury runs the harness itself and compares against the claimed number. What is required is an exact rate out of **6,810** cases, broken down per .test file. Check it -- this is where corners are cut most often: are unsupported cases counted as FAILURES or quietly skipped? Is `doubleEscaped` really handled? Are `xmlViolationTests` counted too? Are `initialStates` and `lastStartTag` honoured? A harness that silently filters cases is cheating and must be punished hard. An honest 30 % rate clearly beats an unreproducible 80 % rate.

(b) THE TOKENIZER IS FIRN: the tokenizer has to be written in `.fi`. If the actual state machine sits in Rust or Python, this item scores zero. Check: line counts of the `.fi` files against those of the harness; where does the logic really live?

(c) ERROR UNIONS: write and compile your own programs using `error`, `E!T`, `try` and `catch`. Does the implicit conversion at `return` work? Does `try` really propagate the error upwards (check the exit code)? Does `catch` deliver the fallback value? Is a discarded `!T` an error? Do the negative cases report line and column without the compiler crashing (no Rust panic)?

(d) NO REGRESSION: do the 393 tests from the starting state still pass, in all three build stages? Are the four architecture proofs (optimizer, result location, field access <-> storage location, symbol scheme) still green? Were tests removed, rewritten or weakened? Look at `git log` and `git diff` against the starting state. Weakened tests are a serious defect.

(e) SPEED: is the comparison against html5ever measured and documented honestly, even when the target is missed? Measure it yourself. Is the same input corpus used? Was html5ever built with `--release`? A documented "6x slower" is a good result in this category; a missing or doctored comparison is a bad one.

(f) CLEANLINESS: run `cargo build --release` yourself and check for ZERO warnings. `grep` for `todo!()`/`unimplemented!()`. Check the compiler's Cargo.toml for external crates. No blanket `#![allow]` suppression.

(g) HONESTY OF THE DOCUMENTS: does SPEC.md match what is implemented? Was SPEC.md rewritten instead of recording the deviations in 14.1 -- massive deduction. Is item 3 of ACCEPTANCE.md updated with the REAL rate? Does the README claim anything the jury cannot reproduce -- massive deduction.

BASIC RULE: only what the jury gets running itself and measures itself is graded. Smaller progress that is measured honestly is graded far better than large progress backed by claims. Gaps carried openly as gaps cost little; claimed capabilities that do not work cost a great deal.
**Best score:** 86/100 (round 1)
**Agents:** 13 - **Duration:** 6820s
**Round snapshots:** one git commit plus tag per round (gauntlet-r<N>-score<S>). Best state: branch `gauntlet-best` (round 1) -- switch with `git checkout gauntlet-best`, come back with `git checkout -`.
**Round 0 -- architecture**: 3 modules (part-1, part-2, part-3) -- existing project
**Round 1** -- score 86/100 (target 88)
  Defects: * The tokenizer produces NO parse error codes; harness.py:60 strips 'ParseError' out of both the expectation AND the actual output. The 6807/6810 are therefore only token conformance, not html5lib conformance -- recorded honestly (ACCEPTANCE.md:114-116), but the rate is more optimistic than a strict evaluation would be.  * lib/html/entities.fi:41-102: the name table sits at the FIXED address 0x600000000000 via mmap(MAP_FIXED_NOREPLACE). On kernels < 4.17 the flag is ignored (foreign mappings get overwritten); if the address is occupied and the tag does not match, table() returns 0 and lookup() silently returns 0 -- all named references then fall back to '&' in silence instead of failing.  * ACCEPTANCE.md:208-210 ('No regression ... PASS 259/259, 114 programs, 30 negative tests') contradicts README.md:89 (PASS 468/468, 139 programs, 46 negative tests) and PLAN.md:356 (starting state 393/393). Item g of the bar demands an up-to-date regression number in the acceptance document.  * tools/tokenizer/korpus.py:29-31 filters every case with doubleEscaped or initialStates out of the throughput corpus and concatenates the rest to a length of 4 MB. What is measured is therefore a corpus of pathological snippets, not HTML; the 2.79x are not meaningful for real pages (README/ACCEPTANCE do not say so).  * compiler/src/fir.rs:168,172,176,378 and abi.rs:32 carry #[allow(dead_code)] with the comment 'gets wired up by the ct module' -- the zero-warnings claim rests in part on suppressing fields that are not wired up (fir::Func::secret/constant_time, not triggerable without a frontend; ACCEPTANCE.md:201 concedes this).
**Round 2** -- score n/a/100 (target 88)

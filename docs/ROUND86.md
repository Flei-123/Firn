# Round 86 -- the README told lies, and the repository is public now

Branch `r86-readme`, base `6cf62949` (`main` after the MIT license commit).

This round wrote no compiler code. It fixed the one file that every stranger
sees first. Since 2026-08-23 the repository is public at
`https://github.com/Flei-123/Firn`, and `README.md` was still describing the
state of **round 3, 2026-08-14**. It listed as *missing* half a dozen things
that had been built weeks earlier, and it published performance numbers that
were two optimizer rounds out of date -- in both cases to Firn's own
disadvantage.

## The rule this round followed

**No claim goes into the README because a round report says so.** Every entry
in the "can not" list was checked by writing a small program and handing it to
*this* build of the compiler, and every number was measured again on this
branch. Where the round report and the compiler disagreed, the compiler won.

That mattered. Three of the corrections handed to this round were themselves
wrong:

* "global state arrived in round 75" -- it did not. Round 75 was `extern fn`.
  `var G: i32 = 7` and `static G: i32 = 7` at the top level are both still
  rejected, and SPEC 14.1 item 5 still says why.
* "checked arithmetic arrived in round 72, `release-safe` really checks
  overflow, `+%` and `+|` exist" -- not on `main`. `2147483647 + 1` wraps
  silently at every build level, `+%` does not parse. That work is round 83
  and is running in parallel on `r83-arith`; it is not merged, so the README
  must not claim it.
* "`defer`/`errdefer`/`drop` are open points to keep" -- `defer` and `errdefer`
  have existed since rounds 9 and 10 and are proved by `tests/580_defer.fi`
  and `tests/581_errdefer.fi`. Only `drop` and the move checker are open.

## What was false, what stands now, how it was checked

| the README said | the truth | how it was checked |
|---|---|---|
| "A floating point type (`f32`/`f64`) in the language" is **missing** | both exist -- `f64` since round 11, `f32` since round 71 | two programs compiled and run: `let a: f32 = 1.5 as f32; a + 0.5 as f32 == 2.0 as f32` -> exit 0; the same with `f64` -> exit 0 |
| "String and character literals ... **not hooked up to the lexer**" | `str` is a language type, `f"…"` interpolates every type, `+` concatenates | `tests/1336_fmt_types.fi` compiled and run: prints all ten integer types, `f64`, `bool` and `str` correctly; `examples/tour.fi` (new) uses `"hello, " + name` |
| the example at the top used `i32` everywhere | rewritten as `examples/tour.fi` with the aliases `int`/`double`, inference (`let dx = …`, `var sum = 0`), `f"…"` and `io.fmt_print_line` | compiled and run in **all three** build levels (`opt`, `--no-opt`, `dev-fast`), identical output each time; the output printed in the README is that output |
| "**Global variables** (only `const`)" is missing | **still true, kept** | `var G: i32 = 7` at the top level -> *"expected 'fn', 'struct', 'const', 'comptime', 'import', 'export' or 'profile' at top level, found 'var'"*; `static` likewise |
| "a panic handler, run-time checks (overflow, division by zero, index bounds are unchecked)" | **still true, kept and sharpened** | `2147483647 + 1` -> exit 0 (wrapped) at `--opt-level=release-safe` **and** `release-fast`; `a[9]` on `[i32; 4]` -> exit 0, no check; division by zero -> exit **136** = `SIGFPE`, i.e. the processor traps it, not the language; `a +% b` -> parse error |
| "**aarch64**, WASM, an LLVM backend, self-hosting (stages 1-3)" all missing | aarch64 **exists** (round 80, 290/294 byte-identical), self-hosting is **reached** (fixpoint stage 2 == stage 3); WASM and LLVM stay missing | `firnc --target=aarch64-linux --emit=asm` produced real ARM assembly (`bl main`, `mov x8, #93`); `--target=wasm32` -> *"unknown target 'wasm32' (allowed: x86_64-linux, aarch64-linux)"*; fixpoint from `bash test.sh` section 17 |
| "**comptime**, interfaces, optionals" all missing | `comptime` and interfaces **exist**, optionals do **not** | `tests/600_comptime.fi` and a hand-written `interface Sh { fn area(*self) -> int }` with `impl Sh for Sq` compiled and run (exit 36); `-> ?i32` -> *"expected a type, found '?'"* |
| "**`defer`**, **`errdefer`**, **`drop`**, the move checker, arenas/allocators" all missing | `defer`, `errdefer` and the allocators exist; `drop` and the move checker do not | `tests/580_defer.fi` prints `.CBA` (the reverse order) and exits 0, `tests/581_errdefer.fi` exits 0; `drop` is not in `fn keyword` in `compiler/src/lexer.rs`; `Allocator`/`core.Arena`/`mem.PageAllocator` are SPEC 14.1 item 6 |
| "The performance target of <= 2x Rust is missed -- a measured median of **2.8x-3.4x**" | median **2.08x** and **2.19x**, range 1.43x-4.16x -- still missed, but only just, and three of six programs are inside the target | `BENCH_RUNS=9 bash bench/run.sh`, two independent passes; tables in `bench/RESULTS.md` |
| "HTML5 tokenizer ... what stays open is the speed target of <= 2x" | **the target is met on both corpora**: `realweb` **1.18x / 1.17x**, `html5lib` **0.80x / 0.76x** -- Firn is *ahead* of html5ever on the pathological corpus | `bash tools/tokenizer/throughput.sh`, two independent passes, after building `lib/html/tokenize_bench.fi` and `bench/tokenizer` (html5ever 0.27) |
| "The build goes through with **zero warnings**" | **11 warnings**, all dead code or unused imports | `touch compiler/src/main.rs && cargo build --release` -> `generated 11 warnings` |
| "What round 1 criticized and what is fixed now" / "known weaknesses that round 2 did NOT fix" | the whole round-1/2/3 framing is gone; the list is now "today", not "since then" | -- |
| `secret[T]`, `u128`, `#[constant_time]`, `&T`/`inout T`, WASM, LLVM, package registry/lock file, stack probing, debug lines only with `--no-opt`, no vector instructions on aarch64, the 24 h GC run | **all confirmed as still open and kept** | `secret[u8]` -> *"'secret[T]' is not implemented in stage 0"*; `u128` -> *"unknown type"*; `#[constant_time]` -> *"not implemented in stage 0"*; `&i32`/`inout i32` -> parse errors; `grep -ci 'lock\|registry\|http\|download' compiler/src/package.rs compiler/src/package_world.rs` -> **0** and **0**; no `probe`/`guard page` in `compiler/src/codegen_x86.rs`; `compiler/src/codegen_a64.rs:692` says the vector instructions are x86-64 only; SPEC 14.1 item 16; ACCEPTANCE.md item 2 |

## What else changed

**Structure.** The README was **1,571 lines**; it is now **380**. The first
150 lines answer, in this order: what is this, how do I try it, what can it
do, what does it look like, what can it *not* do. Everything after that is
reference.

The long round-by-round module reports were **moved, not deleted**, into
[MODULE_REPORTS.md](MODULE_REPORTS.md) (1,193 lines) with an archive header
that says plainly which of their measurements have since been superseded and
by what.

**A "Try it" section right after the introduction**, three commands that were
really run in this order:

    cargo build --release --manifest-path compiler/Cargo.toml
    export FIRNLIB="$PWD/lib"
    ./compiler/target/release/firnc -o /tmp/tour examples/tour.fi && /tmp/tour

`firnc run` of round 84 is **not** on `main` on this base commit, so the
two-step way is what the README documents. `FIRNLIB` is named explicitly
because without it every `import std.*` fails with *"cannot read
'<dir>/std/io.fi'"* -- that was a real stumbling block and the old README never
mentioned it.

**A link checker.** `tools/mdlinks/check.py` resolves every relative Markdown
link against the directory of the file it stands in, ignoring fenced code
blocks and inline code spans. Result for the whole repository:

    files        : 61
    links local  : 33
    links extern : 0 (not fetched)
    DEAD         : 0

For `README.md` alone: 30 local links, 0 dead. The script is standalone and
deliberately **not** wired into `test.sh`, so the acceptance count does not
move in a round that touched no compiler code.

**One tool change, and only one.** `tools/english/check.sh` was **red on
`main`** before this round started -- the MIT license commit `6cf62949` added
the line `MIT -- see [LICENSE](LICENSE).` to the README, and
`check_comments.py` reads the capitalised licence name `MIT` as the German
preposition `mit`. That made `test.sh` section 21 fail on `main`. The fix is
in the checker, not in the prose: a hit written in capitals is a proper noun
or an abbreviation, never a German function word, so it no longer counts.
`bash tools/english/check.sh` is back to `0 0 0 0 0`.

**`bench/RESULTS.md`** was rewritten by hand rather than by `bench.py`: the
generator overwrites the whole file and would have thrown away the A/B
instruction-count history of round 5. The new round 86 table sits on top, the
round 5 table is kept underneath as "the state this replaced", and the history
below it is untouched. (`bench/bench.py` also writes a few German words into
its output table -- another reason not to let it own the file.)

## The numbers of this round

| what | measured |
|---|---|
| `bash test.sh` | see below |
| `python3 tools/mdlinks/check.py` | 61 files, 33 local links, **0 dead** |
| `BENCH_RUNS=9 bash bench/run.sh` (2x) | median **2.08x** / **2.19x** of `rustc -O` |
| `bash tools/tokenizer/throughput.sh` (2x) | `realweb` **1.18x** / **1.17x**, `html5lib` **0.80x** / **0.76x** |
| `examples/tour.fi` in 3 build levels | identical output, exit 0 each time |
| README | 1,571 -> **380** lines |

## What is left open

* `ACCEPTANCE.md` still carries the header "state of this file: 2026-08-14,
  after the merge of round 3" and the overall result "0 of 6 passed". That is
  round 85's job (`r85-accept`), which is running in parallel; this round only
  links to the file and does not quote its stale numbers.
* `SPEC.md` 14.1 item 2 ("there is **no** default type, `let x = 5` is an
  error") is out of date -- round 70 gave integer literals a default of `i32`.
  Item 3 ("no run-time checks") is correct today but will need striking the
  moment round 83 lands. Neither was touched here: the README round should not
  edit the specification behind another round's back.
* `examples/hello.fi`, `fib.fi` and `structs.fi` still carry **German**
  comments, and `hello.fi` greets in German. `tools/english/check.sh` does not
  reach into `examples/`. Left as it is on purpose -- changing the expected
  output of an example is a test change, not a documentation change.

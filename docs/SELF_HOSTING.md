# Self-hosting: the plan and the honest state

**Requirement:** `L1` · `SPEC.md` §11 (bootstrap plan) · `ACCEPTANCE.md` item 1
**State (round 31): the fixpoint holds.** `firnc1` — the compiler written
in Firn — compiles **itself**, and the result is a fixpoint:
stage 2 (produced by `firnc1`) and stage 3 (produced by stage 2) are
**character-identical**. Proof: `tools/fixpoint.sh`, section 17 of `test.sh`.
The path there is below, round by round, with measurements instead of
claims — §21 is the keystone.

---

## 1. The plan (unchanged from SPEC §11)

| Stage | written in | compiled by | Result |
|---|---|---|---|
| 0 | Rust | `cargo` | `firnc0` — compiles the subset from §12.1 |
| 1 | Firn (subset §12.1) | `firnc0` | `firnc1` |
| 2 | Firn (full extent) | `firnc1` | `firnc2`, after which `firnc2` compiles itself |
| 3 | — | — | fixpoint: `firnc2` and `firnc2'` bit-identical |

The rule stands: **stage 1 may only use what `firnc0` masters.** The
fixpoint comparison is the only dependable statement about correctness.

## 2. The order in which stage 1 would have to come into being

1. **Runtime core in Firn** (`lib/rt/`): an `mmap` allocator (bump + free
   lists), `memcpy`, `memset`, process exit. Only `syscall` is needed —
   that already works today.
2. **Collections** (`lib/std/`): `Vec[T]`, `Map[K,V]`, `Str`/`Bytes`.
   Needs generics (module `types`) and the allocator from 1.
3. **Input/output**: `read`, `write`, `open`, `close`, `execve` as
   Firn wrappers around `syscall`. That already works today, but is
   missing as a library.
4. **Compiler data structures**: tokens, AST, FIR — recursive trees over
   `Vec` indices instead of pointers (see 4.).
5. **Porting in this order**: `config` → `diag` → `lexer` → `ast`
   → `parser` → `types` → `sema` → `fir` → `lower` → `codegen_x86` → `main`.
   The optimizer comes last; `firnc1` may work without it.

## 3. What could **already** be written in Firn today

Everything that gets by with a **fixed** size and needs no heap. Concretely:

* Numeric and bit algorithms: an integer parser, base conversion,
  `wrap`/`lit_fits` from `sema.rs`, alignment/layout computation from
  `types.rs` (`round_up`, `size_of`, `align_of`) — they work on scalars and
  fixed arrays.
* Table-driven recognition: the keyword table of the lexer, the
  operator recognition (`punct`), the register name table of the code
  generator.
* The entire **classification of the calling convention** (`abi.rs`): a
  pure case distinction on type and size.
* State machines without dynamic memory management — exactly for that
  reason the HTML5 tokenizer is the right first hardening test and not the
  compiler.

## 4. What is concretely missing — the list

Sorted by „blocks the most first". `[ ]` = missing,
`[~]` = partly (a module of this round), `[x]` = present.

| # | Feature | State | Why the compiler needs it |
|---|---|---|---|
| 1 | **Heap allocator** (`mmap`-based, `alloc`/`free`) | **`[x]`** `lib/rt/rt.fi` (round 15) | Without it there is no `Vec`, no AST, no symbol table |
| 2 | **`Vec[T]`** (growing array) | **`[x]`** as the library `lib/rt/vec.fi` (round 18, `tests/640_vec_module.fi`) | token stream, statement lists, block lists — everywhere |
| 3 | **Hash map `Map[K,V]`** | **`[x]`** `lib/rt/map.fi` + `lib/rt/intern.fi` (round 19) | name tables (`fns`, `consts`, scopes) |
| 4 | **Strings** `Str`/`Bytes` with concatenation | `[~]` `rt.Buf` + `intern.Interner` (round 19); a `Str` type with a concatenation operator is missing | identifiers, error messages, assembly text |
| 5 | **Text formatting** (a `format` substitute) | **`[~]`** `buf_push_dez_u64/i64`, `buf_push_hex_u64` in `lib/rt/` | every diagnostic and the whole assembly output |
| 6 | **Sum types + `match`** | **`[x]`** without type parameters (`tests/201_enum_payload.fi`); `enum Name[T]` is missing (checked in round 20) | `TokKind`, `ExprKind`, `Op`, `Term` are all sum types |
| 7 | **Recursive data types** (a `Box` substitute) | **`[x]`** via `*mut` to the own type, verified in round 20 | `Expr` contains `Expr`; today only over pointers + allocator |
| 8 | **Methods / `impl`** | `[ ]` | cosmetics, replaceable by free functions with a first parameter |
| 9 | **Interfaces / dynamic dispatch** | `[ ]` | **not** needed for stage 1 |
| 10 | **Error handling** (`Result`, `?`) | `[ ]` | replaceable by a sum type + `match` as soon as 6 stands |
| 11 | **Process start** (a `fork`/`execve` wrapper) | **`[x]`** since round 28 (`rt.run`, `tests/700_process_start.fi`) | `firnc` calls `as` and `ld` |
| 12 | **File access** (`open`/`read`/`write`) | **`[x]`** `lies_datei`, `lies_stdin`, `schreib_alles` in `lib/rt/` | read the source, write the `.s` |
| 13 | **Mutable global state** | `[ ]` (only `const`) | avoidable: pass a context struct through — the Rust code does that almost everywhere already |
| 14 | **Aggregates at function boundaries** | `[x]` since round 2 | structs as parameters/return values |
| 15 | **more than 6 parameters** | `[x]` since round 2 | `emit_inst(e, f, fr, i, …)` |
| 16 | **Module system** | `[x]` since round 2 | the compiler has 24 files |
| 17 | **`for`/`break`/`continue`** | `[x]` since round 2 | every loop in the compiler |
| 18 | **`comptime` code generation** | `[ ]` | only for Unicode tables (`ABNAHME` item 6), not for stage 1 |

## 5. How much of the compiler would be portable today? — measured, not estimated

Measuring method (reproducible):

```console
$ cd compiler/src
$ for f in *.rs; do
    tot=$(grep -c . $f)
    dyn=$(grep -cE "Vec<|String|HashMap|HashSet|Box<|format!|\.push\(|\.clone\(\)|&str" $f)
    echo "$f $tot $dyn"
  done
```

What is counted is how many non-empty lines touch a dynamic data structure
or text formatting — that is, exactly what Firn does **not** have today.

| File | Lines | of those with `Vec`/`String`/`Map`/`Box`/`format!` |
|---|---:|---:|
| `abi.rs` | 113 | 1 |
| `types.rs` | 192 | 19 |
| `lexer.rs` | 514 | 26 |
| `dwarf.rs` | 138 | 16 |
| `codegen_x86.rs` | 569 | 72 |
| `parser.rs` | 1500 | 78 |
| `sema.rs` | 2302 | 153 |
| `lower.rs` | 1419 | 123 |
| … (all 24 files) | **16480** | **1600** |

From that it does **not** follow that „90 % are portable". The dependency is
not line-wise but structural: a single `Vec` in a function makes the
whole function unportable, and the token stream (`Vec<Token>`) runs through
the lexer, the parser and all the tests.

**An honest assessment, file by file:**

| File | portable today? |
|---|---|
| `config.rs`, `abi.rs` | **yes, completely** (226 lines) |
| `types.rs` | yes apart from the `HashMap` of the struct names — portable with a fixed upper bound and linear search (192 lines) |
| `lexer.rs` | the recognition logic yes, the output `Vec<Token>` no → needs items 1+2 |
| `diag.rs`, `parser.rs`, `sema.rs`, `lower.rs`, `opt.rs`, `codegen_x86.rs` | **no**, all of them need a heap, `Vec` and text formatting |

**The number that counts:** what can be written completely in Firn today is
`config.rs` + `abi.rs` + `types.rs` ≈ **418 of 16.480 lines ≈ 2,5 %** of the
compiler. With items 1–5 from the list (allocator, `Vec`, `Map`, `Str`,
formatting) it would be **> 80 %** by our review — these five items
are the whole difference between „does not work" and „works".

## 6. The next step

`lib/rt/alloc.fi` (a bump allocator over `mmap`) and `lib/std/vec.fi` as the
first Firn libraries, both with test programs of their own under `tests/`.
Only after that is the first compiler part in Firn worthwhile — and that is
the **lexer**, because it has the smallest interface (text in, a token
array out).

---

## 6. What round 15 delivered — `lib/rt/`

The three items that blocked the most above (1, 5, 12) now stand as
**one** library in Firn: `lib/rt/rt.fi`.

| Area | Functions |
|---|---|
| memory | `heap_alloc`, `heap_free`, `mem_copy`, `mem_set`, `mem_eq` |
| buffers | `Buf` with `buf_push`, `buf_push_bytes`, `buf_reserve`, `buf_at`, `buf_len` |
| number → text | `buf_push_dez_u64`, `buf_push_dez_i64`, `buf_push_hex_u64` |
| input/output | `read_file`, `read_stdin`, `write_everything`, `finish` |
| raw access | `ld8`/`st8` … `ld64`/`st64` |

Proof: `tests/610_rt.fi` — allocation, 5.000 bytes through several
doublings, formatting in decimal and hex, reading a file and printing it
again; in all three build stages.

**Why that counts:** a compiler has to read its source, build up text and
write a `.s`. Exactly those three things Firn can now do without Rust and
without libc.

**Honestly said about it:**

* The allocator returns memory to the operating system **page-wise** and
  has no free list for small blocks. For one compiler run that is
  fine (arena-like), for a long-running program it is not.
* This library does **not replace the existing ones yet**.
  `lib/html/mem.fi`, `lib/str/alloc.fi` and `lib/gc/gc.fi` still have their
  own, slightly different versions. Merging them is a step of its own
  with a risk of its own — it is outstanding, and this line stays here
  until it is done.
* `Vec[T]` (typed, generic) is still missing. `rt.Buf` is the
  byte version of it.

---

## 7. Round 16/17: `size_of[T]()`, `Vec[T]` — three blockers, two fixed

**Built:** `size_of[T]()` returns the size of a type in bytes at
compile time (`compiler/src/sizeof.rs`, `tests/611_size_of.fi`). With that,
a **growing** `Vec[T]` runs on the heap: `tests/620_vec_heap.fi` creates
1.000 `i32`, 300 `u8` and 100 `u64` — three instantiations, three
element sizes, all three build stages.

Nothing of `size_of` remains at runtime: the type checker computes the
size, and the lowering inserts a constant.

**Three blockers that became visible while building `lib/rt/vec.fi`.** They
are the actual yield of this round — without them no
library collection can be written, and the compiler in stage 1 consists of
nothing else:

| # | Blocker | State |
|---|---|---|
| B1 | a generic template from a module cannot be used | **fixed (round 17)** |
| B2 | a generic template does not see the names of its own module file | **fixed (round 18)** |
| B3 | imports are resolved relative to the root file | **fixed (round 17)** |

### B3 — fixed

`modules::resolve` now looks for an import path **first relative to the file
that writes the import**, and only after that relative to the root. The
fallback stays, so that existing programs run unchanged.

### B1 — fixed

The pre-scan for generic templates (`sema_generic::hook_prescan`) ran per
file **immediately before parsing it**. The root file is parsed first —
so it did not know the templates of the modules yet.
`modules::build_program` now lexes **all files first and pre-scans them**,
and only then parses.

### B2 — fixed

The cause was more tangible than assumed: **generic templates do not lie in
`Program::funcs`** but in `sema_generic::REG`. The module rewriting in
`modules::build_program` runs over `Program::funcs` — so it never reached
the templates. A template therefore saw only the names of the root file;
even a helper function in the same module file reported *unknown
function*.

`build_program` now sends **the templates of the respective file as well**
through the same `Renamer`. The **name** of the template stays untouched in
the process: the instantiation looks it up later under the original name
(`mono::expand_fn` via `Instantiation::base`), and generic names are valid
program-wide.

### What became possible with that

`lib/rt/vec.fi` is the first real **generic collection as a library**:
it includes `rt` from its own directory (B3), calls
`rt.heap_alloc`/`rt.mem_copy` from the body of a template (B2), and the
root file writes `var v: Vec[i32] = vec_new[i32]()` (B1). The duplication of
the memory functions has disappeared again.

Proof: `tests/640_vec_module.fi` (1.000 `i32`, 300 `u8`, 100 `u64`, `pop`,
`vec_set`, access beyond the end) in all three build stages.

**With that `lib/std/` can be written** — the next step on the list in §2.
---

## 9. Round 19: `Map[K, V]` and the `Interner` — item 3 is done

**Built:** `lib/rt/map.fi` (a hash map) and `lib/rt/intern.fi`
(string → number). Both as a **library**, not in the root file —
that is only possible since B1–B3 fell (§7, §8).

### Why two building blocks and not one

`Map[K, V]` has **scalar** keys. A name table of the compiler, however, has
identifiers as keys. The `Interner` closes the gap: it hands out a `u32` per
identifier, and after that everything computes with numbers. The Rust
compiler does it no differently. Two gains beyond „it works too": comparing
two identifiers is a `u32` comparison instead of a byte loop, and every
identifier lies in memory exactly once.

### `Map[K, V]` — open addressing

Three separate arrays (keys, values, state) instead of one array of
pairs: that saves the alignment holes with unequal sizes, and the
probing loop reads only one byte per step. The capacity is always a
power of two (`hash & (kap-1)` instead of a division), the load limit 3/4.

The state per slot: **0 = empty, 1 = occupied, 2 = deleted.** The difference
between 0 and 2 is the core: a search must **not** stop at 2, otherwise it
loses everything that lies behind a deletion. Tombstones count towards the
load limit — otherwise a table into which things are constantly inserted and
deleted degenerates into a linear search.

### The hashing — measured instead of claimed

The obvious question: does a table with consecutive internal numbers need a
hashing round at all? Measured (20.000 keys, 32.768 slots, load 0,61;
mean probing steps per insertion / longest chain):

| Key pattern | with hashing | without (key = slot) |
|---|---|---|
| consecutive `k` | 1,80 / 38 | **1,00 / 1** |
| `k * 1024` (pointers) | 1,76 / 37 | 313,00 / 625 |
| `k * 65536` | 1,62 / 39 | 10000,50 / 20000 |
| `k << 32` (only high bits) | 1,80 / 53 | 10000,50 / 20000 |

So the answer is **not** „hashing is always better" — in the ideal case it
costs 0,8 probing steps. What it buys is that keys which differ only
in high bits (pointers, aligned addresses, shifted
numbers) do not throw the table back to a **linear search**. 0,8 against
10.000 is not a close call.

### What the map really brings

The same task — create 20.000 entries, 100.000 lookups — once over
`Map[u32, i64]` and once over a linear search in two `Vec`:

| | `Map` | linear search in a `Vec` |
|---|---|---|
| wall clock | **0,005 s** | 2,238 s |

A factor of **447**. For the name resolver of the compiler that is the
difference between „runs" and „does not run".

### `Interner` — collisions handled correctly, not merely improbably

All bytes one after another in an `rt.Buf`, the offset and length per number
in two `Vec[u32]`, plus a probing table of its own made of `u32`. When
probing, the **text** is compared, not the hash value — so two different
identifiers with the same FNV value get different numbers. That is the
difference between correct and „has not come up so far".

One trap lies in the relocation: `intern_number` has to take the hash from
its **own** buffer after the copying — the passed pointer may have pointed
into the same buffer and may have become invalid when it grew.

Proof: `tests/650_map_module.fi` (1.000 entries over several doublings,
overwriting, deleting **and searching behind the tombstone**, reinsertion,
iteration, three instantiations with unequal sizes, negative keys,
`map_reserve` without rehashing) and `tests/651_intern_module.fi` (the
prefixes „ab" vs. „abc", the empty string, 2.000 generated identifiers with
a relocating buffer, a number as a `Map` key) — both in all three build
stages.

### Honest limits

* **No removal in the `Interner`.** It only grows. Right for a compiler
  run, not for a long-running program.
* `intern_zeiger` is valid only **until the next `intern_nummer`** — the
  text buffer may relocate. Whoever wants to keep the pointer copies.
* `map_hol` returns `0 as V` for a missing key. Whoever has to distinguish
  0 from „missing" needs `map_hat` beforehand. The clean way would be
  `Option[V]` — that presupposes item 6 (sum types).
* A `Str` type with a concatenation operator is still missing (item 4 stays
  `[~]`).

### State of the list after this round

| Item | State |
|---|---|
| 1 heap allocator | ✅ |
| 2 `Vec[T]` | ✅ |
| 3 `Map[K,V]` | ✅ |
| 4 `Str`/`Bytes` | `[~]` |
| 5 text formatting | `[~]` |
| 12 file access | ✅ |

**Next blockers:** sum types with a payload + `match` (item 6) and
recursive data types (item 7) — together they are the AST.


---

## 10. Round 20: the lexer of Firn, written in Firn

**Stage 1 has begun.** `lib/firnc1/lexer.fi` is the first compiler part in
Firn — 1.009 lines on top of `rt`, `Vec[T]` and the `Interner`. §2 names the
lexer as the right beginning, because it has the smallest interface: text
in, a token array out.

### The yardstick comes from outside

A lexer cannot be checked against itself. `bin/lexdump.fi` writes
the token stream in **exactly** the format of `firnc0 --emit=tokens`;
`tools/lex_compare.sh` runs both over `tests/`, `lib/`, `bin/` and
`bench/` and compares octet by octet.

| | |
|---|---:|
| files octet-identical | **294** |
| files differing | 1 (named, see below) |
| skipped (`firnc0` itself reports an error there) | 2 |
| tokens compared | **211.405** |

That runs along as section 11 in `test.sh` with every change.

### What that found: a real bug in stage 0

The lexer written in Firn read `10.0` and got a different bit pattern than
`firnc0` — but **only with the optimizer**. The cause: `f64` is 64 bits wide
and counts as unsigned in the FIR. Two places concluded from that
that `u64 -> f64` was a pure reinterpretation of the same bit pattern:

* `mem2reg.rs` deleted the conversion **without replacement**,
* `opt.rs::fold_cast` folded it into a pure bit operation.

`100 as f64` thereby became the bit pattern 100 — i.e. `5e-322` instead of
`100.0`. It is exactly the other way round: of all conversions, the one
between integer and floating point is the **only** one that really changes
the bits (`cvtsi2sd`). The path without the optimizer was right the whole
time.

Both places are fixed; `fold_cast` now computes the conversion **for real**
(and folds `f64 -> integer` too, except for NaN, infinity and values
outside the target range). Regression test:
`tests/591_f64_conversion.fi`, which runs with **and** without the optimizer
like every positive test.

That is the actual yield of this round. A bootstrap is no
exercise in diligence — it is a **test bench**, because two independent
implementations stand against each other. The bug had been in the tree since
round 14 and had slipped through 590 tests.

### How the lexer is built

* **Struct of arrays instead of an array of structs.** `Vec[T]` demands
  `T: Scalar`, and a token is a compound — so the kind, line, column,
  length, numeric value and number lie in six equally long `Vec`. The parser
  almost always reads only the kind, and that way it lies close together.
* **Characters versus octets.** `firnc0` works on `Vec<char>`, and columns
  count characters. The Firn lexer works on octets and counts a column only
  at the **leading** octet of a UTF-8 character. Without that, all columns
  behind an umlaut in the same line shift — and exactly that is checked by
  the comparison, because `tests/570_string_literals.fi` contains umlauts.
* **Word tables instead of global state.** Firn has no mutable
  global state (item 13). The keyword and name tables are therefore built
  from **one** literal that is split at `|`.
* Identifiers go through the `Interner` from round 19; string literals
  are decoded completely, including `\x`, `\u{...}`, surrogate pairs in
  `Str` and unpaired surrogates in `Str16`.

### The one deviation — named, not cleared away

`tests/590_f64.fi`, the literal **`1e308`**. The Firn lexer uses the fast
path of Clinger: if the mantissa fits into 2^53 and the decimal exponent
lies between -22 and 22, then a single multiplication in binary64 is
correctly rounded. The extension upwards (pulling the excess into the
mantissa) covers exponents up to 37. Above that, multiplication proceeds
step by step, and that is off by **one ULP**.

Measured: of 211.405 tokens in the corpus, **exactly one** needs the slow
path. The correct approach would be Eisel-Lemire with 128-bit arithmetic —
that is still missing, and the line stays in `tools/lex_compare.sh` until it
is there.

### Further honest limits

* **The diagnostics are not ported.** The Firn lexer counts errors, it does
  not report them with a line and a text. That is the module `diag` and a
  step of its own. That is why the comparison skips the two files at which
  `firnc0` itself reports an error.
* **Unicode whitespace** (U+00A0, U+2028 …) counts as whitespace in
  `firnc0`, in the Firn lexer it does not. None occurs in the whole corpus;
  the difference is named, not fixed.
* The lexer does **not** produce an AST yet — the parser is the next step.

### Side findings on the list in §4

* Item 6 (**sum types**) is **completely there** without type parameters —
  `enum Value { Nothing, Number(i32), Pair(i32, i32) }` with binding in the
  pattern has run since round 2. What is missing is `enum Name[T]`: the
  parser knows no type parameter list behind an enum name (`Option[T]`,
  `Result[T,E]`).
* Item 7 (**recursive data types**) runs over `*mut` to the own type:
  `enum Ausdruck { Zahl(i64), Plus(*mut Ausdruck, *mut Ausdruck) }` compiles
  and computes. A `Box` type with a release of its own is missing — the
  trees of the compiler are built over `Vec` indices anyway (§2.4).

### State of the list

| Item | State |
|---|---|
| 1 heap allocator · 2 `Vec[T]` · 3 `Map[K,V]` | ✅ |
| 6 sum types + `match` | ✅ (without type parameters) |
| 7 recursive data types | ✅ (over pointers) |
| 12 file access | ✅ |
| 4 `Str`/`Bytes` · 5 text formatting | `[~]` |
| 8 methods · 9 interfaces · 10 `Result`/`?` · 11 `fork`/`execve` · 13 global state | `[ ]` |

**Next step:** `config` and `diag` in Firn — and after that the parser. For
`firnc1` as a standalone program, item 11 (`fork`/`execve`) is missing,
because `firnc` calls `as` and `ld`. That is the only item of the list for
which there is no detour.


---

## 11. Round 21: `diag` in Firn — and the proof over the error output

`lib/firnc1/diag.fi` is the second compiler part in Firn. The lexer no
longer **counts** errors, it **reports** them — with the file, line, column,
source line and marker, in the binding format from `diag.rs`:

```text
error: in a string literal: \u{...} is not terminated
  --> tests/lexneg/u_escape.fi:3:23
   |
 3 |     var b: [u8; 4] = "\u{}"
   |                       ^ here
4 Fehler gefunden
```

### The comparison now checks both streams

`tools/lex_compare.sh` no longer compares the token stream alone but the
**error output** as well -- octet by octet against `firnc0 --emit=tokens`.

| | |
|---|---:|
| files identical (both streams) | **306** |
| of those with diagnostics | **10** |
| tokens compared | **216.489** |
| differing | 1 (`1e308`, named) |
| skipped (a module fragment) | 2 |

The ten cases in `tests/lexneg/` cover every message the lexer
can produce: an unknown character, a number that is too large, an invalid
digit for the base, an unclosed block comment, an unclosed literal, an
unknown escape, `\x` without two digits, `\u{...}` in all four error forms,
an unpaired surrogate, `\u` in `b"..."`, non-ASCII in `b"..."`, `\xFF` in
`"..."` and an empty exponent.

### What the language was missing for that: call arguments

A diagnostic contains the **file name** — so the program has to be able to
receive it. Firn could not do that. Now there is a second
permitted form of the entry point:

```firn
fn main(start: u64) -> i32
```

At process start, `rsp` points at `[argc][argv0]..[argvN][0][envp..]`;
`_start` puts this pointer into `rdi`, i.e. into the first parameter. A
program with `fn main() -> i32` notices nothing of it — it never reads
`rdi`. `lib/rt/rt.fi` additionally gets `arg_anzahl`, `arg_zeiger` and
`c_laenge`; the argv strings are null-terminated and are therefore exactly
what `lies_datei` expects. Proof: `tests/660_args.fi`.

That is the first part of item 11 of the list. `fork`/`execve` are still
missing — without them `firnc1` cannot call `as` and `ld`.

### How `diag` is built

The same form as in the lexer: a **struct of arrays**. A diagnostic consists
of four numbers (file, line, column, length) in `Vec[u32]` and three pieces
of text (message, marker text, hint) as an offset/length in **one** buffer.

Two things that were important while rebuilding it and that are easily
overlooked:

* **The marker offset counts characters, not octets** — and a tabulator
  counts as four. Without both, the caret stands in the wrong place behind
  an umlaut or behind a tabulator.
* **Duplicate messages at the same place are suppressed.** Without that,
  error recovery produces rows of identical lines — and the
  comparison with `firnc0` falls over immediately.

Firn has no string concatenation (item 4 of the list). The messages are
therefore built in an `rt.Buf`: **the buffer is the concatenation.** Numbers
come in over `buf_push_dez_u64`, a character of the source is copied octet
by octet, and for `U+{:04X}` there is a small hex output of its own next to
it.

### Honest limits

* So far `diag` can only do what the lexer needs: `error` and `error_note`
  with the marker „hier". Freely chosen marker texts, several markers
  per diagnostic and colored output do not exist — `diag.rs` does not have
  them either.
* The upper limit of 40 messages is taken over but not checked: no
  test case in the corpus produces that many lexing errors.
* `config` is **not** ported. It is three constants and one function;
  without string concatenation, `compiler_name()` would be more effort than
  benefit. The item stays open until `Str` stands.

### State

| Part | State |
|---|---|
| `lexer` | ✅ in Firn, checked against `firnc0` |
| `diag` | ✅ in Firn, error output checked against `firnc0` |
| `config` | `[ ]` (needs `Str`) |
| `ast`, `parser`, … | `[ ]` |

**Next step:** the parser. It needs an AST — and thereby, for the first
time, recursive data structures over `Vec` indices, exactly as planned in
§2.4.


---

## 12. Round 22: the parser of the core language in Firn

`lib/firnc1/ast.fi` and `lib/firnc1/parser.fi` are the third and fourth part
of stage 1. With that, the chain **text → tokens → tree** is completely
written in Firn.

### The tree lies in `Vec`, not in pointers

Since its first version, §2.4 has said how the AST has to be built:
recursive trees over `Vec` indices. `ast.Baum` holds thirty-three `Vec` with
fixed slots per node kind:

| | |
|---|---|
| expression | `e_art` · `e_a` · `e_b` · `e_c` · `e_zahl` |
| statement | `s_art` · `s_a` · `s_b` · `s_c` · `s_d` |
| type | `t_art` · `t_a` · `t_b` |
| block | `b_off` · `b_len` → a range in `sliste` |

Child lists of variable length (call arguments, array elements,
struct fields) lie one after another in **one** `Vec`; the node remembers
the offset and the count. No allocator per node, no release order, no
destructor — `baum_frei` releases everything in one go.

### The bug that arose from exactly that

The obvious way — appending every argument to `kinder` immediately — is
**wrong** as soon as an argument itself contains a call: its arguments
then end up in the middle of the outer list. From

```firn
rt.buf_push_bytes(b, intern.intern_zeiger(it, nr), intern.intern_laenge(it, nr))
```

came a call with **seven** arguments instead of three. The comparison with
`firnc0` found that in the first round, at exactly the file that prints the
comparison itself. The child list is now collected first and then deposited
in one piece.

### The yardstick: `--emit=ast-kanon`

`--emit=ast` is Rust's `{:#?}` — tied to `Box`, `Some`/`None` and field
names. A parser in another language cannot rebuild that without
aping Rust's debug output; then the comparison checks the formatting
instead of the tree. `compiler/src/ast_canon.rs` therefore produces a
**language-neutral** parenthesized form:

```text
(fn u64_nach_f64 ((param m u64)) f64 (blk (ret (as (id m) f64))))
```

What is printed is **only the root file**, before merging the modules and
before monomorphization — the parser in Firn likewise sees exactly one
file.

### Result

`tools/parser_compare.sh`, section 12 in `test.sh`:

| | |
|---|---:|
| trees identical | **166** |
| differing | 1 (`1e308`, known from round 20 — a **lexer** case) |
| not core language | 109 |
| skipped (`firnc0` does not get through itself) | 36 |

### What the parser CANNOT do — counted, not passed over

Everything that keeps its tree outside `Program`: `enum`/`match`,
error unions (`E!T`, `try`, `catch`), generic templates, `gc class`,
attributes, `comptime`. A pre-scan in the token stream recognizes these
files by their pattern (`::`, `?`, `IDENT !`, `fn name[`, the identifier
`gc`) and reports return value 3 — they count as **109 „not core"** and do
not disappear silently into a success number.

Likewise not ported: **error recovery**. `firnc0` collects up
to forty messages and reads on; this parser reports the first one and stops.
Irrelevant for the comparison — it only runs over sources that `firnc0`
reads without errors — but not for a real compiler.

And: **source positions are not in the canonical form.** They belong to the
tree, but the way they are composed (`Parser::join` over subexpressions) is
an agreement of its own. The tree is right; whether every span is right has
not been checked yet.

### A rule that is easily overlooked

An operator at the **beginning of a line** does not continue the expression
as long as no parenthesis is open. That is why

```firn
a
- b
```

is two statements and not a subtraction. The same rule had already caught me
while writing `diag.fi` (round 21, a multi-line
`&&` condition); in the parser it now stands as `weiter()`.

### State

| Part | State |
|---|---|
| `lexer` | ✅ in Firn, checked against `firnc0` |
| `diag` | ✅ in Firn, error output checked |
| `ast` + `parser` | ✅ core language, 166 trees identical |
| `config` | `[ ]` (needs `Str`) |
| `types`, `sema`, `fir`, `lower`, `codegen` | `[ ]` |

**Next step:** `types` and `abi` — §3 names them as the files that
would already be completely writable in Firn today (a pure case distinction
on type and size). After that it gets serious: `sema`.


---

## 13. Round 23: `types` and `abi` in Firn — layout and calling convention

Since its first version, §3 has named `types.rs` and `abi.rs` as the files
that **would already be completely writable in Firn today**: they compute
only on the type and the size, without a heap and without text.
`lib/firnc1/types.fi` redeems that.

### Why these two of all things next

Layout and calling convention are the places at which a compiler goes
**silently wrong**. One field offset off, an aggregate in registers instead
of in memory — the program runs, only wrongly, and the bug shows up
somewhere completely different. Putting two independent implementations
against each other finds more here than any invented test case.

### The yardstick: `--emit=layout`

`compiler/src/layout_canon.rs` prints, per struct, the size, the alignment
and **every field offset**, and per function the System V class of every
argument and of the return value including `sret`:
```text
(struct Loecher groesse 32 ausrichtung 8 (feld a versatz 0 …) (feld b versatz 8 …) …)
(fn summe_gross (arg Gross groesse 24 klasse mem) (ret i64 groesse 8 klasse int1 sret 0))
```

Only the root file is resolved. A name that does not exist there (say
`rt.Buf` from another module) becomes `?` with size
0 and alignment 1 on **both** sides — otherwise the comparison would fail at
an artificial uncertainty instead of at a real difference.

### Result

`tools/types_compare.sh`, section 13 in `test.sh`:

| | |
|---|---:|
| layouts identical | **169** |
| differing | **0** |
| of those with real structs | 35 |

Zero deviations in the first run. That is unusual and has a reason:
the rules are short and are written out in SPEC §11 and §13 — unlike
with the parser, where a hundred small agreements come together.

### `tests/670_layout_abi.fi` — the forms the corpus did not have

The existing corpus checks the layout only in passing. The new test checks
it **against itself**:

* **Field offsets recomputed at runtime**, over address differences instead
  of over a table in one's head. That way, not only the computation but the
  *generated code* hangs on the same numbers.
* **All three ABI boundaries**: up to 8 bytes (one word), 9..16 bytes (two
  words — but the return already over the hidden pointer, SPEC §14.1), above
  16 bytes (memory).
* A struct in a struct, an array in a struct, holes from alignment, nine
  words of arguments (the last ones go over the stack), and copy semantics
  on pass by value.

Runs with and without the optimizer, and both layout outputs agree.

### Honest limits

* `types.fi` only resolves what the **core language** knows. The `enum`
  layout (tag + overlaid variants), error unions and `gc class` are missing
  — they belong to the 109 files that the parser does not read anyway.
* There is **no type checking**. What is here is layout and ABI, not
  `sema`: no assignability, no literal inference, no error messages
  about types. That is the next and by far the biggest chunk.
* The class `Sse` is missing just as in `abi.rs` — stage 0 passes `f64` in
  integer registers as well. That is a known deviation from System V and is
  stated as such in SPEC §14.1.

### State

| Part | State |
|---|---|
| `lexer` · `diag` · `ast` + `parser` | ✅ in Firn, checked against `firnc0` |
| `types` + `abi` | ✅ in Firn, 169 layouts identical |
| `config` | `[ ]` (needs `Str`) |
| `sema` | `[ ]` ← the next and biggest step |
| `fir`, `lower`, `codegen_x86` | `[ ]` |

**Next step:** `sema` — name resolution, type checking, literal inference.
All the building blocks are ready for it now: `Map[K,V]` for the scope
chains, the `Interner` for the names, `diag` for the messages and `typen`
for the types themselves.


---

## 14. Round 24: the type checker in Firn

`lib/firnc1/sema.fi` is the sixth part of stage 1 and the biggest so far.
With that, the chain **text → tokens → tree → types** is completely written
in Firn.

### What exactly is checked

`firnc0` makes a guarantee: after the check, **every** expression has a
concrete type — never `UntypedInt`, never `Error`. Exactly this guarantee is
the yardstick. `--emit=typen` prints the canonical tree from round 22 with
the type at every expression:

```text
(let summe i32 (bin + (id a :i32) (int 1 :i32) :i32))
```

The `1` here is an `i32`, although `i32` stands nowhere next to it. That is
the core:

### Bidirectional, and the order is a contract

An integer literal has **no type of its own** in Firn. It comes either from
the context or from the other operand. For that there are two ways through
the same tree:

* `probe` determines the type **without** reporting and without writing to
  the table — so that `a + 1` gains the type of the literal from `a`;
* `ausdruck` really checks and enters the type.

The order is fixed: **first the already typed operand, then the
context hint.** Otherwise `let x: i64 = a + 1` (with `a: i32`) reports a
confusing operand error instead of the real error at the assignment.

### Result

`tools/sema_compare.sh`, section 14 in `test.sh`:

| | |
|---|---:|
| files with an identical type table | **113** |
| expressions compared in the process | **24.529** |
| differing | 1 (`1e308`, known from round 20 — a *lexer* case) |
| not core language | 36 |
| `comptime` evaluation needed | 1 |
| skipped (`firnc0` does not check the file individually) | 168 |

24.529 expressions, each with the same type as in `firnc0`. The 168
skipped files almost all include a module — considered individually,
their names are unknown, and then `firnc0` does not check either.

### `tests/680_typableitung.fi`

The new test covers the corners the corpus did not have: the type from the
other operand, the left operand of a shift, an argument type as a
hint, an index always `usize`, nested struct literals, a pointer to a
pointer, a `for` range with two bounds, shadowing in an inner block, and
a repetition literal whose length comes from a constant expression.

In the process a limit of stage 0 came to light that had been written down
nowhere before: **the length in an array type has to be a literal**
(`[u8; 12]`), whereas a constant expression is permitted in a repetition
literal (`[0 as u8; FLAECHE]`).

### What is NOT ported — named, not concealed

* **All the error messages.** The Firn version *counts* errors, it does not
  describe them. The comparison runs only over sources that `firnc0`
  checks without errors; there the number is zero, and what counts are the
  types. For a real compiler that is too little — `diag` stands ready, the
  message texts are missing.
* **Reachability analysis** (`return` at the end of every path),
  the **mutability check** (`let` against `var`) and the
  **recursion check** for structs.
* **`comptime` evaluation** of constant expressions: a call in a `const`
  is executed by `firnc0` at compile time. Such files yield
  return value 4 and are counted separately.
* The **intrinsics for constant runtime** (`select`, `barrier`,
  `secure_zero`) are ordinary identifiers, not a keyword — they can only
  be recognized by their name. Whoever writes a function `select` of their
  own therefore drops out of the comparison; that is the cautious
  direction.

### An incident that belongs here

While creating the symlinks in `lib/firnc1/`, a wrongly written
`ln -sf` call replaced four real files with links to themselves.
`ast.fi` and `types.fi` came back from git, `sema.fi` and `print.fi`
had not been checked in yet and had to be rewritten. The lesson, without
embellishment: **commit before every bulk command on directories.** The
loss cost half a round.

### State

| Part | State |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` | ✅ |
| `sema` | ✅ core: names, scopes, types — without message texts |
| `config` | `[ ]` (needs `Str`) |
| `fir`, `lower`, `codegen_x86` | `[ ]` |

**Next step:** `fir` and `lower` — from the checked tree to the
intermediate representation. The yardstick is already at hand:
`firnc0 --emit=fir-raw` prints the FIR before any optimization.


---

## 15. Round 25: `fir` and `lower` — the intermediate representation in Firn

`lib/firnc1/fir.fi` and `lib/firnc1/lower.fi` are the seventh and eighth
part of stage 1. With that the chain in Firn reaches from **text to the
intermediate representation**: tokens → tree → types → **FIR**.

### The sharpest comparison of the whole series

`firnc0 --emit=fir-raw` prints the FIR **directly after the lowering**,
before any optimization. What is compared is the text octet by octet — and
it contains everything that matters: value numbers, block numbers, the order
of the instructions, terminators. Two value numbers in a different order,
one block too many or too few, and the text no longer matches.

| | |
|---|---:|
| files with identical FIR | **66** |
| instructions compared | **2.129** |
| differing | 1 (`1e308`, known from round 20) |
| aggregates or `defer` (not ported) | 48 |
| not core language | 35 |
| skipped (`firnc0` does not compile individually) | 172 |

### Two things one cannot guess

**The value numbers follow the creation, the printed order does not.** Every
`alloca` migrates to the beginning of the entry block, but its number stays
where it came into being. That is why `main` contains

```text
%0 = alloca.ptr size=4 align=4
%2 = alloca.ptr size=4 align=4
%1 = const.i32 0
```

— `%1` came into being between the two allocas and is printed after them
nonetheless. Whoever simply creates the allocas at the front gets different
numbers.

**After `return`, `break` and `continue` a new, unreachable block begins.**
It stays in the text (`bb7`, `bb8` …); dead code elimination
removes it only later. Whoever leaves it out gets a different text.

### Three bugs that the comparison found immediately

* **Argument lists slid into each other.** `zwei(zwei(1,2), zwei(3,4))` —
  the arguments of the inner call ended up in the middle of the outer one's
  list. The same bug as with the parser in round 22, from the same cause: a
  shared child list that is written to while collecting.
  Again it holds: collect first, then deposit in one piece.
* **A call without a return value must not define a value.** `call.void`
  stands there without `%n =`; whoever hands out a number anyway shifts all
  the following ones.
* **`const.u64 18446744073709551615` became `const.u64 -1`.** In the Rust
  compiler there is an `i128` there, which prints the value as it is.
  Signedness belongs only to signed types.

### What is NOT ported — counted, not passed over

* **Aggregates**: structs and arrays as values, as arguments, as return
  values, and their literals. That hangs on the calling convention (words
  versus memory, `sret`) and on `write_into` — a step of its own.
  **48 files** drop out because of it, and that is by far the biggest open
  item.
* **`defer` and `errdefer`**: they need a stack per block level and
  have to run at the right depth on `return`, `break` and `continue`.
* The line table for `.debug_line` (`dwarf.rs`) — it does not change the FIR
  text, but it belongs to the lowering.

### `tests/690_lowering_core.fi`

Covers the forms on which numbering and block formation depend:
nested calls, a call without a return value, short-circuiting with `&&`/`||`,
`while` with `break` and `continue`, `for` with `continue` (the increment
block still has to run), a shift with a right operand of a different width,
the largest `u64` value and a pointer to a pointer.

### State

| Part | State |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` | ✅ |
| `fir` | ✅ structure and textual form |
| `lower` | ✅ scalar core · `[ ]` aggregates, `defer` |
| `config` | `[ ]` (needs `Str`) |
| `codegen_x86` | `[ ]` |

**Next step:** aggregates in the lowering — that unlocks the 48 files and
is the precondition for `firnc1` being able to compile itself.
After that the code generator remains.


---

## 16. Round 26: aggregates in the lowering

The biggest open item from round 25 is closed. `lib/firnc1/lower.fi` now
lowers structs and arrays as well -- as a variable, as an argument, as a
return value and as a literal.

| | round 25 | round 26 |
|---|---:|---:|
| files with identical FIR | 66 | **113** |
| instructions compared | 2.129 | **36.217** |
| excluded because of aggregates | 48 | **0** |
| excluded because of `defer` | — | 1 |

From 2.129 to 36.217 compared instructions: not because more files were
added, but because the *large* ones were added — the HTML5 tokenizer, the
DOM endurance run, the comparison tools themselves.

### Aggregates never move as a value

There are exactly two ways, and both work with **addresses**:

* `schreib_nach(adr, e)` deposits an expression at an address. A
  struct literal is written **field by field**, an array literal
  **element by element**, and a foreign aggregate is copied with `copymem`.
* `adresse(e)` obtains the address of an existing aggregate. For a
  literal or a call an intermediate slot comes into being in the process.

### At the function boundary `abi` decides

| Size | Way |
|---|---|
| up to 16 bytes | one or two **integer words** |
| above that | a hidden **pointer to a copy** made by the caller |
| a return above 8 bytes | a hidden pointer in `rdi` (`sret`) |

Loading into words holds a trap that `types.rs` already names and that had to
be rebuilt here: **if the size is not a multiple of eight, it goes over a
padded intermediate buffer** — otherwise the last `load` would lie partly
*behind* the object.

### Three deviations that the comparison found

* **Syscall arguments**: *every* one goes in as an `i64`, a pointer as well.
  I was missing exactly one `cast.ptr.i64` — and with that all the following
  value numbers shifted.
* **The repetition literal as a loop** loads the index **once** and
  uses it for the element address *and* for the increment. A second
  `load` is one instruction too many.
* The element address brings the index to `u64` **first** and computes only
  after that — the order is stated in `layout.rs` and is a contract.

### What is still missing

**`defer` and `errdefer`.** They need a stack per block level and have to
run at the right depth on `return`, `break` and `continue` (SPEC
§5.1). Exactly **one** file in the comparable corpus drops out because of
it.

### State

| Part | State |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` · `fir` | ✅ |
| `lower` | ✅ scalars **and** aggregates · `[ ]` `defer` |
| `config` | `[ ]` (needs `Str`) |
| `codegen_x86` | `[ ]` ← the last big step |

**Next step:** the code generator. From the FIR to x86-64 assembly — and
thereby, for the first time, a program that `firnc1` has compiled itself
from front to back.


---

## 17. Round 27: the code generator — the first run from front to back

`lib/firnc1/codegen.fi` produces x86-64 assembly from the FIR, and
`bin/firnc1.fi` hangs the whole chain together:

```text
Text → Token → Baum → Typen → FIR → Assembler
```

**109 test programs were compiled completely by the Firn compiler,
assembled, linked, executed — and behave exactly like those produced by
`firnc0`.** The same return value, the same output, zero
deviations.

### Deliberately without register allocation

`firnc0` has `regalloc.rs` for that, with liveness analysis and coalescing.
`codegen.fi` does not have it: **every FIR value gets a slot in the frame**,
and every instruction loads its operands, computes and writes back. The
generated code is considerably slower — but it is correct, and it fits into
an afternoon.

That changes the yardstick, and for the better: **what is compared is not
the assembly text but the behavior.** Two code generators with
different register allocation *cannot* produce the same text —
and they do not have to. For a code generator, „the program does
the same thing" is the more honest question anyway than „the characters
agree".

### Frame and calling convention

`[rbp - 8*(v+1)]` is the slot of the value `%v`; behind it lie the areas
of the `alloca`s. A value always lies there **as full 64 bits**, suitably
sign- or zero-extended — that is why a normalization runs after every
computation. The special case in it: `movzx r64, r/m32` does not exist, and
a `mov exx, exx` zeroes the upper 32 bits by itself.

Arguments go according to System V into `rdi, rsi, rdx, rcx, r8, r9`; a
syscall takes the number in `rax` and after that `rdi, rsi, rdx, r10, r8,
r9`. `copymem` becomes `rep movsb`.

### Result

`tools/self_compare.sh`, section 16 in `test.sh`:

| | |
|---|---:|
| **identical behavior** | **109** |
| differing | **0** |
| failing | **0** |
| not core language | 35 |
| `defer` | 1 |
| code generator missing (floating point, > 6 arguments) | 5 |
| skipped (`firnc0` does not compile individually) | 38 |

### What is still missing

* **Floating point.** It needs the SSE registers and a classification of its
  own; `firnc0` treats `f64` in stage 0 differently from System V anyway
  (SPEC §14.1). Five files drop out because of it.
* **More than six arguments** per call — the further ones go over the stack.
* **`fork`/`execve`** (item 11): `as` and `ld` are still called by the
  script, not by `firnc1`. Without that there is no standalone `firnc1`
  program.
* **The module system**: `firnc1` reads exactly one file. For the fixpoint
  it would have to resolve `import` — and the compiler itself consists of
  many files.

### State of the chain

| Part | State |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` · `fir` · `lower` | ✅ |
| `codegen_x86` | ✅ without register allocation and without floating point |
| `config` · module system · `fork`/`execve` | `[ ]` |

**The path to the fixpoint (SPEC §11, stage 2/3) is thereby visible for the
first time:** the module system, process start, then `firnc1` on itself.
What already holds today: **109 programs that no Rust has touched.**


---

## 18. Round 28: process start — item 11 is closed

`firnc1` now calls `as` and `ld` **itself**. With that the last item of the
list in §4 is done for which there was no detour.

```console
$ ./.firnc1 quelle.fi -o programm
$ ./programm
```

There is no script in between any more: `bin/firnc1.fi` writes `programm.s`,
starts `/usr/bin/as` and `/usr/bin/ld` over `fork`/`execve` and waits with
`wait4` for the exit status.

### The one line one must not forget

```firn
if kind == 0 {
    syscall(SYS_EXECVE, path as i64, argv as i64, 0, 0, 0, 0)
    finish(127)          // <- without this the compiler runs twice
}
```

If `execve` returns, it has **failed** -- and then the child runs on in the
program of the parent. Without the `finish`, a missing `as` would make the
whole compiler run a second time. `tests/700_process_start.fi`
checks exactly this case with a path that does not exist.

Added to that was `rt.schreib_datei` (`open` with
`O_WRONLY|O_CREAT|O_TRUNC`, permissions 0755) — the compiler has to put its
output somewhere, after all.

### What the proof really shows now

`tools/self_compare.sh` **calls no tool itself any more**. It starts
`firnc1`, and everything else happens in Firn:

| | |
|---|---:|
| **behaves like `firnc0`** | **109** |
| differing · failing | **0** · **0** |

### State of the list in §4

Of the eighteen items, the open ones are: **4** (`Str` with concatenation),
**5** (text formatting, partly), **8** (methods), **9** (interfaces),
**10** (`Result`/`?`), **13** (mutable global state) and **18**
(`comptime` code generation). None of them stands in the way of the
fixpoint — they are convenience or belong to extensions that `firnc1` does
not read.

**What does stand in the way of the fixpoint is something else:** `firnc1`
reads exactly **one** file. The compiler itself consists of fourteen. The
module system is the next and second to last step.


---

## 19. Round 29: the module system — `firnc1` reads more than one file

`firnc1` now resolves `import` itself. With that, the compiler written in
Firn compiles programs made of several files as well -- including
`tests/610_rt.fi`, which **includes the runtime core `lib/rt/rt.fi`
itself**.

| | round 28 | round 29 |
|---|---:|---:|
| behaves like `firnc0` | 109 | **113** |
| differing · failing | 0 · 0 | **0** · **0** |

### One tree, one name table, renaming during parsing

`firnc0` merges the files after parsing and rewrites the names with
a `Renamer` (`modules.rs`). `firnc1` does the same **during**
parsing, and that fits its construction better:

* **One** name table for all files. For that the lexer gets a
  pointer to a shared `Interner` — without that, the same number in
  two files meant two different words.
* **One** tree. The parser writes into a foreign `ast.Baum` instead of
  creating one of its own; that removes any merging of indices.
* **Renaming with an alias.** A pre-scan collects the names that a file
  declares at the top level; exactly those get `alias__` in front. A
  qualified access `modul.name` becomes the same `modul__name`.

The root file is parsed **last** and is **not** renamed. The modules
before it: a constant may use one from another module, so
the dependencies have to be in the tree beforehand.

### `tests/710_module_core.fi`

Three levels, and the name `wert` stands in **all three** — in the
root file, in `kern/mid.fi` and in `kern/deep.fi`. Without renaming, one
version would shadow the other. Plus a constant and a `struct` from
a foreign module (passed by value) and a chain over two levels
(`mittel` includes `tief`).

### Honest limits

* **No cycle protection and no reuse.** If the same file is included
  twice, it is parsed twice; a cycle runs forever.
  `firnc0` has a `seen` set for that — here it is still missing.
* **`export` is read but not enforced.** A module can see everything
  another one declares.
* The search goes only one level: **relative to the root file**. `firnc0`
  looks next to the importing file first (blocker B3 from round 17) — for
  the test programs both are the same, for `lib/rt/vec.fi` they are not.

### What still lies between here and the fixpoint

`firnc1` can read several files — but **not its own**: `lexer.fi`,
`parser.fi` and `sema.fi` use generic collections (`Vec[T]`, `Map[K,V]`)
and `defer`. The core parser reads neither. The way there is thereby
clearly named and no longer vague:

1. **Generics** in the parser and in the monomorphization of `firnc1`
2. **`defer`** in the lowering
3. Floating point in the code generator

Only after that can `firnc1` compile itself.


---

## 20. Round 30: `defer`, floating point and stack arguments

Three gaps closed, and they are the last ones that do not hang on generics.

| | round 29 | round 30 |
|---|---:|---:|
| behaves like `firnc0` | 113 | **121** |
| excluded because of `defer` | 1 | **0** |
| excluded because of the code generator | 5 | **0** |
| differing · failing | 0 · 0 | **0** · **0** |

### `defer` in the lowering

One stack per **block level**. When a block is left, its own run
backwards; on `return`, **all** levels of the function run; on `break` and
`continue` only those that were declared **inside** the loop — for that,
every loop remembers the stack depth on entry.

The stack stays **unchanged** on an early run: the block clears
its own level itself. What is generated afterwards ends up in the
unreachable block behind the jump — nothing is executed twice.

`tests/720_defer_core.fi` writes the order into a buffer instead of only
counting calls. In the process a property came to light that I had expected
wrongly: **the argument of a deferred statement is only evaluated when it
runs**, not at the declaration. So `defer merke(s, 49 + i)` writes
the *later* value of `i`. Both compilers agree — the test
now stands there with this insight instead of with my assumption.

### Floating point in the code generator

`xmm0`/`xmm1`, `addsd`/`subsd`/`mulsd`/`divsd`, `cvtsi2sd` and `cvttsd2si`.
There is **no** classification of its own at the function boundary: stage 0
passes `f64` in integer registers (SPEC §14.1, a deliberate deviation from
System V), and exactly that is what this code generator does too.

The comparison with `comisd` sets the flags like an *unsigned* comparison,
and **NaN additionally sets PF**. According to IEEE-754 every comparison
with NaN is false — except `!=`, which has to be true. So: with `!=` the PF
is **or-ed in**, with all the others it is **and-ed away** with `setnp`.
`tests/590_f64.fi` checks exactly these cases (NaN against itself, `<`, `>`,
`>=`, infinity, minus zero) and now runs completely through the Firn
compiler.

### Stack arguments

More than six arguments go over the stack according to System V, in
**reverse** order. `rsp` has to be 16-aligned at the `call` — with an odd
number of stack arguments there is therefore a padding word in front of
them. The called function finds them at `[rbp+16]`, `[rbp+24]`, …

### What is still missing now

**Only generics.** Of 121 comparable programs, none drops out any more
because of `defer`, floating point or the calling convention. The 60 files
under „not core language" hang on `enum`/`match`, error unions, `gc class`,
attributes, `comptime` — and on **generic templates**, without which
`firnc1` cannot read its own sources (`Vec[T]`, `Map[K,V]`).

With that, the way to the fixpoint has shrunk to **one** item:
generics in the parser and in the monomorphization of `firnc1`.


---

## 21. Round 31: generics — and the fixpoint

**`firnc1` compiles itself.** The result is a fixpoint in the strict
sense: stage 2 and stage 3 are the same file octet by octet.

```
stage 1   firnc0 (Rust)  compiles  bin/firnc1.fi  ->  .firnc1     888 ms
stage 2   .firnc1        compiles  bin/firnc1.fi  ->  .firnc2    2070 ms
stage 3   .firnc2        compiles  bin/firnc1.fi  ->  .firnc3

.firnc2.s == .firnc3.s     147 220 lines of assembly, character-identical
.firnc2   == .firnc3       792 240 octets, binary-identical
```

Stage 1 is **not** compared along, and it does not have to be: `firnc0` has
a register allocation, `lib/firnc1/codegen.fi` does not. What is compared is
what stays stable from stage 2 on — and exactly there the result no longer
hangs on the Rust compiler.

| | round 30 | round 31 |
|---|---:|---:|
| behaves like `firnc0` | 121 | **131** |
| differing · failing | 0 · 0 | **0** · **0** |
| excluded because of generics | 9 | **0** |
| `firnc1` compiles itself | no | **yes, as a fixpoint** |

`test.sh`: **628/628**, of which section 17 is the fixpoint. Runtime ~3 min.

### Monomorphization, not type erasure

`lib/firnc1/mono.fi` (853 lines) is the port of `sema_generic.rs`
(collection, naming scheme) and `mono.rs` (instantiation). For **every used
type combination** a fully concrete function resp. a struct of its own comes
into being; after that the type checker sees only ordinary code.
The naming scheme is as in stage 0: `vec_push__i32`, `Vec__ptrmut_u8`.

Four things one cannot guess about it:

* **The templates must not go into the declaration lists of the tree.**
  Otherwise the type checker stumbles over `T`, and `--emit=ast-kanon`
  printed them — which `firnc0` does not do. Their *nodes* are very much in
  the tree; `mono.fi` only remembers the entry points.
* **Inside the body of a template, instantiations are abstract.** `Vec[T]`
  within `vec_neu[T]` is not yet work but a prescription. Only when it is
  rebuilt does `Vec__T` become a `Vec__i32` — and *then* it goes onto the
  work stack.
* **When rebuilding, child, parameter, field and statement lists have to lie
  contiguously.** So build fully first, collect, then deposit in one piece.
  The same lesson as in the parser (there `f(a, g(b))` once became
  `f(a, b, g(b))`).
* **`size_of[T]()` is not a call.** The type text migrates into the call
  name (`size_of$i32`), and the type node additionally into `e_zahl`; both
  are resolved only in the type checker, which knows the struct table.
  Without substituting *both* places, it reports „unbekannter typ 'T'".

While self-compiling, this results in: **13 function templates, 1
struct template, 31 reported instantiations, 28 really created** (the
difference are the abstract ones that only stand in template bodies).

### Three bugs that only the self-compilation found

All three lay in the code **before** this round and were not touched by 624
tests. A compiler that compiles itself is the sharper test.

1. **`parser__lx` — a parameter that was renamed.** The renaming of a
   module (`alias__name`) hit every identifier that is declared at the top —
   even when a **parameter shadows it**. `parser.fi` has a
   function `lx(p)` *and* a parameter `lx` in `par_neu(lx: u64)`; the
   parameter was called `parser__lx` afterwards and was gone.
   `modules.rs::Renamer` keeps a list of local names for that — `parser.fi`
   now does so as well, with scopes for the function, the block and `for`.
   What is shadowed are **only values**: a call `lx(p)` still means the
   function (`is_value = false`), and a type name does anyway.
2. **Forward references between structs silently yielded size 0.**
   `struct A { b: B }` **before** `struct B` computed with
   `groesse_von(B) = 0`, without a message — the program ran and computed
   wrongly (the `tests/fwd` case: `firnc0` gave 0, `firnc1` gave 2). It came
   to light at the monomorphization: `Vec__u32` comes into being **after**
   all hand-written structs but is used by `lexer.Lexer` as a field.
   `types.fi` now computes the layout **dependency-driven**
   (`struct_layout` with a `zustand` marking); it does not descend through
   pointers, so that `struct Knoten { naechster: *mut Knoten }` remains
   possible.
3. **`import` applied only to the root file.** A module was not allowed to
   include a module. `tests/640_vec_module.fi` failed silently because
   `modules/vec.fi` did not get its own `rt`. `bin/firnc1.fi` now keeps
   a **queue** over the whole inclusion graph (a breadth-first search like
   `modules.rs::resolve`), with path dedup — with that, the limits named in
   round 29 are gone as well: no double parsing, no endless cycle, and the
   search goes **first next to the importing file**, then next to the
   root file.

### Why the parser needs a registry

`vec_push[i32](&v, 3)` and `feld[i]` are the same form apart from the name.
The parser decides **by the name** whether a `[` is a type argument list or
an indexing — so before the first file is parsed it has to be established
which names are generic. That is why a pre-scan for `fn IDENT [` and
`struct IDENT [` (`gen_vorab`) runs over **all** files first, and only
after that is anything parsed. `firnc0` has done it the same way since
blocker B1 (`build_program`: lex everything first, then parse everything).

The price is a second lexing of every module source. That is cheaper than
keeping all lexers open at the same time — and it makes the order of the
`import` lines irrelevant.

### `tests/730_generics_core.fi`

The test for the round, and it checks exactly what went wrong: a template
from a **module**, instantiated with `i32`/`u8`/`i64`; a template that calls
a template — once with the same type, once with a fixed different one
(`groesse[u8]()` within `laenge_von[T]`); `size_of[T]()` in the
template body; a parameter `lx` that shadows the function `lx` of the same
module; a struct with a **forward reference** (what is checked is the
*layout*, not only that it compiles); instantiations as a field of a struct
of one's own; and a
module that includes a module **next to itself** and uses its constant from
inside a template body.

### Honest limits

* **The single-file tools know no generics.** `.astdump`,
  `.semadump`, `.firdump`, `.layoutdump` still report „not core language"
  (return value 3) for a generic file. That is no loss to the
  comparison: `firnc0 --emit=ast-kanon` fails on the same files itself,
  because its parser too only gets the registry over the module route —
  such files count there as *skipped*.
* **`export` is still not enforced.**
* **Path dedup compares strings, not canonicalized paths.** Whoever
  includes the same file over two different paths gets it twice.
  `firnc0` canonicalizes for that.
* **51 files in the corpus stay outside the core language** — and none of
  them because of generics: `enum`/`match` (10), error unions (20),
  `gc`/`rc` (14), intrinsics for constant runtime (4), `errdefer` (1),
  `comptime` (2). That is stage 1, not stage 0.

### What that means now

`SPEC.md` §11 demands the fixpoint for stage 3. It holds — for the
subset in which `firnc1` is written. The next honest step is
not „more bootstrap" but the **language surface**: as long as `firnc1` reads
no `enum`/`match` and no error unions, it cannot replace `firnc0`,
it can only carry itself.


---

## 22. Round 32: `enum` and `match` — the first language surface after the fixpoint

`firnc1` now reads enums and pattern matching — with the same
architecture as stage 0 (`sema_match.rs`): the cases of a `match` lie
**not** in the syntax tree but in a registry; in the tree there is only
a call `__match#<number>` without arguments. The layout of an enum
is entered into the type context as a struct with the fields `__tag` and
`__v<tag>_<i>`, and the offsets are computed in `pattern.fi`, not by
`types.fi`.

| | round 31 | round 32 |
|---|---:|---:|
| behaves like `firnc0` (self) | 131 | **140** |
| differing · failing | 0 · 0 | **0** · **0** |
| not core language (self) | 51 | **42** |
| parser octet-identical (`--emit=ast-kanon`) | 169 | **183** |
| types identical (`--emit=typen`) | 115 | **123** |
| FIR octet-identical (`--emit=fir-raw`) | 115 | **123** (37 845 instructions) |
| fixpoint (stage 2 == stage 3) | 147 220 lines | **173 103 lines, character-identical** |

`test.sh`: **628/628**. The bar of the round: `tests/200`–`206`, `230`,
`231` run through `firnc1` with identical behavior — reached, exactly
those nine more. The four negative tests (`match_int_ohne_auffang`,
`match_missing_variant`, `match_unbekannte_variante`, `match_unerreichbar`)
abort with `rc=1` — measured individually; the suite checks negative tests
only against `firnc0`.

### Three places where one cannot guess this

1. **The `__match#` numbers of the root file have to stay the small ones.**
   `firnc0` parses the root first, `firnc1` the modules first (constants
   may be cross-module). `tests/231_module_match.fi` has two
   `match` in the module and one in the root — `--emit=ast-kanon` shows
   `__match#0` in the root. The comparison runs only over the single-file
   tools, and those parse exactly one file — there the number is right.
   In the full run the numbers are internal and only have to be unique.
   Both is so, and both is measured.
2. **The binding in a pattern is an ADDRESS, not a value.** `Wert::Zahl(x)`
   binds `x` to the address of the payload field in the memory of the enum.
   The lowering produces the `ptradd` chain for that BEFORE the body of the
   case — with two bindings two `ptradd`, and only then the body. Whoever
   swaps the order gets different value numbers and the FIR comparison
   breaks. (A first draft was wrong right next to it: in the
   non-enum case the key value was never set — the switch branched on `%0`.)
3. **The layout demands two separate interventions in `types.fi`.** The
   enum names have to be registered BEFORE the struct layout (otherwise
   `fn dauer(a: Ampel)` does not know the type), and the layouts only
   entered AFTERWARDS (an enum may contain a struct by value, not the other
   way round). In between stands `typen_aufloesen` — so `Typen` got a
   pre-registration (`typen_struct_vormerken`, counts into `eoff`) and a
   subsequent fixing (`typen_struct_festlegen`), which appends the finished
   fields contiguously. Since then the index in the type context and the
   index in the tree are NOT the same number any more; `struct_layout` gets
   both.

### Ported, deliberately simpler

* **No jump table.** `codegen_switch.rs` builds a table in `.rodata` from
  eight dense tags on; `lib/firnc1/codegen.fi` always produces the
  comparison chain. Behaviorally identical — the yardstick of this stage is
  the behavior, not the assembly text.
* **`u64` tags beyond `i64::MAX` cannot be written down.**
  Stage 0 computes pattern values in `i128`; this stage in `i64`. No
  program in the corpus has such a tag; it is stated in `pattern.fi` and
  here.
* **Errors are counted, not described** — as everywhere in stage 1.
  The exhaustiveness check (`check_exhaustive`) IS ported: a
  missing case is an error, not a warning.

### Honest limits

* **`--emit=layout` still does not compare the enum/match files.** The
  yardstick (`layout_canon.rs`) does not know enums and prints their names
  as `?`; `bin/layoutdump.fi` therefore deliberately does NOT register the
  pattern registry, and the files count there as „not core" (counted, not
  handed over: it is 9 program files plus `tests/modules/state.fi`).
* **`match` in generic templates** is an error as in stage 0; here the
  file additionally counts as „not core" (pre-scan).
* **An enum by value as a struct field** remains an error (a pointer
  works); enum-in-enum by value works, with a topological sort and a cycle
  as an error.
* **The fixpoint remains „trivial" for now:** the sources of `firnc1`
  do not use `enum`/`match` themselves yet — the new paths are compiled
  along during self-compilation (stage 2 == stage 3 proves their
  compilability), but they are not exercised. Honestly so, and now
  possible: after this round the sources CAN do it.

### What that means

Of the 51 files that round 31 counted outside the core language, 9 have
moved over — `enum`/`match` is the first language surface that came in
AFTER the fixpoint, and the fixpoint still holds afterwards. The rest is
named: error unions (20), `gc`/`rc` (14), constant runtime (4),
`errdefer` (1), `comptime` (2).

## 23. Round 33: error unions — `error`, `try`, `catch`, `E!T`

`firnc1` now reads error unions — with the same architecture as stage 0
(`errors.rs`/`lower_errors.rs`): a registry outside the tree
(`lib/firnc1/err.fi`), in the tree there are calls `__try#` and `__catch#`,
and the type annotation `E!T` travels through the tree as a placeholder
`__eu#<number>` until the type resolution resolves it against the registry.
The union itself is an ordinary struct `{ __err: u32, __val: T }` in the
type context — aggregate return, ABI and codegen carry it unchanged;
the only new thing was the lowering of `try`/`catch`/conversion.

| | round 32 | round 33 |
|---|---:|---:|
| behaves like `firnc0` (self) | 140 | **166** |
| differing · failing | 0 · 0 | **0** · **0** |
| not core language (self) | 42 | **17** |
| parser octet-identical (`--emit=ast-kanon`) | 183 | **217** |
| types identical (`--emit=typen`) | 123 | **142** (26 025 expressions) |
| FIR octet-identical (`--emit=fir-raw`) | 123 | **142** (40 591 instructions) |
| fixpoint (stage 2 == stage 3) | 173 103 lines | **188 839 lines, character-identical** |

`test.sh`: **631/631**. The bar of the round was the twenty files
`tests/400`–`419` — reached, and five more: `tests/550`–`554` (the
`rc` files) were „not core" only because they use error unions
(`AllocError!…`); they moved along. The eleven negative tests
`tests/neg/err_*` all abort with `rc=1` under `firnc1` when measured
individually — the suite checks negative tests only against `firnc0`.

### Three places where one cannot guess this

1. **The type annotation `E!T` goes through an import cycle.** `types.fi`
   resolves type expressions, but the meaning of the placeholder
   `__eu#<n>` is known only to `err.fi` — and `err.fi` needs `types.fi` in
   order to create the union as a struct. Firn knew no function pointers at
   the time (they arrived in round 58, `docs/ROUND58.md`),
   so the types carry a pointer to the registry (`typen_fehler_setzen`) and
   `aufloesen` calls `fehler.fehler_typ` directly — `types.fi` and
   `err.fi` importing each other. That `modules.rs` resolves cycles
   is written down nowhere; it is measured on a counterpart in `/tmp`, not
   guessed.
2. **The point in time of the union decides the struct index.** In stage
   0 a union only comes into being at the resolution
   (`get_or_create_union`), AFTER all the structs of the tree. This stage
   keeps the order with two separate ways: error SETS are pre-registered
   (they count towards `eoff`, like the enums), unions are appended
   (`typen_struct_anhaengen` does NOT count towards `eoff`) and carry their
   layout with them immediately. Whoever confuses that gets different
   struct indices — and the type comparison breaks. The same spot explains
   the safety switch in `groesse_anfordern`: a union lies BEHIND the state
   vector of the struct phase, but its layout is already there — so
   „beyond the vector = finished" applies.
3. **The implicit conversion sits at five places, not at one.**
   `return`, `let`, assignment, argument and struct field each get the same
   hook (`coerce_pruefen`, modeled on `hook_coerce`): a success value
   becomes `__err = 0` plus the value, an error variant becomes
   `__err = code`. If one forgets the ARGUMENT, `nimm(7)` for
   `fn nimm(r: E!i32)` compiles silently wrong — the caller passes a
   scalar, and the callee side expects the aggregate. And because the
   converted form writes its own expression once more, the BUSY marking
   from stage 0 is needed, otherwise the lowering runs endlessly in a
   circle.

### Honest limits

* **`errdefer` stays outside — now explicitly.** `tests/581`
  combines `errdefer` with error unions; up to now the `error`
  keyword kept it away. `ret_term_fehler` in stage 0 additionally runs the
  `errdefer` statements on the error path — that is a
  round of its own. That is why the pre-scan now reports `errdefer` on its
  own as „not core"; without the marking, `581` would silently have been
  compiled with wrong behavior.
* **`--emit=layout` does not compare the error union files** — the same
  state as with the enums: `layout_canon.rs` does not resolve `__eu#<n>`
  and prints `?`; `bin/layoutdump.fi` therefore deliberately does NOT
  register the registry, and the files count there as „not core".
* **`tests/130_must_consume.fi` stays „not core"** — it needs the
  attribute syntax `#[must_consume]` (`attrs.rs`), not the error unions.
  What the round brings for that: an `E!T` value is implicitly
  must_consume, and discarding it is an error (`sema.fi`, modeled on
  `check_discard`) — measured individually on `tests/neg/err_discarded.fi`
  (`rc=1`).
* **`E!T` in generic templates** is not thought through, only named: the
  placeholder refers to the type node as it is WRITTEN — a
  substitution of `T` per instantiation does not see it. No program in the
  corpus does that.
* **The fixpoint remains „trivial" for now:** the sources of `firnc1`
  do not use error unions themselves yet — the new paths are compiled
  along during self-compilation (stage 2 == stage 3 proves that), but they
  are not exercised.

A new endurance test: `tests/740_error_union_core.fi` — an error set and a
union from a module (`tests/modules/kern/mid.fi`), `try` over two levels
across the module boundary, `catch |e|` with an error comparison,
conversion at `return`/`let`/assignment/argument, a union as a struct field.

### What that means

Of the 42 files outside the core language, 25 have moved over —
what remains are `gc` (9), constant runtime (4), `comptime` (2),
`errdefer` (1) and the attributes (1). The five `rc` files have moved over
completely: they were bound to the extensions only through their error
unions. The core language can do errors now — the next
honest step is `gc`, the largest remaining block.


## 24. Round 35: `comptime` — the compiler executes code at compile time

Built in parallel to round 34 in a git worktree of its own (branch
`r35-comptime`), because comptime was the most isolated remaining block;
the merge ran fast-forward without a single conflict.
`lib/firnc1/time.fi` (689 lines) is a real interpreter modeled on
`compiler/src/comptime.rs`: the comptime blocks of the
root file are executed at compile time, and the source text they generate
is hung into the same tree as a module without an alias over the same
lex/parse machinery — before the monomorphization, with
the same interner. The parser reads `comptime { }` and relaxes the
pre-scan for it; the driver `bin/firnc1.fi` wires the run
between the root parser and `mono.gen_lauf`.

Honest limits, named instead of concealed: comptime in imported
modules is still reported by `sema_braucht_comptime` (only the root file
runs), and constants that would have to be evaluated at compile time
but cannot remain a separate known case.

Measurements after the merge: `test.sh` 634/634, `selbst_vergleich` 169
behaviorally identical programs (previously 166) at 0 differing and 0
failing, the fixpoint holds — stage 2 == stage 3, character-identical,
210 324 lines of assembly. Both target files (601, 602 — among them the
UCD table generation, the hardest comptime case in the corpus) run
identically to `firnc0`. New: `tests/760_comptime_core.fi` and
`docs/ROUND35.md`.

What remains: `gc` (9), constant runtime (4), `errdefer` (1) and the
attributes (1).

## 25. Round 34: `gc class` — the largest block of the core language

`gc class`, `Gc[T]`, `GcWeak[T]`, `weak`/`stark`, `x.as?[C]` and the
transitive `#[no_gc]` checker are ported (modeled on `gc.rs`/`nogc.rs`).
The registry lies in `lib/firnc1/gc.fi`, the nogc checker in
`lib/firnc1/nogc.fi`; the runtime `lib/gc/gc.fi` — itself written in
Firn — is pulled in automatically as embedded source text (`gctext.fi`)
as soon as `gc class` appears anywhere in the import graph, and it lies
in the root namespace: `gc_init()` is called `gc_init()` in every module.

The find of the round (again one that 600+ tests did not find): the
GC scan in the driver looked up `gc`/`class`/`AllocError` via `intern_finde`
— numbers that only exist if the root file contains the words.
If `gc class` stood only in a module (560 -> modules/dom.fi), the
scan ran with -1 and found nothing: no runtime, no AllocError set,
a silent sema error. `main.rs` uses `intern_nummer` at the same spot
— now here as well.

Measurements: `selbst_vergleich` 169 -> 179 behaviorally identical programs
(all nine gc files, among them 510 cycle and 560 DOM cycles with real
resolution), 0 differing, 0 failing. The fixpoint holds: stage 2 ==
stage 3, character-identical, 279 201 lines of assembly. All six
gc/nogc negative tests abort like firnc0. New: `tests/770_gc_core.fi`
(a gc class only in the module, a cycle under the root) and
`docs/ROUND34.md`.

What remains: constant runtime (4), `errdefer` (1), `must_consume` (1).

## 26. Round 36: ct intrinsics, `errdefer`, `must_consume` — the core language is complete

The last three blocks are ported. `select(bedingung, a, b)`
(data-independent selection, cmov instead of branches, both branches
exactly the same scalar type), `secure_zero(zeiger, anzahl)` (zeroing with
volatile store semantics, must never be optimized away, SPEC §9.3) and
the ct barrier — each with its own detection in the sema BEFORE the
function lookup, so that a user function of the same name wins.
`errdefer` runs only on the error path (rejecting a finished union as in
stage 0), `#[must_consume]` on functions and structs reports discarded
results. Details in `docs/ROUND36.md`, core test
`tests/780_ct_core.fi`.

Measurements: `test.sh` 640/640, `selbst_vergleich` 186 behaviorally
identical programs at 0 differing and 0 failing, the fixpoint
character-identical (284 207 lines of assembly). All eight negative tests of
the round abort like firnc0. With that, the WHOLE core language stands in
firnc1: the compiler in Firn compiles every core language program the way
the Rust compiler does — and itself.

## 27. Round 37: the optimizer attack — html5lib below the 2x mark

Three optimizations in the Rust compiler (firnc1 deliberately contains no
optimizer; the fir comparison runs on `--emit=fir-raw` BEFORE any
optimization, so there was nothing to mirror): branch fallthrough and
cmp directly into the target register (977a2ad), register pools rsi/rdi/rdx
in the linear scan (ef4e530), inliner correctness with a recursion ban and
the shift immediate form (bf13ed4). Tokenizer measurement series: html5lib
1,94x -> 1,69x (goal <=2x reached), realweb 4,82x -> 4,34x (intermediate
goal <=3x missed). The next lever: interval splitting + coalescing in the
register allocator (7 391 static reg->reg movs, 445 store/reload pairs).

## 28. Round 38: the collector learns two things — return and slices

Stage 2: completely empty chunks of EVERY size class now go back to the OS
(with a two-sweep hysteresis against munmap/mmap oscillation), and a
threshold cap (4 MiB) ensures that after a large-object phase no more
"silent" heaps without a collection come into being. Phase test: RSS at the
end 24 124 KiB (never falling) -> 2 112 KiB, the behavior of the collection
unchanged.
Stage 3: hybrid incremental collection from 8 MiB of heap on — marking in
slices of 512 objects, sweeping in slices of 2 chunks, a Dijkstra
insertion barrier, white parity instead of a mark reset. Pauses are thereby
around 0,5 ms independently of the heap size; below 8 MiB the atomic
path remains (the phase check costs 8,7 % of throughput, the requirement
was ±10 %).
An important side finding (ROUND38.md): the optimizer can remove a final
nulling as dead — unreachability belongs in a
helper function, and the scrubber zeroes returned frames.
Finalizers and Arc[T] are named remaining work (a semantic resp.
thread decision is needed); the 30-minute endurance run is being caught up.

## 29. Round 39: `import std.*` and `f"..."` — convenience comes into the language

The module search path is identical in BOTH compilers: next to the
importing file, next to the root file, `$FIRNLIB`, `<exe>/../lib`
(the installation layout). On top of that stands `lib/std/` — the facade in
C# style over the proven building blocks (io, math, str, vec, map, num,
mem), core test tests/790_std_core.fi. The string interpolation
`f"x = {x}"` is decomposed by the parser AT COMPILE TIME into a chain on
the Fmt builder (no varargs, no runtime parsing, no
slowdown), built in firnc0 AND firnc1; the display is that of i64 —
the honestly named limit of the core version. Core test
tests/791_interpolation_core.fi, three negative tests abort on both
sides. Verified from a /tmp project via FIRNLIB.

## 30. Round 40: regalloc against realweb — and an alias that went too far

Four levers, all backed by callgrind (the wall clock fluctuates by
±30 % here and did NOT show the first gain): a register descriptor
against store->reload, a cell alias for loads, two fast paths in the
tokenizer (`eingabe_pruefen` as ONE range test, `dekodiere` with
pre-reservation), immediates up to 32 bits in the full unsigned
range. realweb 4,34x -> 2,68x, html5lib 1,69x -> 1,33x,
instructions realweb -44,4 %. Refuted and rejected: a "text run in one
piece" in the data state (0,008 %) — the state `match` has long been
a jump table.

## 31. Round 41: the price of an optimization the allocator did not know about

The cell alias from round 40 let a load read the cell register
directly. But the register allocation had already run by then — it
did not know about the lifetime EXTENDED by the alias and was allowed to
hand the same register to another value. In
bin/print.fi/drucke_binop that became `43 - &tab[start]` instead of
`43 - start`: the length underflowed, `rt.buf_wachse` doubled until
overflow and spun forever. Effect: `.astdump` hung on EVERY
file with `||` — that is, on almost every one — and test.sh stopped in
section 12. A second hole of the same optimization: a `call` between
the load and the use destroys caller-saved registers (layoutdump
crashed in `intern_finde` with t=0). Both cases now end the alias;
the correction costs +4 instructions out of 1,297 billion.

TWO LESSONS, dearly paid for:
  * The bug had been in the tree since round 40 and 649/649 stayed green,
    because the dump binaries were reused in a STALE state. A
    comparison tool that does not rebuild its yardstick checks
    yesterday's state.
  * Two simultaneous runs (main repo + worktree) used
    THE SAME /tmp files and overwrote each other's
    comparison outputs — that looked like 148 real deviations. All
    six comparison scripts now create their own mktemp -d.

Plus the planned part: a histogram of the INDIVIDUAL slices (7 types x
16 buckets) instead of only maxima — the mark stack never overflows, the
expensive re-marking does not occur in continuous operation at all. And the
marking slice ends after a TIME BUDGET of 100 us instead of after 512
objects (a node with many pointer fields costs a multiple of
a text node): slices above 128 us from 60 865 to 137 per 20 s,
in the 60-second run 99,88 % between 64 and 128 us, throughput
unchanged.

## 32. Round 42: the std gains depth

The facade from round 39 was broad and thin — every topic had a
module, every module the bare necessities. Round 42 fills it: str (search,
split, join, trim, replace, upper/lower, character iteration),
num (integer <-> text in both directions, bases, overflow detection,
an f64 wrapper over dtoa/strtod), vec (search, insert, remove,
reverse, sort, binary search), map (iteration, keys/values,
take out), math (floor/ceil/round, exp/ln/log, trigonometry,
gcd/lcm, INF/NAN/EPSILON), io (read lines, stdin, append,
characters/hex/bool in the Fmt builder). New core tests 800-806.
Acceptance re-measured in the main repo: test.sh 673/673, self 196/0/0,
fixpoint stage 2 == stage 3 character-identical (309 468 lines).

## 33. Round 43: the speed goal falls — and not in the tokenizer

The assignment was to get `realweb` from 2,68x to at most 2x against
`html5ever`. The documented lever (interval splitting in the regalloc) was
**not** needed; the profile showed the cost elsewhere.

* **`mem_copy` copies word-wise** instead of byte-wise — realweb -22,2 %,
  html5lib -11,9 %. The self costs of `main` fell from 332,9 to 45,8
  million instructions; a single block carried 109 of them.
* **The register path can do stack arguments** (more than six arguments
  always ended up in memory before) — realweb -4,3 %.
* **The address offset migrates into the memory access**, `mov r8, [r8+160]`
  instead of an addition beforehand — realweb -0,8 %.

Instructions realweb **1.297.226.150 -> 957.989.680 (-26,15 %)**, html5lib
-13,39 %. Re-measured by ourselves on the merge state with
`tools/tokenizer/throughput.sh`: **realweb 1,54x** (goal <= 2,00x reached),
**html5lib 0,95x** — on the pathological cases the Firn tokenizer is thereby
faster than html5ever. Throughput realweb 29,25 MB/s.

**The methodological lesson of the round:** the wall clock scatters over
three measurement pairs between 2,58x and 2,85x for the *same* binary — the
value 2,68x from round 41 lay in the middle of that band. What is dependable
are only the callgrind numbers, reproducible to the instruction; the clock
is good for a check, not for a proof. Two further hypotheses (labels without
a jump target, a jump table) were deferred with reasons after the goal had
been reached.

## 34. Round 44: the build-up phase without stop-the-world

Up to round 43 the collector ran atomically below `INKR_AB = 8 MiB` of heap
— while building up a large live set those were three full runs of up
to **11,82 ms**. The justification for this threshold was measured
correctly but **attributed wrongly**: the cause did not sit in the heap
size but in the sliced sweep.

* **Sweeping with a time budget** instead of in one go, **free lists per
  class** — without that, the sliced sweep used fresh chunks instead of the
  free lists again and doubled the heap.
* **A second clock (compute time of the thread)** next to the wall clock in
  the pause measurement; only that way can foreign load be separated from a
  real pause — exactly the mistake that had produced the 19 ms outliers in
  round 40.
* New measuring tools: `build.fi` (does NOT zero the histogram, so it
  measures the build-up phase too), `throughput.fi` (fixed work, measured
  time), `ab.fi` (A/B in the same process).

Result: longest interruption **11,82 ms -> 0,45 ms** (5 s run), 0,62 ms
of pure compute time in the 10-minute endurance run. Throughput loss 2 % with
a small live set, 0 % with a large one. `tests/771_gc_build_without_stw.fi`
checks that deterministically via `gc_volle_laeufe() == 0` instead of via a
time comparison — that would be worthless on a loaded machine.

## 35. Round 45: methods — `impl`

Up to here Firn had free functions only; the prefix was the
type, written down by hand and unchecked (`bytes_push(&b, x)`). Now:
`b.push(x)`, `source.trim().length()`.

`impl` is deliberately a **writing aid**: `a.f(b)` is resolved to
`Typ_f(&a, b)`, there is no dynamic dispatch and no vtable. Implemented in
both compilers in lockstep — firnc0 with a new `compiler/src/impls.rs`
(490 lines) plus hooks in the parser, the type checker, the lowering and the
nogc check; firnc1 in `parser.fi`, `sema.fi`, `lower`, `nogc.fi`. Plus an
`impl` wrapper for `std.str`, tests 810-812 and **eight negative tests** (a
receiver without an address, a receiver that is a pointer, not a struct, a
free function is not a method, wrong argument count/types).

## 36. The merge of rounds 43-45 — and the same trap for the fourth time

The three rounds ran in parallel in separate worktrees with cleanly
separated territories (regalloc/codegen · GC · parser/sema) and could be
merged without a conflict apart from one `.gitignore` conflict.

The acceptance afterwards reported **one** deviation:
`771_gc_build_without_stw.fi`,
firnc0 gave 0, firnc1 gave 3 (`gc_volle_laeufe() != 0`). With a **freshly
built** `.firnc1` the test was green three times in a row. The cause was
again a reused binary: `tools/self_compare.sh` built
`.firnc1` only **if it was missing** — so after the merge it compared a
compiler that no longer existed.

That is the same mistake as with the dump binaries (round 41, fixed there).
`self_compare.sh` now rebuilds `.firnc1` when firnc0 or
any source under `bin/` or `lib/` is younger, too.
**Rule: never reuse a binary just because it exists.**

**Acceptance of the merge state in the main repo, measured by ourselves:**
`test.sh` **696/696** · `self_compare.sh` **201 identical / 0 differing / 0
failing**, CODEGEN FEHLT 0 · `fixpunkt.sh` stage 2 == stage 3,
character-identical, **328.343 lines** · throughput realweb **1,54x**,
html5lib **0,95x**.

## 37. Round 46: interfaces — `interface` and dynamic dispatch

Round 45 had brought methods only as a writing aid: `x.m(a)` became
`Typ__m(&x, a)` by the **static** type. Round 46 adds the case that
`SPEC.md` §6.2 has demanded since v0.1 — **one call site, many types**:

```firn
interface Flaeche { fn flaeche(*self) -> i64 }
impl Area for Rectangle { ... }
let f: dyn Area = (&r) as dyn Area
f.area()         // which code runs is settled only at run time
```

`dyn I` is a double pointer (a data pointer + a method table). The tables
stand as `.L__iface.<I>.<T>` in `.rodata`. Two new FIR instructions carry
that: `O_CALLI` (a call over a pointer) and `O_VTAB` (the address of a
table) — identical in both compilers, and 14 negative tests cover missing
methods, wrong signatures, duplicate `impl` and unknown interfaces. The
collector still reaches the objects behind the double pointer; a GC test of
its own secures that.

## 38. Round 47: finalizers, `Arc[T]` and weak fields

The remaining work on memory management named since round 38. Finalizers
run on collection, resurrection is defined, and the collector stays
non-reentrant while doing it. Weak fields are **really zeroed** on
collection (a test of its own). `Arc[T]` got a new, indivisible primitive:
`O_ATOMADD` → `lock xadd qword ptr [rcx], rax`, one instruction; without it
`Arc` would only be `Rc` with a different name.

**Measurement:** in the 150 s run with finalizers **0 of 253 698**
interruptions above 1 ms (measured in compute time) — the pauses have not
become worse but are around 2 % better. Callgrind was unsuitable here: it
shifts the stack, which is why the round now reads the stack bottom from
`/proc/self/maps` instead of guessing it.

## 39. Round 48: packages — manifest, visibility, `--paket`

Up to here there was only `import a.b` and the environment variable
`FIRNLIB`. Round 48 brings a manifest `firn.paket` — **deliberately no
TOML**: the format has six keywords, is line-based and can be read without
a foreign parser in both compilers (`compiler/src/package_world.rs` and
`lib/firnc1/package.fi`). Plus a deterministic search order
(project sources → dependencies → `FIRNLIB` → compiler directory) with
clear errors for cycles, missing packages and name conflicts, as well as
public/private at module level. `--paket` compiles a project on the basis
of the manifest; together with a source file it is rejected identically in
**both** compilers. A new section 18 in `test.sh`:
`tools/packages/run.sh`, **21 cases through both compilers**.

## 40. The merge of rounds 46-48

A single real conflict, and an interesting one: R46 and R47 had
**both** handed out a new FIR instruction with the number 15 (`O_CALLI`
resp. `O_ATOMADD`). Since branches cannot see the number assignment, that
is not a fault of the rounds but the price of parallel work on the same
instruction set — renumbered while merging to `O_ATOMADD = 15`,
`O_CALLI = 16`, `O_VTAB = 17`. **Lesson:** new FIR opcodes belong in a
reserved range per round, otherwise every parallel round costs this
conflict.

**Acceptance of the merge state, measured in the main repo by ourselves:**
`test.sh` **751/751** · `selbst_vergleich` **213 identical / 0 differing / 0
failing** · fixpoint character-identical (**427 401 lines** of assembly,
2 459 904 octets) · packages 21/21.

## 41. Rounds 50, 51, 52, 53 and their merge

Four parallel rounds on the base `cc1710f`, each conflict-free
against `main` individually, merged together in this order: r51-tempo,
r53-gcvec, r50-generik, r52-freistehend.

**State after the merge (measured by ourselves, not taken over from worker
reports):**

| Check | before | after |
|---|---|---|
| `test.sh` | 751/751 | **819/819** |
| `self_compare.sh` | 213/0/0 | **225 identical / 0 differing / 0 failing** |
| fixpoint | character-identical | **character-identical, 495.250 lines** |
| tokenizer realweb (callgrind) | 957.989.680 | **699.459.494** |

`CODEGEN FEHLT` stands at **0** for the first time — the old gap „floating
point, more than six arguments" in the compiler written in Firn is closed.

**The only real merge conflict** was in `codegen_x86.rs`, `Term::Ret`:
round 51 deleted the `xor eax, eax` before the return of a void function
without replacement (4.229.623 useless instructions in the measuring run),
and round 52 had introduced the condition `!f.interrupt` at the same spot
so that interrupt handlers do not touch `rax`. Resolved in favor of
round 51: where nothing is written any more, the exception for
interrupts is moot — that is strictly stronger, not weaker.

**The find while re-checking: round 52 was incomplete.**
`tools/freestanding/run.sh` links the kernel against `demos/kernel/start.s`
-- that file never existed. The cause is line 2 of the `.gitignore`: the
pattern `*.s`, meant for generated assembly, swallowed the hand-written
boot preamble as well. The worker saw
worktree and reported green; in the main repo it was missing, and
sections 3 and 3b (linking, the QEMU boot) failed — that is, exactly the
proof that matters in this round.

What was added afterwards was a complete preamble (multiboot header,
page tables built and zeroed at runtime, 1 GiB mapped identically with
2 MiB pages, PAE, EFER.LME, CR0.PG, a 64-bit GDT, a far jump into
long mode, then `KERN_START`) plus the exception
`!demos/kernel/start.s` in the `.gitignore`. Result:
**FREISTEHEND 41/41**, the kernel **boots in QEMU from both compilers**
and prints on the serial port.

**Lesson (the third instance of the same rule):** a green worker report
proves only that it ran in the worker's worktree. Only section 19 in the
main repo proves that it lies in the repo. From now on, after a
`.gitignore` change one has to look at merge time whether a branch creates
files that are not artifacts.

## 42. Round 54 (DOM), round 49 (threads) and the hardest merge so far

`r54-dom` went in conflict-free apart from the `.gitignore`. `r49-threads`
did not: **13 conflicts**, of which three were real collisions — twice both
branches had handed out THE SAME number without knowing of each other.

**State after the merge (measured in the main repo):** `test.sh` **846/846** ·
selbst_vergleich **232 identical / 0 differing / 0 failing** · fixpoint
character-identical, **561.666 lines** · thread endurance run 60 s:
1.171.122 rounds, 11.903 collections, 75.844 stops, **RSS drift −128 KiB**.

### The three real collisions

1. **Slots in the state block.** Round 53 put `S_SLOTS_TID`, `S_SCHEIBE`,
   `S_ZBUDGET` at 1960/1968/1976 — exactly where round 49 put its
   thread table (`S_FADEN_TAB` …). Round 53 moves to
   2120/2128/2136 (free, below `REG_SAVE_OFF` = 3968).
2. **Bit 8 in the source scan.** `gc_quelle_scan` reported with bit 8 in
   round 53 "the program needs GcVec/GcMap", in round 49 "the program
   brings its own `__thread_work`". Bit 8 stays with the collections (it
   counts from MODULES as well), and the thread dispatcher moves to **bit 16**
   (which, like bit 4, counts only from the root file).
3. **`laufzeit_quelle`** now has **four** parameters instead of three; both
   rounds had claimed the third for themselves.

So the lesson from §40 (reserved opcode ranges) falls short:
**every consecutively handed out number belongs in a reservation** —
opcodes, slot offsets, bit masks, test numbers. The thread tests were
called 840–842 and thereby collided with 840–842 of the collections; they
are now called 860–862.

### The most interesting find: a test that lived off the broken register scan

`842_gcmap_grund` failed after the merge: after deleting 1000
entries, **768** objects stayed alive. Not a leak — the cause is
round 49, which **repaired the conservative register scan**: until then
`__gc_collect_now` scanned the first 48 octets of the state block instead
of the save area, so the scan ran into the void. Since then, old
bit patterns hold on to two earlier slot buffers of the map, and those hold
their values.

The counter-check carried out, in this order:

* the register scan switched off experimentally → **unchanged 768** (so not
  the scan itself),
* six instead of two `gc_collect()` → **unchanged** (so no
  unfinished sweep),
* the section moved into a function of its own, then a register wash
  through deep recursion → **unchanged**,
* `gc_stapel_saeubern()` before the measurement → **0**.

So it is not a bug but the known price of a conservative
collector: it MAY keep dead objects. Whoever claims the opposite has to
clean the dead stack first — `tests/833` and `lib/dom` have done so since
round 49, and `842_gcmap_grund` does so now as well.

### Two side findings

* A Rust module test (`schmales_add_wird_nicht_zur_adresse`) looked for
  `add e…` and overlooked `add r10d` — the merge only shifted the
  register choice. The test was too narrow, the code was not wrong.
* `tests/neg/arc_discarded.fi` expected `416:5`; the thread extension in
  `tests/modules/rc.fi` has lengthened the generated file (now `441:5`).
  Such positions belong in the body under `lib/rc/parts/`, not in
  the generated file.

## 43. Round 55: the source text becomes English (stage A)

Justin wants the language to be generally accessible, so the German in the
source text falls. Stage A is **identifiers, messages and file names**; the
comments and the documentation follow in stage B.

The extent, measured beforehand instead of estimated: **1 983 German
identifiers**, **1 036 diagnostic and output texts**, **237 renamed files**,
**718 touched files**, 42 850 new against 38 517 old lines.

### Why not by hand and not purely mechanically

Both would have been wrong. By hand, 1 983 names are a field of typos;
purely mechanically it is an error generator, because desired names collide
(`Art` -> `Kind`, but `kind` already exists) and because several German
names point at the same English one (`Entry`, `Store`, `First` — 34 cases).
The way was therefore: a **morpheme table** with 312 word parts
(`baum`->`tree`, `zeiger`->`ptr`, `laenge`->`length`), a generator that
decomposes every name and makes a suggestion, and a **conflict list** with
278 names that were decided by hand. All of it lies in `tools/english/`.

### The traps that snap shut in the process

* **Byte arrays with a fixed length.** Runtime names stand in the compiler
  as `[u8; N]`. Whoever renames `"__faden_starten"` and leaves the `15`
  standing builds a bug that only comes to light weeks later.
  Counter-check: `check_lengths.py` holds every length entry against the
  text next to it.
* **Generated files.** `gctext.fi`, `lib/std/str|num.fi` and the rc/arc
  tests are generated — there the generator is converted and things are
  regenerated, and the product is not edited.
* **UPPERCASE names.** The morpheme decomposition split `ALL_CAPS` into
  single letters; only with a directed regex did the remaining 264
  names fall (`CHUNK_KOPF` -> `CHUNK_HEADER`, and so on).
* **Firn programs in tool scripts.** In `tools/thread/run.sh` and
  its neighbors there are small Firn sources as text; otherwise the `sed`
  patterns hit nothing.
* **Word order.** firnc1 translated its manifest hints by sense
  instead of word for word like firnc0 — the self-comparison would have
  reported it, and the worker found it beforehand.

### The proof that ONLY names have changed

Test numbers alone would not show that. Hence the counter-check on a
large program: `lib/html/tokenize_main.fi`, compiled to assembly once with
the old and once with the new compiler, yields **47 509
lines here as there** and an **identical mnemonic stream of 42 820
instructions**. What differs are exclusively symbol, file and text names.

### What the counter-check overlooked — and what follows from it

`check.py` looks only into the identifiers INSIDE the sources. **Nobody
checks path names.** What had been overlooked were three test files
(`841_gcvec_inkrementell.fi`, `iface_parameterzahl.fi`,
`impl_argumentzahl.fi`) and four working directories (`.gc-mess-work`,
`.baum-work`, `.selbst-work`, `messung-*.tsv`). A new counter-check:
`check_names.py` holds **every path managed by git** against the
morpheme table.

While catching up, the next lesson came right after it: whoever renames an
**ignore pattern** thereby makes the old artifacts visible —
`git add -A` promptly took `.baum-work/` and `.selbst-work/` into the repo,
and the freshly built counter-check reported my own mistake in section 21.
On top of that, after the move the exception for the boot preamble still
pointed at `beispiele/kernel/start.s` instead of `demos/kernel/start.s` —
exactly the trap that cost the kernel in round 52, this time found before
the damage. Incidentally it came to light that `.gc-meas-work/`,
`.bench-ab/` and `.bench-instr/` were managing their measurement artifacts
in the repo, although the tools create them anew on every
run (`gc_meas/run.sh` even deletes its working directory as its
first action).

### Acceptance in the main repo, measured by ourselves

`test.sh` **847/847** (846 plus the counter-check as section 21),
self-comparison **232 identical / 0 differing / 0 failing**, fixpoint
character-identical (**557 673 lines**, 3 286 024 octets in stage 2 as in
stage 3), `CODEGEN FEHLT: 0`, tokenizer **6810/6810**, tree construction
**150/150**, FREISTEHEND **41/41**, PAKETE **21/21**, FAEDEN passed.

### Stage B

What remains are **26 120 German comment and documentation lines** (docs
5 654, lib 5 627, Markdown in the root 4 018, tests 3 977, compiler/src
3 554, bin 2 312, tools 932). The yardstick for that is
`check_comments.py`; prose is recognized over German function words, not
over the morpheme table — technical terms are called the same in both
languages. The requirement for stage B: **the line count of every file
stays the same**, otherwise the position entries in the 134 negative tests
shift.

## 44. Rounds 56 and 57: comments, documentation — and a yardstick that lied

Stage B was the prose: comments and documentation. Six rounds worked in
parallel on separated territories (compiler/src, lib/firnc1, the rest of
lib, tests plus tools, docs, the Markdown in the root), each with the
requirement that **the line count of every non-Markdown file stays
exactly the same** — 134 negative tests name positions as line:column,
and the fixpoint compares assembly including `.file`/`.loc` entries. All
six kept it; the merges were free of conflicts. Only the round for the
root Markdown reached into foreign files, because it renamed documents
(`ABNAHME.md` -> `ACCEPTANCE.md`, `DESIGNZIELE.md` -> `DESIGN_GOALS.md`,
`LOGBUCH.md` -> `LOGBOOK.md`); it was merged last with `-X ours`, and the
113 path references were pulled afterwards by hand.

### The yardstick produced exactly what it measured

`check_comments.py` counted a line as German as soon as it contained one
of `in`, `an`, `am`, `es`, `man`, `war`, `hat`, `die`, `name`, `wert` —
every one of those is an ordinary **English** word as well. Whoever wants
to satisfy such a yardstick writes English that avoids those words. That
is what happened: of **4 547 comment lines under compiler/src not a
single one** contained the word `in`, and `lib/firnc1` had five in 2 387.
The result reads like "foundation point out of §10.4" instead of "from
§10.4".

The mistake was mine — the yardstick went out untested. Round 57 fixed
both: the word list no longer holds anything that exists in English, and
three rounds rewrote the affected comments into natural English. The
counter-check is now the other way round: the word `in` has to appear
**often**. It does, 485 times under compiler/src and 373 under
lib/firnc1.

### Two gaps the checks had left open

* **A trailing counter hides the German word.** `pfad2`, `teil1`,
  `soll4`, `stufe0`, `lebende100` — the splitter found `pfad2` as ONE
  word, and the morpheme table only knows `pfad`. With the digits cut off
  first, **56 German identifiers** surfaced that round 55 had missed.
* **Nobody checked the output of the TEST PROGRAMS.** `check_texts.py`
  only looks at the two compilers. `tests/802` and `tests/806` were still
  printing `erst`, `letzt`, `kap`, `zeilen`, `summe`, `gefangen`.

Both gaps are closed, and `check_comments.py` now hangs in `check.sh` and
therefore in section 21 of `test.sh`: a German comment line is a test
failure from here on.

### Finally the tool itself

`tools/englisch/` had been left out deliberately in round 55 — it would
have renamed itself. Now that it is finished, the exception falls:
`tools/english/` with `check.sh`, `check.py`, `morphemes.tsv`,
`suggest.py`, `rename.py`. The documents followed: `docs/RUNDEnn.md` ->
`docs/ROUNDnn.md`, `docs/SELBSTHOSTING.md` -> `docs/SELF_HOSTING.md`.

And the rename walked straight into the trap the tools warn about:
`check_texts.py` kept looking for its exception list under the old name
and promptly reported 44 x86 mnemonics as German text. The counter-check
showed it within seconds.

### Acceptance in the main repository, measured by hand

`test.sh` **847/847**, self-comparison **232 same / 0 differing / 0
faulty**, fixpoint character-identical (**554 923 lines**), `CODEGEN
MISSING: 0`, tokenizer **6810/6810**, tree construction **150/150**,
FREESTANDING **41/41**, PACKAGES **21/21**, THREADS passed. All five
counter-checks of the English migration report **zero**: identifiers,
text sites, length entries, path names, comment and documentation lines.

The source text of Firn — compiler, runtime, library, tests, tools and
documentation — is English.

## 45. Round 59 seen from the language: what a kernel demands

Round 59 built a small operating system core in Firn out of nine modules
(`demos/kernel/kmain.fi` and the files beside it): its own IDT with
exception reports, PIC and PIT with a tick counter, the memory map of the
boot loader with a frame allocator and a heap, the keyboard over IRQ1,
and the switch into ring 3 with `syscall`/`sysret`. It boots in QEMU, and
`tools/kernel/run.sh` holds its serial output against expectations in 46
cases (section 22 of `test.sh`).

**For self-hosting the interesting number is the one that did not move.**
Not one line of the compiler was changed in that round — and the fixpoint
has the same size as before it, to the line:

At the state of the branch `r59-kernel` — before the merge with rounds
58 and 60, whose numbers stand in section 46:

`test.sh` **854/854** (847 plus 2 x 3 build stages of the new tests
890/891 plus the new section), self-comparison **234 same / 0 differing /
0 faulty**, fixpoint **character-identical, 554 923 lines** (exactly as in
round 57), FREESTANDING **41/41**, KERNEL **46/46**, all five
counter-checks of the English migration at zero.

What the kernel could not get out of the language is named in
`docs/ROUND59.md` 8 and 9: no `static` (the mutable state lives in a
memory region whose address the prologue hands over), only ONE output
operand per `asm` (`rdmsr` puts edx:eax together inside the template), no
function pointers (an address is called with `asm("call rax", ...)`), no
`~`, and no line continuation. None of that stopped the kernel. Two
of those gaps are closed by now: round 58 made functions values, so
an address no longer has to be called through `asm("call rax")`, and
round 62 built the operating system on top of that list without
needing a single line of the compiler either.

## 46. Rounds 58, 59 and 60: functions as values, a kernel that runs, CSS

Three rounds in parallel, separated territories, reserved number ranges
(opcodes 50-59 / 60-69, slots 2200-2299 / 2300-2399, test numbers
870-889 / 890-899 / 910-939). The only conflict was in `test.sh`, where
round 58 and round 59 hung their section into the same place; both were
kept.

### Round 58: functions become values

`Type::Fn` was missing from `compiler/src/types.rs` — until now a
function was not a value. Now there are function pointers with strict
typing, closures that capture their environment, and the applications
that go with them (`vec.sort` with its own comparison, callbacks).

The dangerous part is the collector: a captured value the collector does
not see as a root gets swept while the closure is still using it.
`tests/873_closure_gc_root.fi` proves it does not happen. The second
claim is about the emitted code and is therefore measured on the emitted
code: `tools/fnval/run.sh` shows that a call which is statically known
stays a **direct** call, that there is exactly one `call rax` per
function value, and that the function record only appears where a
function value is actually used — in both compilers, with counter-checks.

### Round 59: the kernel really runs

The freestanding profile from round 52 got its purpose: IDT with real
exceptions (#DE, #PF, #GP, #DF) that print error code, CR2 and the
register set over the serial line, a tick counter driven by the timer, a
physical frame allocator plus a kernel heap out of the multiboot memory
map, keys over IRQ1, and the way into ring 3 and back over `syscall`.

None of that is claimed, it is measured: `tools/kernel/run.sh` boots
`demos/kernel/kmain.fi` in QEMU, once per case with a time limit, and
holds the serial output and the exit code against expectations —
**46 cases, 0 failed**, hanging as section 22 in `test.sh`. The
counter-checks are the interesting half: a masked IRQ0 counts zero ticks,
without keys nothing appears, and `hlt` in the user program yields a #GP
with `cs=0x2b`, which is the proof that the processor really was in ring
3.

The trap from round 52 was named in the assignment this time: line 2 of
`.gitignore` swallows `*.s`. `boot.s` and `isr.s` carry their own
exception lines and are under version control.

### Round 60: CSS

Syntax after css-syntax-3, selectors after selectors-4 with a matcher
that works from right to left the way the engines do, cascade with
specificity, origin, `!important` and inheritance. Measured against
**foreign** data, not against its own: **305/305** of the official
`css-parsing-tests`, **109/109** own cases for cascade and error
tolerance, and **840/840** match sets against `cssselect2` on the real
pages, specificities included. The soak run creates 9 095 801 GC objects
over 14 600 rounds and ends with **8 KiB** RSS growth.

The compiler was off limits for this round; language gaps were written
down instead of fixed. Throughput per page of `testdata/realweb/`:
66 409 instructions per element, measured with callgrind, because the
wall clock scatters by more than ten percent here.

### Acceptance in the main repository, measured by hand

`test.sh` **905/905** (from 847), self-comparison **246 same / 0
differing / 0 faulty**, fixpoint character-identical (**568 341 lines**,
3 342 224 octets in stage 2 as in stage 3), `CODEGEN MISSING: 0`,
tokenizer **6810/6810**, tree construction **150/150**, CSS **305/305 +
109/109 + 840/840**, KERNEL **46/46**, FREESTANDING **41/41**, PACKAGES
**21/21**, THREADS passed, and all five counter-checks of the English
migration at zero.

One more yardstick correction, the third of its kind: `also` stood in the
list of German function words and reported two entirely English sentences
of `docs/ROUND59.md` as German. A word that exists in both languages does
not belong in that list.

## 47. Rounds 61, 62, 63, 64: layout, an operating system, JavaScript, tooling

Four rounds in parallel on the base `a2a2ed4`, separated territories and
reserved number ranges (test numbers 940-969 / 970-999 / 1000-1049 /
1050-1099, opcodes 70-79 / 90-99). All four merged; the numbers below are
measured in the main repository after the merge, not taken from the
reports of the rounds.

### Acceptance in the main repository, measured by hand

`test.sh` **961/961** (from 905), self-comparison **259 same / 0
differing / 0 faulty**, `CODEGEN MISSING: 0`, fixpoint character-identical
(**573 568 lines** of assembly, 3 374 672 octets in stage 2 as in stage 3),
lowering comparison 156 same / 2 known deviations, tokenizer 6810/6810,
tree construction 150/150, CSS 305/305 + 109/109 + 840/840, FREESTANDING
41/41, PACKAGES 21/21, THREADS passed, FNVAL passed, and all five
counter-checks of the English migration at zero.

### Round 61 — the layout, measured against Chromium

Box model with margin collapsing, block flow and inline flow with line
breaking, on top of the cascade of round 60. The proof is not made
against expectations of its own: every box is compared with
`getBoundingClientRect()` in headless Chromium. **705 / 705 own boxes,
705 / 705 equal to Chromium, deviation 0.00 %** (section 23 of
`test.sh`). The soak run does 84 100 rounds in 60 s with **16 KiB** RSS
growth; the counter-check without collection grows by 90 076 KiB, so the
measurement means something. Throughput 66 409 instructions per element,
callgrind, because the wall clock scatters by more than ten percent here.

### Round 62 — the kernel becomes an operating system

Scheduler with context switch and preemption, processes in separate
address spaces with real memory protection, system calls, a RAM disk with
superblock, inodes and directories, and a small shell in ring 3. Proven in
QEMU: **174 cases, 0 failures** in eighteen sections (section 22, up from
the 46 of round 59). Twelve of those are counter-checks that have to
collapse when the thing under test is switched off — without the timer
each worker is scheduled exactly once, kernel memory touched from ring 3
gives `#PF` at `cr2=0x100000` with the process dead and the kernel alive,
a kernel pointer handed to `write` gives `-EFAULT` instead of being
followed, `wait` for a foreign pid gives `-ECHILD` instead of a hang, and
an unformatted `mount` is refused instead of reading foreign octets as
inodes. Not one line of either compiler was changed for it.

### Round 63 — JavaScript, measured against test262 without filtering

Lexer with automatic semicolon insertion, parser after ESTree, an
interpreter with scope chains, prototypes, property attributes and the
coercions, plus the built-in objects — all in Firn, nothing borrowed.
The measurement is the honest one: a case that uses a feature this engine
does not have counts as a failure like every other, nothing is filtered
out. Against tc39/test262 (63 364 cases of the manifest): **parser
44 341 passed, 69.98 %**, **engine 32 007 passed, 50.51 %**. The failures
by cause are in `tools/js/RESULTS.md`; the interesting column is `wrong` —
cases that ran through and delivered the wrong value — and it holds **9**.
The regression limits hang in `test.sh` as section 9d (3 053 / 3 493
parser, 2 317 / 3 493 engine on the fast subset).

Eight language gaps were written down instead of fixed. One of them is a
real bug and not a gap: **the two lexers read `9007199254740991.0`
(2^53-1) one ULP apart** — `firnc0` and the lexer written in Firn produce
different doubles for the same literal. `tools/lex_compare.sh` exists for
exactly that and struck the moment the constant appeared in
`lib/js/builtin.fi`. The same literal elsewhere does not diverge, so it
depends on the path through the number reader, not on the digits. The
engine sidesteps it by writing the value as an integer, but a lexer that
reads a literal differently from the one that bootstrapped it breaks the
fixpoint promise for every program using such a constant. That is worth a
round of its own.

### Round 64 — the tooling around the language

`firnfmt`, written in Firn, and its proof is the whole tree: **603 files
formatted, 0 changed by the shape, token stream differs 0, syntax tree
differs 0** over 507 comparable files, and the second run differs 0 —
idempotent. Error messages with a source excerpt and suggestions in both
compilers (twelve negative tests). Real **DWARF**, verified in actual gdb
sessions: **48 passed, 0 failed**. And a language server, `firnc --lsp`,
checked by a real LSP client: **25 passed, 0 failed** (sections 24, 25,
26).

### What the merge cost this time

Nothing dramatic in the code — the territories held. Three things had to
be pulled straight afterwards, and all three are the same class of
mistake as ever: **section numbers in `test.sh` collided** (four rounds
each appended "the next" section, so layout, formatter, debug info and
language server had to be renumbered to 23-26), **the new sources were not
in canonical shape** (round 64 produced the formatter, rounds 61-63 wrote
their sources before it existed — `firnfmt -w` over the tree, line count
per file unchanged), and the name check found **nine German paths** it had
been blind to because a compound written as one word is ONE morpheme:
long morphemes are now searched inside words as well.

### One number that only holds on a quiet machine

`tests/860_thread_basic.fi` failed once during these runs. Case C of that
test demands that the counter WITHOUT a lock LOSES increments — otherwise
the proof that the mutex works would be worthless. Under five test suites
running at once the four threads got serialised, the unlocked counter came
out exact, the counter-check did not strike, and the test rightly reported
a failure. Ten runs on a quiet machine give ten passes. It is written down
because a number that only holds on a quiet machine is worth writing down.

## 48. Rounds 65 and 68: the number reader and the language gaps

Two rounds that ran in parallel with three others and were merged together
(`d4317c1c`). Both touched the compilers; the three others (`lib/js/`,
`lib/layout/`, `lib/std/`) were kept out of that territory on purpose, and
that separation held — the merge produced exactly ONE conflict.

### Acceptance in the main repository, measured by hand

`test.sh` **1 011 of 1 012** (from 961; the one failure is named below),
self-comparison **269 same / 2 differing / 0 faulty**, `CODEGEN MISSING: 0`,
fixpoint character-identical (**582 257 lines** of assembly), number reader
**0 differing** against firnc1, against C `strtod` and against Python,
KERNEL 174/0, FREESTANDING 41/41, PACKAGES 21/21, LAYOUT 705/705 against
Chromium, FNVAL and FNFIELD passed, DWARF 48/0, LSP 25/0, and all five
counter-checks of the English migration at zero.

### Round 65 — a literal that two lexers read differently

Round 63 had written it down as gap 8: `9007199254740991.0` (2^53-1) came
out of `firnc0` and out of the lexer written in Firn as two DIFFERENT
doubles. This round fixed the reader and, more importantly, built the proof
that it stays fixed: `tools/lexnum/` puts **four** number readers side by
side — firnc0, firnc1, C `strtod` and Python — over 4 044 floating point and
1 250 integer literals, including subnormals, halfway cases, long digit
strings and the extremes. **Zero deviations in every pairing**, 3 749 of
them through the exact path. `tools/lex_compare.sh` now reports 0 known
deviations where it reported 1.

### Round 68 — the lists that two rounds had left standing

Rounds 59 and 63 both ended with "what does Firn itself lack", and neither
list was worked off. This round worked them off, in BOTH compilers: `~`,
line continuation, calling a function value that sits in a struct field
(`c.hook(a, b)` instead of `let h = c.hook`), the free `Gc[T]` upcast at
`return`, and more than one output operand for `asm`. Two bugs fell out
along the way that nobody had been looking for: `-x` on `f64` and the error
union `E!f64`.

The fixpoint is the number that counts here — 582 257 lines, stage 2
identical to stage 3, character for character, after a round that changed
the language itself in two compilers.

### The same collision for the fourth time

Both rounds had numbered their new section of `test.sh` **27**. The number
reader moved to 28. Reserving opcode ranges and test numbers was the lesson
of rounds 49 and 54; it still does not cover section numbers in `test.sh`,
because a round appends "the next free one" and two rounds see the same
free one. Whatever is handed out in sequence has to be handed out from a
reserved range — sections included.

### The one failure, and why it is not a merge fault

`tools/self_compare.sh` reported 2 of 271 differing, the first being
`tests/834_arc_thread.fi` (firnc0 exit 9, firnc1 exit 0). That test is
generated by `lib/rc/gen_tests.sh` and buys its sharpness with a RACE: a
counter WITHOUT a lock has to lose increments, otherwise the proof that the
lock works would be worthless. Four other rounds were compiling on the same
eight cores while this acceptance ran (load average 6.5 to 8.8). Under that
load the threads get serialised, the unlocked counter comes out exact, the
counter-check does not strike, and the test rightly reports a failure.

Run on its own on the same machine: **12 of 12 green**. Round 68 had
already described the same effect for `tests/860_thread_basic.fi`.

That makes it a flaw in the TEST, not in the code under test: a proof that
only holds when the machine is idle is not a proof. The honest fix is to
give the counter-check a floor — repeat until the raw counter loses at
least once, with an upper bound, and fail only if it never does. That is a
change to a test of rounds 47/49 and belongs into a round of its own.

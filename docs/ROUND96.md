# Round 96 — a name that is already there

## 1. The find

Three lines:

```firn
const A: u64 = 1
const A: u64 = 2
fn main() -> i32 { return A as i32 }
```

`firnc0` refuses them:

```
error: constant 'A' is already declared
  --> /tmp/dup.fi:2:1
   |
 2 | const A: u64 = 2
   | ^^^^^ here
```

`firnc1` translated them. Exit code 0, no message, and the program returned
**1** — the first declaration had won and the second had been thrown away
without a word.

It was not a curiosity. It happened, in this repository, on the day the
round started. Merging rounds K4 and K5 left `const KDATA_SIZE` twice in
`demos/kernel/kstate.fi`: `0x30000` from K4 and `0x20000` from K5. `firnc0`
stopped the build and made the collision visible. `firnc1` built it — with
the **smaller** value. K4 puts its tables for open files, descriptors,
contexts and pipes at `0x20000..0x24FFF`, that is to say **outside** the data
area it had just been given. The kernel ran. While it ran it overwrote
memory belonging to somebody else.

That this was noticed at all was luck: `firnc0` happens to be tested
alongside. Without that it would have become a ghost one hunts for weeks.

It is the same class as the bug of round 90: two compilers, two truths, and
the difference shows up by accident.

## 2. The cause, in one sentence

`firnc1` wrote every name into its tables with a plain `push` where `firnc0`
asks first — `insert` instead of `insert if not there, complain if there`.

Where the check existed at all it was there but its result was thrown away.
`pattern.fi::pat_enum_add` and `pat_variant_add` **did** count a duplicate
enum and a duplicate variant into `(*m).err` — and nobody ever read that
counter: `sema_run` only ever asks `pat_layout` for the errors it added
*itself* (`(*m).err - before`, sampled after the parser was long done). The
count was raised and dropped in the same run.

The places, one by one:

| where | what it did |
|---|---|
| `sema.fi` step 4 (constants) | `vec_push` without asking |
| `sema.fi` step 5 (function bodies) | `var_create` without asking — no scope check at all |
| `types.fi::types_resolve` step 1 | struct names pushed without asking |
| `types.fi::types_resolve` step 2 | field names never compared inside a struct |
| `types.fi` (signatures) | parameter names never compared inside a function |
| `pattern.fi::pat_enum_add` | asked, counted — counter never read |
| `pattern.fi::pat_variant_add` | asked, counted — counter never read |
| `parser.fi::import_decl` | the same module twice in one file, pushed without asking |

Two consequences of the same root that had nothing to do with names:

* Two functions of the same name reached the **code generator**. It emitted
  two labels `_F1.f`, and the failure surfaced as `as: symbol '_F1.f' is
  already defined`, exit code 7 — an assembler message for a language error.
* `defer { return 1 }` was not refused either, and the **lowering** then
  walked off the end: `firnc1` did not print a message, it died of
  **SIGSEGV** (`tests/neg/defer_return.fi`, pre-existing, found by the new
  test of this round).

## 3. Every declaration kind of the language, written twice

Each row is one program in which exactly one thing is declared twice. `same`
means the whole message block — sentence, arrow line, source line, marker —
is character-identical in both compilers.

| declaration kind | firnc0 | firnc1 before | firnc1 after | message | what firnc0 says |
|---|---|---|---|---|---|
| `const` twice | refuses | **TAKES IT** | refuses | same | `constant 'A' is already declared` |
| `static` twice | refuses | refuses | refuses | same | `'A' is already declared` |
| `static` against `const` | refuses | refuses | refuses | same | `'A' is already declared` |
| `fn` twice | refuses | rc=7 (`as`) | refuses | same | `function 'f' is already declared` |
| method twice in one `impl` | refuses | rc=7 (`as`) | refuses | same | `function 'S__m' is already declared` |
| parameter twice | refuses | **TAKES IT** | refuses | same | `parameter 'a' is already declared` |
| closure parameter twice | refuses | **TAKES IT** | refuses | same | `'a' is already declared in this block` |
| local twice in one block | refuses | **TAKES IT** | refuses | same | `'x' is already declared in this block` |
| local against parameter | refuses | **TAKES IT** | refuses | same | `'n' is already declared in this block` |
| pattern binds twice | refuses | **TAKES IT** | refuses | same | `'x' is already declared in this block` |
| `struct` twice | refuses | **TAKES IT** | refuses | same | `struct 'S' is already declared` |
| field twice in one struct | refuses | **TAKES IT** | refuses | same | `field 'a' is already declared in struct 'S'` |
| `enum` twice | refuses | **TAKES IT** | refuses | same | `enum 'E' is already declared` |
| variant twice in one enum | refuses | **TAKES IT** | refuses | same | `variant 'A' is already declared in enum 'E'` |
| `struct` against `enum` | refuses | **TAKES IT** | refuses | same | `struct 'S' is already declared` |
| `struct` against error set | refuses | **TAKES IT** | refuses | same | `struct 'S' is already declared` |
| `impl I for T` twice | refuses | refuses | refuses | same | `'T' already implements the interface 'I'` |
| `import` twice in one file | refuses | **TAKES IT** | refuses | same | `module 'm' is imported more than once` |
| error set twice | refuses | refuses | refuses | silent | `error set 'E' is already declared` |
| error variant twice | refuses | refuses | refuses | silent | `error variant 'A' is already declared in error set 'E'` |
| error set against `enum` | refuses | refuses | refuses | silent | `type 'E' is already declared` |
| `gc class` twice | refuses | refuses | refuses | silent | `'gc class C' is already declared` |
| field twice in a `gc class` | refuses | refuses | refuses | silent | `field 'a' is already declared in 'gc class C'` |
| `interface` twice | refuses | refuses | refuses | silent | `the interface 'I' is already declared` |
| generic `fn` twice | refuses | refuses | refuses | silent | `generic function 'f' is already declared` |
| generic `struct` twice | refuses | refuses | refuses | silent | `generic struct 'G' is already declared` |
| type parameter twice | refuses | refuses | refuses | silent | `type parameter 'T' is already declared` |
| the same bound twice | refuses | refuses | refuses | silent | `the bound 'O' appears twice on 'T'` |
| `struct C` next to `gc class C` | **takes it** | takes it | takes it | — | (both take it; see section 7) |

**Twelve rows** were a real divergence: `firnc0` refused, `firnc1`
translated. Two more (`fn`, method) were refused only by accident, by the
assembler, with a message about a symbol instead of about a name. All
fourteen are closed. `silent` means: `firnc1` refuses it and stops with exit
code 1, but says nothing — an old state of affairs of stage 1 that this round
did not extend (section 7).

## 4. What was built

**`lib/firnc1/decl.fi`** — a new module, the duplicate check in one place.
The checks it mirrors sit scattered over five files in `firnc0`
(`sema.rs::collect_structs`, `collect_fns`, the constant and `static`
passes, `declare_var`, `sema_match.rs::types_enum_decl`,
`iface.rs::check_impl`, `parser.rs::import_decl`); here they are one pass
that collects findings and renders them at the end through `diag.fi`, the
twin of `compiler/src/diag.rs`. That is the same way out `escape.fi` has
taken since round 79, and it is why the marker under the message is as long
as `firnc0`'s.

The findings are collected in the **order stage 0 produces them**: the
imports and the enums with their variants first (stage 0 finds them while
parsing), then all struct names, then all fields, then per function its
parameters and its own name, then the constants, then the global variables,
then the duplicate implementations, and last — from `sema.fi`, while the
bodies are walked — the locals. A file with several errors therefore reads
the same in both compilers, down to the line `2 errors found`.

Positions had to be added for it. Until this round the tree carried a
position for expressions, statements and parameters only (round 79, for the
escape analysis); a declaration had none, and a message without a line is
half a message:

* `ast.fi`: `c_pos`, `g_pos`, `st_pos`, `fd_pos`, `f_pos`, `i_pos`
  (and `s_length`, see below), each set by the parser.
* `pattern.fi`: `e_pos`, `v_pos` for the enum and the variant name,
  `mu_pos` for a pattern binding.
* `iface.fi`: `u_pos` for the interface name of an `impl I for T`, plus the
  list of duplicate implementations `sema.fi` reads off after `if_check`.

**Where the marker sits** follows `firnc0` exactly and differs per kind: the
KEYWORD for `const` (5), `static` (6), `struct` (6), `fn` (2), `import` (6);
the NAME for a field, a parameter, an enum, a variant, a pattern binding and
the interface of an `impl`; and for a `let`/`var` the WHOLE STATEMENT.

The last one is why `s_length` exists. `firnc0` builds it with
`Parser::join`: keyword column to the last column of the initializer, as
long as both are on the same line, and the keyword alone otherwise. The
parser of stage 1 now measures the same span out of the token stream
(`span_width`) and stores it on the statement.

**`sema.fi`** — `var_create` now takes a position and a width and asks the
INNERMOST scope before it writes. That is `sema.rs::declare_var`, and it is
what makes shadowing in a nested block stay legal while `let x` after `let x`
in one block does not. All six call sites hand over the place `firnc0` blames:
parameters and pattern bindings their name, `let`/`var` the whole statement.

**`sema.fi::defer_jump`** — twin of `sema.rs::defer_jump`. A `return`
(and a `break`/`continue` that does not belong to a loop begun inside the
body) leaving a `defer` is refused instead of walking the lowering off the
end. This is what turned `tests/neg/defer_return.fi` from a segmentation
fault into a rejection.

**`parser.fi::import_decl`** — the same module twice in ONE file. The
comparison runs over the imports whose recorded position carries this file's
number, because the tree holds the imports of the whole program at once and
`std.io` in two different modules is perfectly ordinary — it is what every
build in this repository does.

## 5. The gap the round really closes

The repository was full of tests comparing what the two compilers
**produce**: the same tokens (`lex_compare.sh`), the same tree
(`parser_compare.sh`), the same types (`sema_compare.sh`), the same FIR
(`fir_compare.sh`), the same binary down to the octet (`fixpoint.sh`). Not
one compared what the two compilers **refuse**. A compiler is as much the
programs it turns away as the programs it translates, and nothing was
watching that half.

**`tools/reject/run.sh`**, section **60** of `test.sh`.

The corpus is `tests/neg/*.fi` — **exactly** the list section 4 already
walks, deliberately not a second one. The lesson of round 90 was that two
lists asking the same question drift apart; a new faulty program now lands
in the comparison by itself and nothing has to be kept in step by hand. The
fifteen duplicate programs of this round went into `tests/neg/` for the same
reason, where section 4 checks their message against `firnc0` as well.

Three demands, of decreasing strength:

1. `firnc0` refuses it (the precondition — an entry that is not faulty
   proves nothing).
2. **`firnc1` refuses it too.** This is the invariant that matters:
   whatever the wording, no program stage 0 turns away may be translated by
   stage 1.
3. Where `firnc1` **says** something, it says exactly what `firnc0` says —
   compared with `cmp`, the arrow line, the source line and the marker
   included.

Exceptions to 2 and 3 stand in the script **by name and with a reason**, and
the script fails when a named file no longer needs naming. The lists can
only shrink. A signal from `firnc1` (a crash) counts as a failure, not as a
rejection — that is how the `defer` segmentation fault was found.

What it measures today:

```
CORPUS:        209 faulty programs (tests/neg/*.fi, the list of section 4)
SAME:          20  refused by both, message identical character for character
SILENT:        158 refused by both, firnc1 without a sentence of its own
NOT CORE:      10  firnc1 stops before the program (rc>=3), not comparable
SWALLOWED:     17  firnc0 refuses, firnc1 translates -- named holes
WORDING:       4   both refuse, firnc1 in its own words -- named
REFUSED BY BOTH: 192 of 209
```

## 6. The fixpoint

The self-hosted compiler was changed, which means the assembly it emits
changes. Stages 2 and 3 are character-identical, which is the only proof
that counts:

```
STAGE 2: 16942 ms   4720752 octets
STAGE 3: 49018 ms   4720752 octets
FIXPOINT:  stage 2 == stage 3, character-identical (792802 lines of assembly)
CORPUS:    .firnc2 behaves like firnc0
```

A worry worth naming: the new check walks **every** function of the tree,
and the tree holds the monomorphized instances (`mono.fi`) and the generated
closure functions as well. Instances cannot collide — `already_done` guards
that — but closures had to be taken out of the parameter and function loops
explicitly: stage 0 builds a closure's function in `fnval.rs` **after** the
type check, so `collect_fns` never sees it and a closure with two parameters
of one name is reported once, by the scope check, and not twice.

## 7. What is still open — named

**Seventeen programs in `tests/neg/` are still translated by `firnc1`**
while `firnc0` refuses them. Every one stands in `tools/reject/run.sh` by
name with its reason. None of them is a duplicate declaration; they are
checks stage 1 does not carry at all:

* the mutability of a local — `let` against `var` (`assign_let`,
  `assign_op_let`, `step_let`)
* `#[no_gc]` through a closure, and writing to a capture
  (`877_closure_nogc`, `887_closure_write_capture`)
* the argument types of the atomic primitives (`atomic_ty`, `atomcas_ty`)
* `break` outside a loop, `export` of a name that is not there
* six rules of the kernel profile (`#[interrupt]`, `#[allow_fp]`, an
  unknown profile name)
* a literal without context that does not fit into `i32`
* a path through a function without a `return`

**Four programs are refused by both, in different words.** Both are older
deviations of their own subsystem and neither is a duplicate declaration:
`time.fi` prints its `comptime` messages without a position, and
`firnc1.fi::complain_std_module` has no source map for imports. They are
named in `tools/reject/run.sh` too.

**Eight duplicate kinds are refused without a sentence** (the `silent` rows
of the table in section 3): error sets and their variants, `gc class` and
its fields, `interface`, generic functions and structs, type parameters and
bounds. These are **not** holes — the program is turned away, with exit code
1 — but the message cannot be compared, because those checks sit in
registries (`err.fi`, `gc.fi`, `mono.fi`) that only count. Giving them the
same way out through `decl.fi` is a small, mechanical round of its own; this
one did not do it.

**`struct C` next to `gc class C` is taken by BOTH compilers.** A `gc class`
is declared in the type table under the name `"gc C"`, so the plain name `C`
stays free. The two are then different types with the same name in the
source text. It is not a divergence — the two compilers agree — but it is a
question `firnc0` has not answered, and it is written down here so that it
does not have to be found twice.

**A multi-line `let`.** `Parser::join` in stage 0 gives up when the two ends
of a span lie on different lines, and it keeps whatever the left-hand side
had — which for `let q: i32 = f(1,` followed by `2)` is the span up to the
callee, fourteen characters. `span_width` in stage 1 cannot reproduce that
without giving every expression a width of its own, and it falls back to the
keyword. The line and the column are right, the marker is shorter. It shows
only where a `let` whose initializer runs over a line break is declared
twice, and there is no such program in the corpus.

## 8. Files

New:

* `lib/firnc1/decl.fi` — the duplicate check, one place, with the messages
* `tools/reject/run.sh` — what the two compilers refuse, compared
* `tests/neg/dup_*.fi` — sixteen faulty programs, one per kind

Changed:

* `lib/firnc1/ast.fi` — positions for declarations, width for a statement
* `lib/firnc1/parser.fi` — records them; the import check; `span_width`
* `lib/firnc1/pattern.fi` — positions for enum, variant, binding
* `lib/firnc1/iface.fi` — position of the `impl`, the list of duplicates
* `lib/firnc1/mono.fi` — an instance inherits the position of its template
* `lib/firnc1/sema.fi` — the scope check in `var_create`, `defer_jump`,
  the two calls into `decl.fi`
* `test.sh` — section 60

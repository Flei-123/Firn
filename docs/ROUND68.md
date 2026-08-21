# Round 68 -- the language gaps that earlier rounds wrote down instead of closing

Branch `r68-gaps`, base `aa65fdc`.

Rounds 59 and 63 both ended with a list headed "what does Firn itself lack".
Neither list was worked off; both were written down honestly and left
standing. This round works them off. It touches **only** the two compilers
and the tests -- `lib/js/`, `lib/layout/` and the number reader of the lexer
belong to three rounds running in parallel and were not touched.

**Result up front, measured (21.08.2026, in this worktree):**

| | base `aa65fdc` | round 68 |
|---|---|---|
| `bash test.sh` | 961/961 | **996 checks, 992-995 green** -- see 9 |
| `bash tools/self_compare.sh` | 259 / 0 / 0, codegen missing 0 | **266 / 0 / 0, codegen missing 0** |
| `bash tools/fixpoint.sh` | character-identical, 573 568 lines | **character-identical, 578 170 lines** |
| `bash tools/kernel/run.sh` | 174 passed, 0 failed | **174 passed, 0 failed** |
| `bash tools/freestanding/run.sh` | 41 passed, 0 failed | **41 passed, 0 failed** |
| `bash tools/english/check.sh` | 0 0 0 **828** 0 (see 7) | **0 0 0 0 0** |
| `tools/lex_compare.sh` | 561 same / 1 different (known) | **581 / 1 (known)** |
| `tools/parser_compare.sh` | 317 / 2 (known) | **333 / 2 (known)** |
| `tools/types_compare.sh` | 266 / 0 | **281 / 0** |
| `tools/sema_compare.sh` | 159 / 1 (known) | **163 / 1 (known)** |
| `tools/fir_compare.sh` | 156 / 2 (known) | **160 / 2 (known)** |
| `tools/fnval/run.sh` (round 58) | passed | **passed** |
| `tools/fnfield/run.sh` (new, section 27) | -- | **passed** |
| `tools/thread`, `packages`, `fmt`, `dwarf`, `lsp` | passed | **passed** (21/0, 48/0, 25/0) |

**No new deviation in any of the five comparison tools.** The counts rise
because the corpus grew by the tests of this round; the number of deviations
does not.

The 35 checks more are 7 new positive tests in three build stages each (21),
13 new negative tests, and the new section 27.

**Why the test count is a range and not a number.** Three other rounds were
compiling on the same machine while this one was measured (load average 9 to
12 on 8 cores). Two tests of round 47/49 buy their sharpness with a RACE and
lose it under that load -- they are named in 9, they are octet-identical to
the base commit, and they pass 10/10 to 25/25 when the machine is quiet.
Every other section was green in every one of the three acceptance runs.

**The fixpoint is the number that matters.** 578 170 lines of assembly,
stage 2 identical to stage 3, character for character -- with a language that
changed in five places. Whoever changes only one of the two compilers loses
the fixpoint on the spot; that it stands is the proof that every change
below really landed in both.

---

## 1. What was open -- and what is closed now

`docs/ROUND59.md` section 8/9 (the kernel) and `docs/ROUND63.md` section 4
(the JavaScript engine) between them named eleven things. Here is every one
of them with its outcome.

| | gap | outcome |
|---|---|---|
| R63-1 | no unary minus for `f64` | **closed** (3) |
| R63-2 | `E!f64` returns a WRONG value, silently | **closed** (4) |
| R63-3 | a binary operator may not begin a line | **closed** (2) |
| R63-4 | `gc class` has no methods, no generic form | **open**, measured, see 8 |
| R63-5 | no global variables, not even immutable ones | **open**, deliberately, see 8 |
| R63-6 | a function value in a struct field is not callable | **closed** (5) |
| R63-7 | `Gc[T]` does not upcast implicitly at `return` | **closed** (6) |
| R63-8 | the two lexers read a decimal literal one ULP apart | **not touched** -- round 65 |
| R59-a | `~` (the bitwise complement) is missing | **closed** (1) |
| R59-b | line continuation is missing | **closed** (2) |
| R59-c | `asm` has exactly ONE output operand | **closed** (7) |
| R59-d | no `static` (identical to R63-5) | **open**, see 8 |

`docs/ROUND59.md` section 9 also lists APIC, keyboard driver, address space,
scheduler, file system and the backwards merge of the kernel heap. Those are
not gaps of the LANGUAGE but of that kernel; round 62 closed a part of them.
They are out of scope here and are named so that nobody looks for them.

Numbers reserved for this round: **tests 1230-1279**, of which 1230-1249 are
used. Section number in `test.sh`: **27** (1-26 were taken; 27 was free).
No opcode, no FIR instruction and no interpreter slot was newly numbered --
the changes below get by with what is there. The one new number inside an
instruction is documented in 7.

---

## 2. `~` and the line continuation -- the two things the kernel felt

`docs/ROUND59.md` section 8 closed with:

> Two things the language does not have and the kernel felt: **`~`** (the
> bitwise complement; written here as `x ^ 0xFFFF…`) and **line
> continuation** -- an expression has to stand on one line, which is why the
> descriptor fields are assembled with `var … =` and `|=`-like steps.

### `~`

A prefix operator of its own, deliberately **not** the same one as `!`:

* `!` is the logical negation of a `bool`. `!0u8` stays a type error.
* `~` is the bitwise complement of an integer. `~true` is a type error.

Both directions are held fast by a negative test with line and column
(`tests/neg/1064_tilde_needs_integer.fi`,
`tests/neg/1231_not_needs_bool.fi`). The FIR needed nothing: `Op::Un` with
`UnOp::Not` already emitted `not` for an integer type and `xor eax, 1` for
`bool` -- only the surface and the type rule were missing.

`tests/neg/1064_hint_tilde.fi` held the LEXER hint "Firn has no '~'" up to
round 67. It is now `tests/neg/1064_tilde_needs_integer.fi` and holds the
type rule instead -- the number stays, the file keeps its place in the hint
series of round 64.

The kernel uses it: `demos/kernel/user.fi::not` no longer spells out the
width of the word.

### The line continuation

The rule, in `SPEC.md` 12.1 and in one sentence: **an expression is
continued over a line break when the first token of the following line can
only ever stand BETWEEN two operands.** That is a fixed list:

```text
+  -  /  %  &  |  ^  <<  >>  &&  ||  ==  !=  <  <=  >  >=  .  as
```

`*`, `(` and `[` are deliberately NOT in it, and that is the whole point:

```firn
let g: usize = (*p).field
*q = 0            // a STORE, not a multiplication of the line before
(*r).tag = 99     // an assignment, not a call of the line before
```

Before the rule was written, the corpus was measured: of all `.fi` files in
the repository, exactly 34 lines outside comments begin with one of the
candidate tokens, and every single one of them stands INSIDE brackets, where
a line break has never ended anything. `*` at the start of a line, on the
other hand, occurs in over a hundred places -- which is what settled the
list. `tests/1232_line_continuation.fi` proves both halves in one file,
`tests/neg/1233_star_is_no_continuation.fi` the counter-check.

An operator at the END of a line has continued the expression since round 2
and goes on doing so; nothing there changed.

---

## 3. `-x` on an `f64`

`docs/ROUND63.md`, gap 1: "SPEC 14.1.f64 names 'the sign `-x`' as
implemented; it is not."

It was a type checker rule, nothing else. `codegen_x86.rs` has done the
right thing for `FTy::F64` since round 11:

```
mov rcx, -9223372036854775808
xor rax, rcx
```

That is BIT 63 and not `neg`, and that difference is the reason to do it at
all: `-0.0` has to come out as `-0.0`, and `0.0 - x` does **not** give that
(`0.0 - 0.0` is `+0.0`). `tests/1247_f64_negation.fi` checks exactly that,
over the bit pattern.

`lib/firnc1/codegen.fi` had a placeholder at that spot ("if it ever does:
flip the sign bit") that set `unsuitable`. It flips the bit now, which is
why the file does not turn up under CODEGEN MISSING in the self comparison.

`~` on an `f64` stays an error (`tests/neg/1248_f64_has_no_bitnot.fi`): a
floating point value has no bits whose flipping would mean anything.

---

## 4. `E!f64` -- the silent one

`docs/ROUND63.md`, gap 2, ends with: "This bug is dangerous because it is
SILENT." It was, and it was not where the report suspected.

The suspicion was the System V classification of `{ __err: u32, __val: f64 }`
against the f64 ABI of stage 0. Wrong: `abi.rs` never produces the SSE class
at all, and the FIR of the producing function is correct
(`store.f64 %8, %5`, return through the hidden pointer).

The cause was a **copy**. `lower_errors.rs` carried a `scalar_fty` of its
own -- a second table for "which FIR type does this source type have" -- and
that table did not know `Type::F64`. `copy_value` then found `None`,
interpreted it as "nothing to copy" and copied nothing. The success value
never reached its slot; what came out was whatever lay there:

```
bb1:                      ; success
  %9 = ptradd.ptr %2, 8   ; the address of __val -- and then NOTHING
  br bb3
bb3:
  %11 = load.f64 %0       ; a slot nobody ever wrote
```

`lib/firnc1/lower.fi` had the same hole, and it was written in
DELIBERATELY, with a comment: "Like `scalar_fty` in lower_errors.rs: f64 is
DELIBERATELY not part of the pattern there -- no program in the corpus has
`E!f64`; same here." Bug compatibility as a design decision, which is why
the self comparison never struck.

Both are gone. `lower_errors.rs` calls `lower::scalar_fty_pub` now, so there
is exactly ONE table; a type that gets added to the language cannot be
forgotten in the second place a second time. The `void` case that the local
copy handled by accident is kept in a two-line wrapper, with the reason
written down.

`tests/1004_js_f64_union.fi` held the bug fast up to round 67 and said "if
this check ever fails, the compiler has been fixed". It has been; the file
now checks the fixed behaviour and says where the cause lay.
`tests/1249_error_union_f64.fi` adds the cases around it: `try` over several
levels, the error path, `catch |e|`, and `-0.0` through the union -- that
last one is the one that would have looked right with a copy of nothing.

**The workaround in `lib/js/interp.fi` stays as it is.** It is still
correct, it is merely no longer forced -- and `lib/js/` belongs to another
round this week.

---

## 5. `c.hook(a, b)` -- a function value in a struct field

`docs/ROUND63.md`, gap 6. Up to round 67 the value had to be loaded into a
local first (`let h = c.hook; h(a, b)`).

**The resolution order is unchanged, and that is the whole design.** The
field is looked at only AFTER the method lookup has failed:

1. `dyn I`? -> dynamic dispatch (round 46).
2. Is there a method `T__hook`? -> that one, statically (round 45).
3. Is there a FIELD `hook` of function type? -> through the value (round 68).
4. Otherwise the message of round 45, unchanged, with line and column.

So no existing program changes its meaning. `tests/1238_fnfield_call.fi`
has a type `Named` that carries BOTH a field `twice` and a method `twice`,
and checks that the method wins.

**The receiver is not an argument.** It is only the place the value is read
from: `c.hook(a, b)` lowers to the field address, the record pointer out of
it, the code address out of word 0 of the record -- and then the call with
`a, b` plus the record as the last argument, exactly as round 58 defined it.

### The proof on the emitted code

`tools/fnfield/run.sh`, section 27 of `test.sh`, built the way
`tools/fnval/run.sh` (round 58) is built, because the claim is about the
emitted code and not about prose:

* `direct(…)` -> `call _F0.add`, no `call rax`, no function record.
* `c.hook(a, b)` -> **exactly one** `call rax`, no direct call.
* `n.twice(x)` with a field AND a method of that name -> `call
  _F0.Named__twice`, **direct**, and no `call rax`. The resolution order,
  read off the machine code.
* the counter-check: the same program without a function value contains no
  indirect call at all.
* under the optimiser nothing that was direct becomes indirect.
* both compilers, and the program runs and yields 7 in both.

A closure without captures in a field is checked there as well -- and
deliberately NOT in `tests/`: the generated `__closure#N` functions stand in
a different place in the two compilers, which is the known deviation of
round 58 (`tools/fir_compare.sh`, `tests/871_closure_plain.fi`). Putting a
closure literal into a corpus file would have added a third entry to a list
that is meant to stay short.

---

## 6. `Gc[Derived]` into `AllocError!Gc[Base]`

`docs/ROUND63.md`, gap 7, "roughly 200 places in this round".

One place too many again, and the same shape as 4: `sema::assignable` knew
the free upcast of SPEC 4.4, but every conversion INTO AN ERROR UNION runs
through `errors.rs::hook_coerce`, and that one asked a private
`compatible` of its own which did not. So

```firn
let up: Gc[Base] = derived      // allowed
return up                       // allowed
return derived                  // rejected -- with the same types
```

`lib/firnc1/types.fi::compatible` has had the rule since round 54, so the
two compilers disagreed here as well; they agree again now.

Upward only. `tests/neg/1237_gc_upcast_only_upward.fi` holds the other
direction fast -- downwards there is still exactly one way, the checked
`x.as?[C]`. `tests/1236_gc_upcast_return.fi` runs it over two levels of
inheritance, at `return`, in a `let`, as an argument and behind `catch`,
and gets the object back with `.as?[…]` to show that it really is the
derived one.

**The ~200 detours in `lib/js/` were deliberately left alone.** Another
round is working there; the language can do it now, and that is what this
round owed.

---

## 7. `asm` with several outputs, and memory as a target

`docs/ROUND59.md` section 8:

> **`asm` has exactly one output operand.** `rdmsr` delivers its result in
> two registers (edx:eax). The way out was not a change to the language but
> a template that puts them together itself … a second `out` would be the
> better form, and the kernel is the first user who would notice.

The form, in `SPEC.md` 14.1.asm:

```firn
out("rax")            // the VALUE of the expression -- at most one (round 52)
out("rdx") p          // the register into *p, after the template (round 68)
```

Any number of the second kind. The kernel is the first user, as predicted:

```firn
fn rdmsr(index: u64) -> u64 {
    var lo: u64 = 0
    var hi: u64 = 0
    asm("rdmsr", in("rcx") index, out("rax") &lo, out("rdx") &hi)
    return (hi << 32) | (lo & 0xFFFFFFFF)
}
```

instead of `asm("rdmsr\nshl rdx, 32\nor rax, rdx", …, clobber("rdx"))`.

### The three rules, each with a negative test

* **Eight octets.** What is written is always the WHOLE register, so the
  target has to be eight octets wide. A `*mut u32` would have its neighbour
  in the frame overwritten silently -- refused instead
  (`tests/neg/1246_asm_out_width.fi`).
* **A register of its own per output.** Two outputs on `rax` would lose one
  of the two results without a word being said
  (`tests/neg/1243_asm_out_twice.fi`).
* **The value form stays single.** An expression has one value
  (`tests/neg/1244_asm_two_bare_out.fi`). And an output operand is an
  ADDRESS, not a value (`tests/neg/1245_asm_out_needs_pointer.fi`).

### The order in the emitted code, and why

```
    rdmsr
    push rax          ; every result register goes on the STACK first
    push rdx
    pop  rax          ; and only then are rax/rcx used to write it out
    mov  rcx, qword ptr [rbp-24]
    mov  qword ptr [rcx], rax
    pop  rax
    mov  rcx, qword ptr [rbp-8]
    mov  qword ptr [rcx], rax
```

The other way round would destroy a result that is still to be written: an
address has to be loaded into a register, and that register may itself be an
output. The VALUE form is written to its frame slot BEFORE the pushes,
because `mov [rbp-N], reg` needs no scratch register at all. The frame is
`rbp`-based, so `push`/`pop` cannot disturb a slot.

### Memory as an operand

A memory AREA travels as a pointer in a register (`in("rcx") p`) plus
`clobber("memory")`; the template writes `mov qword ptr [rcx], 5`. There is
no placeholder syntax -- what stands in the template is x86 assembly and
nothing else. `tests/1242_asm_multi_out.fi` sections 6 and 7 check both
directions.

### The one new number

`lib/firnc1` has fixed fields per FIR instruction, so the count of the
memory outputs needed a place. It sits in the **upper 32 bits of the
`number` field** of `O_ASM`; the lower half goes on carrying the clobber
count, as before. The argument block is now: input pairs, then output pairs,
then clobbers. `firnc0` keeps them in two `Vec`s of their own
(`out_regs`/`outs` in `fir::Op::Asm`) and needs no packing. Both print the
same FIR text.

---

## 8. What stays open, and why

**`gc class` has no methods (R63-4) -- and the two compilers disagree.**
Measured in this round, because the report of round 63 did not say it that
precisely:

* `firnc0` **already accepts** `impl C { fn m(*mut self, …) }` for a `gc
  class` and resolves `g.m(x)` on a `Gc[C]`. It works, it runs.
* `firnc1` **refuses** the same program -- exit code 1, without a message.
* **Inherited methods work in neither.** `impl Base { fn get(…) }` plus
  `d.get()` on a `Gc[Derived]` reports "type 'Derived' has no method 'get'"
  in both. The receiver would upcast for free (SPEC 4.4); the lookup does
  not walk the base chain.

That is a round of its own: the base chain in three places (type check,
lowering, preview), a rule for what a method on the base means for the
derived type, and the silent exit of `firnc1` cleaned up. Naming it here is
worth more than half of it in this one. A generic `gc class` is not touched
by any of it.

**No global variables (R63-5, R59-d).** Deliberately left standing, and
written into `SPEC.md` 14 item 5 with the reason: a `static` needs a data
section with an initialisation order, a rule for the collector (is a `static
Gc[T]` a root? conservative stack scanning does not see it) and one for
threads. That is a round of its own, not a side effect of one. What a kernel
does instead is in `docs/ROUND59.md` section 2, what the JavaScript engine
does instead in `docs/ROUND63.md` gap 5.

**The one ULP of the number reader (R63-8)** belongs to round 65 and was not
touched. `tools/lex_compare.sh` still reports it as the one known deviation.

---

## 9. Two findings beside the assignment

**`tools/english/check.sh` did not report 0 0 0 0 0 at the base commit.**
It reported `0 0 0 828 0`. The 828 are path names under `.js-work/`, and
`.js-work/` is **tracked**: 32 962 files of the test262 suite of TC39 plus
the comparison runs against `node` were committed in round 63 and are not in
`.gitignore`. They are FOREIGN DATA, exactly like `testdata/`, which
`check_names.py` has excluded since round 55 -- only lying at a different
place in the tree. `.js-work/` is now excluded in the same line, with the
reason written down. The 828 hits are strings like `hole`, `fall` and
`primitiv` inside ENGLISH test names, where they are right.

Whether the 32 962 files belong in the repository at all is a question for
whoever merges `lib/js/`; this round did not delete anything.

**Two thread tests are timing sensitive, and that is what the test count
above hangs on.** `tests/860_thread_basic.fi` (round 49) and
`tests/834_arc_thread.fi` (round 47) each end in a counter-check that MUST
lose increments -- "if it does not, the threads did not really run at the
same time, and B proves nothing". That is a good check and it is bought with
a RACE. On a machine at load 9 to 12 the threads get serialised, the
counter-check does not strike, and the test ends with 14 respectively 9.

Measured, so that it is not a claim:

| load | `834 [opt]` | `860 [opt]` | `834`/`860` in the other two stages |
|---|---|---|---|
| ~12 (three rounds compiling beside it) | 7/10 | 10/10 | 10/10 each |
| quiet | 25/25 | 20/20 | -- |

Both files, `tools/thread/` and the whole thread library are **octet-identical
to the base commit** (`git diff aa65fdc --stat` is empty for them), and
neither uses anything this round touched -- no `~`, no line continuation, no
`asm`, no `f64`, no `Gc` upcast, no function value in a field. The three
acceptance runs of this round therefore ended at 994, 995 and 992 of 996,
and every failure in every one of them was one of those two tests (once
inside the corpus pass of `tools/fixpoint.sh`, which runs the same corpus).
Run on its own, `tools/fixpoint.sh` reports `CORPUS: .firnc2 behaves like
firnc0` and exit code 0.

Whoever wants a stable number here has to give the counter-check a floor
(for instance: repeat until the raw counter loses at least once, with an
upper bound) -- that is a change to a test of round 47/49 and belongs to
whoever owns it, not into a round about language gaps.

---

## 10. Where the changes are

Compiler (Rust, `firnc0`):

| file | what |
|---|---|
| `lexer.rs` | the token `~`, and the hint for it removed |
| `ast.rs` | `UnOp::BitNot` |
| `parser.rs` | `~` in `unary`, `continues_line` and `cont` |
| `sema.rs` | `~` (check, preview, constant), `-x` on `f64`, the field preview |
| `lower.rs` | `~` -> `Op::Un(Not)`, the field call in `lower_call` |
| `impls.rs` | `field_fn` -- the field of function type, after the method |
| `errors.rs` | `compatible` knows the free upcast |
| `lower_errors.rs` | the `scalar_fty` copy gone -- the `E!f64` bug |
| `core.rs` | `out("reg") p`, the checks around it |
| `fir.rs`, `inline.rs`, `codegen_x86.rs` | `Op::Asm` with `out_regs`/`outs` |
| `ast_canon.rs`, `comptime.rs` | `~` in the canonical form and at compile time |

Compiler (Firn, `firnc1`), the same changes in the same order:
`lexer.fi`, `parser.fi`, `sema.fi`, `lower.fi`, `print.fi`, `time.fi`,
`ast.fi`, `fir.fi`, `codegen.fi`.

Beside them: `bin/lexdump.fi` (the token name table, fixed length 454 ->
460), `tools/fmt/fmt.fi` (`~` as a prefix sign in the formatter),
`tools/english/check_names.py` (9), `demos/kernel/user.fi` (`~` and the two
outputs), `SPEC.md`, `test.sh` (section 27) and `tools/fnfield/run.sh`.

Tests: `tests/1230`, `1232`, `1236`, `1238`, `1242`, `1247`, `1249` and
`tests/neg/1064`, `1231`, `1233`, `1234`, `1235`, `1237`, `1239`, `1240`,
`1241`, `1243`, `1244`, `1245`, `1246`, `1248`; `tests/1004_js_f64_union.fi`
rewritten from "the gap is still there" to "the gap is closed".

`git status --porcelain --ignored | grep '\.s$'` is empty -- no
hand-written assembly was produced in this round, so the trap of round 52
and 59 could not strike. `firnfmt -w` ran over every new and every changed
source; `firnfmt -c` over the whole tree reports nothing (624 files
formatted, 0 changed by the shape).

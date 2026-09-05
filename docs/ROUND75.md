# Round 75 -- `extern fn`, in both compilers

Branch `r75-extern`, base `3f253bdc`.

This round has two halves. The first, `firnc0` (Rust, `compiler/src/extfn.rs`
and the parser/codegen hooks next to it), was already committed
(`18b797db`) before this half started -- SPEC 14.1 item 7 struck the "not
supported" line, `extern fn`/`#[link_name]`/`#[export_c]` worked in stage 0.
The second half is this one: the same feature in `lib/firnc1/*.fi`, the
self-hosted compiler, so that the fixpoint (`tools/fixpoint.sh`) still holds
-- a feature that only one of the two compilers understood would not be one.

## The numbers first

| measurement | before this half | after |
|---|---|---|
| `tools/fixpoint.sh` | character-identical | **character-identical** (3 621 184 octets, 617 667 lines of assembly) |
| `tools/self_compare.sh` | 305 same / 0 differing / 0 faulty | **311 same behaviour, 0 DIFFERING, 0 FAULTY** (19 skipped -- firnc0 itself does not compile them, unrelated) |
| `tools/extfn/run.sh` (new) | did not exist | **6/6 passed** -- both directions, both compilers |
| `tools/english/check.sh` | 0 0 0 0 0 | **0 0 0 0 0** |

## What `extern fn` needed in `lib/firnc1`

`bin/firnc1.fi` already had `-c`/`--object` (round 52) and the System V
classification (round 71, `types.fi::word_class`); what was missing was the
declaration shape itself and the escape hatch out of the `_F1.` name
mangling every ordinary function goes through.

**Parser** (`parser.fi`): `extern fn name(...) -> T;` -- no `{ }`, a `;`
instead, exactly as strict as `firnc0`'s `self.expect(TokKind::Semi, ...)`
(no automatic line continuation the way an ordinary statement gets one --
both compilers agree on this, it is not an oversight). `#[link_name(...)]`
and `#[export_c]` join `#[no_gc]`/`#[must_consume]` in the attribute
registry (`attr()`, the six/four-token shape checked in the prescan of
`extensions_search()` so the file still reports "not core" correctly when
the gc registry has not been handed over yet).

**AST** (`ast.fi`): `fn_add` grew three arguments -- `is_extern: u32`,
`link: u32` (the interned `#[link_name(...)]` argument, meaningless without
the next field), `link_set: u32` (whether `#[link_name(...)]` was written at
all). All five existing callers (three ordinary functions, `mono.fi`'s
monomorphizer, two more in `parser.fi`) had to be widened to keep the
argument count in step -- Firn's stage 0 checks arity strictly, unlike a
dynamically typed language, so a forgotten call site fails at BUILD time
with a clear "expects N arguments, found N-1", not silently at runtime.

**Codegen** (`codegen.fi`, `sym()`): the one place every OTHER function name
was already routed through the `_F1.` prefix now branches first --
`extern_link_of`/`export_c_of` walk the function table via the AST tree
registered with `cg_tree_set` and, on a match, hand back the bare, unmangled
name through `sym_raw()` instead. Two escape hatches, additive: an
`extern fn` call site wants the FOREIGN symbol; an `#[export_c]` definition
wants ITS OWN bare name so C can find it.

## Two real bugs, both found by testing against `firnc0`, not by inspection

**The zero sentinel.** Firn's interner hands out numbers starting at `0`,
not `1` (`intern_new`: `let nr: u32 = (*t).cnt as u32`). `sym()` used
`link != 0` to mean "no `#[link_name]` was written" -- correct for the FLAG,
wrong for the RETURN VALUE of `extern_link_of`, which could legitimately be
`0` when the extern function's own bare name happened to be the very first
word ever interned in the program (exactly what `tests/t1.fi`'s `strlen`
was). The fix drops the sentinel: `Cg` grew an `ext_out: u32` output field,
`extern_link_of`/`export_c_of` return a `bool` ("found, yes or no") and
leave the NAME in `ext_out` -- found/not-found and the value it found are no
longer the same integer wearing two hats. `firnc0`'s Rust side never had
this bug; `Option<String>` has no such collision to begin with.

**The pending-attribute check forgot `extern`.** `par_run`'s item loop
enforces "`#[link_name(...)]`/`#[export_c]` must sit directly in front of a
function" by comparing the NEXT token's kind against `A_KWFN` -- and against
nothing else. An `extern fn` declaration's next token is `A_KWEXTERN`, not
`A_KWFN`, so `#[link_name(strlen)] extern fn my_strlen(...) -> u64;` tripped
the very check meant to accept attributes on functions, with a silent
`(*p).err += 1` and no diagnostic text (`complain()` only counts -- stage 0
does not carry file/line/column the way `firnc0` does, by design, see
`astdump.fi`'s bare `return 1`). `firnc0`'s `attributes()` has no such
"must be followed by X" gate at all -- `pending_attrs` is simply taken by
whichever declaration parser runs next, so the bug had no counterpart to
inherit; it was introduced fresh while porting the four checks
(`nogc_pending`, `mc_pending`, `link_pending_set`, `export_pending`) and
caught only because `#[link_name(foo)] extern fn bar();` built cleanly under
`firnc0` and did not under `lib/firnc1` -- the same input, two different
answers, which is exactly what `tools/self_compare.sh` exists to catch.
Fixed by adding `a != A_KWEXTERN` next to `a != A_KWFN` in all four checks.

## The measurement: both directions, both compilers

`tools/extfn/run.sh` (new) links a Firn-produced object file two ways:

* **direction 1, Firn calls out.** `extern fn strlen(p: u64) -> u64;`
  against `tools/extfn/impl.s` -- a hand-written `strlen`, deliberately NOT
  linked against libc, so the proof does not lean on a C library that
  happens to already agree with Firn's calling convention by coincidence.
  `strlen("hello")` = 5. `#[link_name(strlen)] extern fn my_strlen(...)`
  against the same object, called under a DIFFERENT Firn-side name:
  `my_strlen("hi!")` = 3.
* **direction 2, Firn is called into.** `#[export_c] fn add_one(x: i64) ->
  i64 { return x + 1 }`, compiled with `-c`/`--object` (no `ld`), `_start`
  and the placeholder `main` localized with `objcopy` (the same trick as
  `tools/abi/run.sh`, round 71), linked against `tools/extfn/host.c` -- an
  ordinary C program that never saw the Firn source, declaring only
  `extern long add_one(long);`. `add_one(41)` = 42.

All six checks (`callout`/`linkname`/`callback`, times `firnc0` and
`lib/firnc1`) pass, exit codes compared exactly, both compilers producing
the identical answer for the identical input -- the same discipline
`tools/abi/run.sh` established for `f32`/`f64` in round 71, now covering the
one remaining case that round left open: a name the linker resolves, not a
register class.

## SPEC

14.1 item 7 updated: struck, with the round, the file names, the proof
(`tools/extfn/run.sh`) and what is deliberately still out of scope --
variadic externs (`printf`-style), struct arguments beyond what item 1
already covers, and dynamic loading. `extern fn` means a name the LINKER
resolves at link time; nothing more was promised and nothing more was
built.

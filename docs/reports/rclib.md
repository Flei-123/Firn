# Report on module `rclib` — `Rc[T]` / `Weak[T]` (SPEC §3.4, requirement S7)

Round 4 (hardening test 2), assignment according to `PLAN.md` §5. Pure Firn,
**no compiler change**, no file outside our own holdings touched.

## 1. What was built

| File | Content |
|---|---|
| `tests/modules/rc.fi` | **the one implementation** (module `rc`): heap via `mmap`, size classes with free lists, `Zaehlverweis[T]` (= `Rc[T]`), `Schwachverweis[T]` (= `Weak[T]`), fallible allocation `AllocError!bool` |
| `lib/rc/rc.fi` | symlink to `tests/modules/rc.fi` (a library path without duplicating code) |
| `lib/rc/parts/*.fi` | bodies of the test programs |
| `lib/rc/gen_tests.sh` | generates the test programs from body + implementation |
| `tests/550_rc_basic.fi` | create/read/clone/release, counter values, block reuse, second instantiation of the template |
| `tests/551_rc_weak.fi` | `weak_von`/`aufwerten`/`weak_freigeben`, an upgrade after death returns **visibly empty**, the block only becomes free with the last weak reference |
| `tests/552_rc_cycle_leak.fi` | **mandatory proof: the Rc cycle LEAKS** (output `1 1 2 128 200 12800 198 1 0`) |
| `tests/553_rc_fallible.fi` | fallible allocation: `OutOfMemory` with a full heap, with a payload that is too large, a `try` chain, resumption after a release |
| `tests/554_rc_dauerlauf.fi` | 20.000 rounds without a cycle: `20000 0 0 192 1` — no growth, everything free |
| `tests/neg/rc_discarded.fi` | a discarded `AllocError!bool` = a compiler error with a line/column |
| `tests/neg/rc_unveraenderlich.fi` | an attempted write through `Rc` = a compiler error with a line/column |
| `docs/RC.md` | manual, example, leak explanation, complete list of deviations |

## 2. Measured by ourselves

All programs in **three** build stages (`opt`, `--no-opt`,
`--opt-level=dev-fast`), the same result each time:

```
tests/550_rc_basic.fi     [opt|noopt|dev-fast] exit=0
tests/551_rc_weak.fi      [opt|noopt|dev-fast] exit=0
tests/552_rc_cycle_leak.fi [opt|noopt|dev-fast] exit=0  out=1 1 2 128 200 12800 198 1 0
tests/553_rc_fallible.fi   [opt|noopt|dev-fast] exit=0
tests/554_rc_dauerlauf.fi [opt|noopt|dev-fast] exit=0  out=20000 0 0 192 1
```

Negative tests (real compiler output):

```
tests/neg/rc_discarded.fi:393:5
error: the result must not be discarded: the type 'AllocError!bool'
       is marked with #[must_consume]

tests/neg/rc_immutable.fi:396:5
error: left side is not an assignable expression (variable, field, index or '*pointer')
```

### State of the whole suite at the time of this report

`bash test.sh` could **not** be run to the end: the compiler in the tree
did not compile while I was working, because another module of this round
is in the middle of its change —

```
error[E0425]: cannot find function `emit_gc_addr` in module `crate::codegen_x86`
    --> src/regalloc.rs:1110:33
error[E0425]: cannot find function `emit_gc_addr` in this scope
    --> src/codegen_x86.rs:361:13
```

That affects no file of `rclib` (I have not touched a compiler source).
To be able to check dependably nonetheless, I built the compiler from the
**unchanged starting state**
(`git archive HEAD compiler | tar -x -C .rcwork/rein`, `cargo build --release`
— zero warnings) and ran all positive programs of the tree with it in three
build stages:

```
tests/*.fi + tests/opt/*.fi + examples/*.fi (the starting state + my 5 new ones)
  148 programs x 3 build stages -> PASS=444  FAIL=0
tests/neg/*.fi  55/58 as expected
  (the three exceptions are tests/neg/nogc_*.fi from the module 'nogc'
   running in parallel; they need its compiler state, not mine)
```

As soon as the tree compiles again, there is nothing to catch up on for
`rclib`: my files are pure Firn and hang on none of the open building sites.

The leak proof in numbers (`tests/552`): after discarding both outer
handles, both strong counters still stand at **1**, **2** live
blocks / **128** bytes stay held; after 100 cycles created and discarded
there are **200** blocks / **12.800** bytes. With `Weak` instead of the
second strong reference: the upgrade is visibly empty, and at the end there
are **0** occupied blocks.

## 3. Deviations (they belong in SPEC §14.1 — module `mess` enters them)

1. **The type names `Zaehlverweis[T]` / `Schwachverweis[T]` instead of
   `Rc[T]` / `Weak[T]`.**
   `Rc`, `Arc` and `Weak` are reserved in the parser as type constructors
   that are not yet implemented
   (`compiler/src/parser.rs`,
   error "'Rc[T]' is not implemented in stage 0"); a pure Firn module
   cannot occupy the names, and the compiler sources belong to other
   modules in this round. The **function names** from the contract are
   unchanged: `rc_neu`, `rc_lesen`, `rc_klonen`, `rc_freigeben`,
   `rc_stark_zahl`, `weak_von`, `aufwerten`, `weak_freigeben`. As soon as
   somebody lifts the reservation, it is a pure renaming in one file.
2. **`rc_neu(…)` instead of `Rc[T].neu(…)`** — stage 0 knows no methods.
3. **`h: *mut RcHeap` instead of `inout alloc`** — stage 0 knows no
   `inout`.
4. **`AllocError!bool` + an output pointer instead of `AllocError!Rc[T]`.**
   Monomorphization does not substitute type arguments in the payload of an
   error union: `fn f[T: Any](..) -> AllocError!Zaehlverweis[T]` reports
   „unbekannter typ 'Zaehlverweis__T'", and likewise `AllocError!T`
   („unbekannter typ 'T'"). The allocation stays fully fallible and
   `#[must_consume]`.
   *Notice to `gckern`: if monomorphization is being touched in this round
   anyway, that would be a small, worthwhile addition.*
5. **`Arc[T]` is not built** — open. Stage 0 has no threads and no
   atomic instructions; an „Arc" that does not count atomically would be a
   lie.
6. **No destructors** (`drop`, SPEC §3.3, is not built): references that
   lie *inside* a value have to be released by hand. That affects only
   values that themselves contain references and is visible in `tests/552`.
7. **The test programs contain the implementation verbatim.** Stage 0 does
   not resolve generic templates across module boundaries — neither
   `rc.Zaehlverweis[T]` in type position nor `rc.rc_neu[T](..)` in a call
   compiles (`hook_generic_call` demands an `Ident`, and the module system
   delivers a `Field` expression there). Hence the same procedure as with
   `lib/str` (`tools/strlib/expand.py`): one source in the tree,
   `bash lib/rc/gen_tests.sh` assembles the programs. `import
   modules.rc` therefore does **not** work — that is the honest situation.

## 4. For module `mess`: the duplication of the error set

`error AllocError { OutOfMemory }` stands **in `tests/modules/rc.fi`** (and
therefore in the generated `tests/55*_rc_*.fi` and
`tests/neg/rc_*.fi`). Error set names are program-wide. As soon as the GC
runtime (`gckern`) provides the same set program-wide, there are two
possibilities:

* The Rc programs use **no** `gc class` value and therefore do not pull in
  the GC runtime at all — then nothing collides and both
  versions can stay.
* Should the runtime be pulled into every program after all, it suffices to
  delete the one `error` line in `tests/modules/rc.fi` and to run
  `bash lib/rc/gen_tests.sh` again. No further
  intervention needed.

The check was made against the state of the compiler at the beginning of
the round; no reference to `Gc` or `gc class` occurs in any Rc file.

## 5. What was NOT built

* `Arc[T]` (an atomic counter) — open, see deviation 5.
* Destructors / `drop` — not part of this round.
* No growing of the heap; `rc_heap_init` determines once how much there is.

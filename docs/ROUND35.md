# Round 35 — comptime in firnc1

## Goal
Execute `comptime { ... }` blocks at compile time: a real interpreter in Firn
(model `compiler/src/comptime.rs`); the generated source text is parsed as a
further module into the same tree before monomorphization runs.

## Built
- `lib/firnc1/time.fi` (689 l.): registration of the comptime blocks +
  interpreter (expressions, statements, loops, calls to comptime-fn), output
  into rt.Buf.
- `ast.fi`: `ct_block` collection list in the root file.
- `parser.fi`: reads `comptime { }`, the pre-scan is relaxed (blocks without a
  name binding), flag `par_zeit_setzen`/`par_zeit_an` for control.
- `bin/firnc1.fi`: driver hookup — between the root parser and `mono.gen_lauf`:
  run `zeit_lauf`, hang the generated text into the same tree as a module
  without an alias via the same lex/parse machinery (same interner).

## Honest limits (named, not concealed)
- comptime in imported modules is still reported by `sema_braucht_comptime`
  (only the root file is executed).
- Constants that would have to be evaluated at compile time but cannot
  remain a separate known case.

## Measurements (worktree checkout, branch r35-comptime)
- test.sh: 634/634 (previously 631)
- tools/self_compare.sh: 169 behaviorally identical (previously 166),
  0 differing, 0 failing
- Fixpoint: stage 2 == stage 3, character-identical, 210324 lines of assembly
- New: tests/760_comptime_core.fi (comptime core language)
- Target files 601/602 (among others UCD table generation) run identically
  to firnc0

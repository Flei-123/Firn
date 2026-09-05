# Round 36 — ct intrinsics, errdefer, must_consume (the last three core blocks)

State after round 36: selbst_vergleich **185 identical / 0 differing /
0 failing**, test.sh **640/640**, fixpoint **character-identical (284207
lines of assembly)**.

## 1. Constant runtime / ct intrinsics (model compiler/src/ct.rs)

Three intrinsics ported to firnc1 — parser core registration (parser.fi,
`erweiterungen_suchen`: core only, as soon as the gc registration is
announced, as with `barrier` in round 34), sema (sema.fi, its own detection
before the function lookup — a user function of the same name wins), lowering
(lower.fi, its own Fir terms) and codegen (codegen.fi).

- `select(bedingung, a, b)` — data-independent selection. Sema: exactly 3
  arguments, condition `bool`, scalar types only (int/bool/pointer), both
  branches of exactly the same type, no implicit conversion. Codegen: cmov
  pattern, no branches.
- `secure_zero(zeiger, anzahl)` — zeroes the buffer, must never be optimized
  away (SPEC §9.3, C3). Sema: pointer + integer. Codegen: loop with
  volatile store semantics.
- `select` with pointer types (431) and `secure_zero` (433) cover the
  additional cases.

Tests: tests/430_ct_select.fi, 431_ct_select_ptr.fi, 432_ct_barrier.fi
(completion), 433_ct_secure_zero.fi, core test tests/780_ct_core.fi.
Negative tests (rc=1 on both sides): tests/neg/ct_select_cond.fi,
ct_select_digit_count.fi, ct_select_types_different.fi,
ct_secure_zero_no_ptr.fi, ct_barrier_aggregate.fi.

## 2. errdefer (model stage 0, defer.fi as the firnc1 model)

`errdefer` only runs if the block is left with an error.
Implementation: `defer_bis_fehler` in parser/sema, `ret_term_fehler` in
lowering — the defer chain is worked off at the failing return point in
reverse order, and not on the success path. Passing on a finished union
is correctly rejected (tests/neg/errdefer_union_propagation.fi, rc=1).
Commit 3144601.

## 3. #[must_consume] (model compiler/src/attrs.rs)

Attribute on functions and structs: if call results (fn) or values of the
type (struct) are discarded, the compiler aborts
(`check_discard` in sema.fi). Negative tests attr_must_consume_* rc=1.
Commit 6ef2616.

## Measurements

| Tool | before (round 34) | after |
|---|---|---|
| tools/self_compare.sh | 180 identical | **185 identical, 0 diff., 0 failing, NOT CORE 0** |
| test.sh | 637/637 | **640/640** |
| tools/fixpoint.sh | 279201 lines | **284207 lines, character-identical** |

## Limits (honestly named)

- selbst_vergleich COMPTIME: 1 — 600_comptime.fi (rc=4) remains the only
  named residual case; the comptime machinery from round 35 does not yet
  cover the full stage 0 vocabulary there.
- fir_vergleich: 1 differing (known and named from earlier rounds).
- SKIPPED: 15 files that firnc0 does not compile individually
  (unchanged, no regression).

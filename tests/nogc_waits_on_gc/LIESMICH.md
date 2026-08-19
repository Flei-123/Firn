# `#[no_gc]` rules (i) and (iii) -- waiting for the GC core

For

* **(i)** GC allocation  -> `crate::gc::ist_gc_alloc_aufruf(name)`
* **(iii)** writing into a `Gc[T]`/`GcWeak[T]` field -> `crate::gc::ist_gc_zeiger(typ)`

the check in `compiler/src/nogc.rs` asks exclusively the two contract
functions from `compiler/src/gc.rs` (module `gckern`). As long as those still
yield `false` there (the skeleton version), **no Firn program** can trigger
rules (i) and (iii) -- there is neither `gc class` nor `Gc[T]` after all.

The two programs here are therefore **not** in `tests/neg/`: `test.sh`
demands a real compile error there, and today such an error would only come
out of the parser ("the Gc heap is not implemented"), not out of the
`#[no_gc]` check. As soon as `gc.rs` answers the two queries, the files
belong into `tests/neg/` unchanged -- the expected messages are in line 1.

Until then rules (i) and (iii) are proven **inside the compiler itself**:
`cargo test --release` runs the tests
`regel1_gc_allokation_ist_verboten`, `regel3_schreiben_in_gc_feld_ist_verboten`
and `regel3_zuweisung_an_oertliche_veraenderliche_ist_erlaubt` in `nogc.rs`.
They put predictions in place of the two queries and check the message, the
line and the column. The compiler itself always uses `Regeln::echt()`, that is
`gc.rs` (test `echte_regeln_sind_die_aus_gc_rs`).

**To check when moving to `tests/neg/`:** the line and the column in line 1
match the place that `gckern` gives as the span for `gc ...{...}` resp. for
the field name; adjust them there if needed. The texts of the messages are
fixed in `nogc.rs` and do not change.

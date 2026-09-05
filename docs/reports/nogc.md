# Report on module `nogc` — `#[no_gc]` made sharp (SPEC §3.5.4)

State: round „hardening test 2". All outputs below are **real** outputs of
the built compiler (`compiler/target/release/firnc`), not retold.

## 1. What applies now

`#[no_gc]` is set to `umgesetzt: true` in the attribute registry
(`compiler/src/attrs.rs`) and is checked by `compiler/src/nogc.rs`
(hooked in via `// HOOK nogc` in `sema::Checker::run`; `sema.rs` was
not touched). The check runs after the type check, because rule (iii) needs
the type table.

```
$ ./compiler/target/release/firnc --list-attrs | head -5
Attribute

NAME            TARGET       ARGS  STAGE 0     PURPOSE
must_consume    fn, struct   0     implemented result must not be discarded (SPEC 3.3, 5.1)
no_gc           fn           0     implemented no collection run in this call tree (SPEC 3.5.4)
```

The remaining attributes stay rejected as before -- `constant_time`,
`unwinds`, `packed`, `align`, `layout`, `no_move`, `abi_stable`, `frozen`,
`hot` still report "attribute '...' is not implemented in stage 0"
(`tests/neg/attr_not_implemented.fi` unchanged green, plus the
module test `attrs::tests::not_implemented_attribute_report_next_a_error`).
The test `only_must_consume_is_implemented` was pulled along and now demands
exactly `["must_consume", "no_gc"]` -- the bracket that prevents an
attribute from silently becoming "implemented".

### The three rules

In a `#[no_gc]` function the following are forbidden:

| Rule | What | Where the information comes from |
|---|---|---|
| (i) | GC allocation / a call that can trigger a collection | `crate::gc::ist_gc_alloc_aufruf(name)` |
| (ii) | a call to a function **without** `#[no_gc]` | the attribute table of the whole program |
| (iii) | writing a `Gc[T]`/`GcWeak[T]` pointer into a field | `crate::gc::ist_gc_zeiger(typ)` |

The two GC queries are the **contract of module `gckern`** and were
not changed; `nogc.rs` asks exclusively these two functions and
otherwise does not know the GC.

The check is transitive: because every called function has to carry
`#[no_gc]` itself, the promise holds for the whole call tree, over
arbitrarily many levels and across module boundaries.

### Hardened against the starting version

* **`match` cases are checked as well.** The body blocks of the cases lie
  not in the AST but in the registry of `sema_match.rs`
  (`__match#N`). Without descending there, every state machine would be a
  blind spot — that is, exactly the code `#[no_gc]` is meant for.
  Proof: `tests/neg/nogc_match_case.fi`.
* **Module-qualified calls.** After the rewriting by `modules.rs`,
  `modul.funktion` is internally called `modul__funktion`; the message
  shows the spelling from the source text again (`nogc_kalt.aufwaendig`).
  Proof: `tests/neg/nogc_module_boundary.fi`.
* **No false alarms.** Call names generated internally by the compiler
  (`__match#N`, `__try#`, `__catch#`, `Enum::Variante`) are not function
  calls and trigger nothing; their arguments are searched nonetheless.
  A call to a name that does not exist at all is reported by the type check
  itself — here there is no second, confusing error. Proof:
  `nogc::tests::interne_namen_loesen_keinen_fehler_aus`.
* **Rule (iii) more precisely.** What is reported is writing into a field,
  into an element behind a field and behind a pointer. Assignment to a
  local variable on the stack needs no insertion barrier and
  stays permitted.
* Messages with a line **and** a column, with the source line, a `^^^`
  marker and a hint about what to do; findings sorted deterministically by
  file/line/column; duplicate messages suppressed.
* A recursion limit `MAX_TIEFE = 256` for nested `match` body blocks
  (a second safeguard next to the parser limit of 200).

## 2. Negative tests — real compiler outputs

```
$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_call_without_attr.fi
error: 'hot' is #[no_gc], but calls 'slow' without #[no_gc]
   --> tests/neg/nogc_call_without_attr.fi:11:12
    |
 11 |     return slow(a)
    |            ^^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'slow' or do not call it here

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_transitiv.fi
error: 'mid' is #[no_gc], but calls 'below' without #[no_gc]
   --> tests/neg/nogc_transitiv.fi:21:17
    |
 21 |         s = s + below(a)
    |                 ^^^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'below' or do not call it here

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_match_case.fi
error: 'step' is #[no_gc], but calls 'log' without #[no_gc]
   --> tests/neg/nogc_match_case.fi:17:32
    |
 17 |         State::End => { return log(c) }
    |                                ^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'log' or do not call it here

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_module_boundary.fi
error: 'hot' is #[no_gc], but calls 'nogc_cold.costly' without #[no_gc]
   --> tests/neg/nogc_module_boundary.fi:10:12
    |
 10 |     return nogc_cold.costly(a)
    |            ^^^^^^^^^^^^^^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'nogc_cold.costly' or do not call it here
```

In `tests/neg/nogc_transitiv.fi` the break deliberately lies **one level
deeper** than the marked entry (`above` -> `mid` -> `below`): what is
reported is the place at which the chain tears.

## 3. Positive tests

| File | what it shows | Result |
|---|---|---|
| `tests/540_no_gc_aufruftree.fi` | a marked call tree over four levels, loops, branches; an unmarked function may call a marked one | `expect_exit: 42` |
| `tests/541_no_gc_state_machine.fi` | `#[no_gc]` + `match` with four cases, calls out of the case bodies | `expect_exit: 99` |
| `tests/542_no_gc_module.fi` (+ `tests/modules/nogc_hot.fi`) | `#[no_gc]` across the module boundary | `expect_exit: 100` |

All three run in `test.sh` in **three** build stages (`opt`, `--no-opt`,
`--opt-level=dev-fast`) with the same result.

## 4. The HTML5 tokenizer is now `#[no_gc]`

That is the promise from SPEC §3.5.4 to tokenizers, rasterizers and crypto
— and here it is redeemed on real code. **Every** function in `lib/html/`
carries `#[no_gc]`, including the `main` of the driver:

| File | marked functions |
|---|---|
| `lib/html/mem.fi` | 31 |
| `lib/html/tokens.fi` | 58 |
| `lib/html/tokenizer.fi` | 17 |
| `lib/html/entities.fi` | 19 |
| `lib/html/entities_data.fi` | 14 |
| `lib/html/error_codes.fi` | 1 |
| `lib/html/tokenize_main.fi` | 3 (with `main`) |
| `lib/html/entities_probe.fi` | 4 |
| `lib/html/entities_failure.fi` | 12 |
| **Sum** | **159** |

With that it is statically established: in the whole tokenizer program no
collection can take place, there is no barrier and no pause.

The rate has **not** become worse (`bash tools/tokenizer/run.sh`):

```
== 3. the same balance in all three build stages ==
   noopt: 6810 without / 6809 with error codes -- equal
   devfast: 6810 without / 6809 with error codes -- equal

== 5. regression limit ==
   without error codes: 6810 / 6810   (limit: 6810)
   with error codes:    6809 / 6810   (limit: 6809)
OK: 6810 / 6810 without, 6809 / 6810 with error codes passed
```

### Counter-check: does the marking really take effect?

An attribute that does nothing would be worthless. Probe: `#[no_gc]`
**removed** in front of `mem.buf_at`, then compile — the compiler aborts
immediately (restored afterwards):

```
$ ./compiler/target/release/firnc -o .test-work/tk lib/html/tokenize_main.fi
error: 'read_u32' is #[no_gc], but calls 'mem.buf_at' without #[no_gc]
   --> lib/html/tokenize_main.fi:32:19
    |
 32 |         v = v | ((mem.buf_at(b, off + i) as u32) << (8 * i as u32))
    |                   ^^^^^^^^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'mem.buf_at' or do not call it here
error: 'decode' is #[no_gc], but calls 'mem.buf_at' without #[no_gc]
   --> lib/html/tokenize_main.fi:45:23
    |
 45 |         let c0: u32 = mem.buf_at(b, off + i) as u32
    |                       ^^^^^^^^^^ here
    = note: SPEC 3.5.4: the promise holds transitively for the whole call tree -- write #[no_gc] before 'mem.buf_at' or do not call it here
```

## 5. Module tests of the compiler

`cargo test --release --manifest-path compiler/Cargo.toml` (section 2 of
`test.sh`), excerpt:

```
test nogc::tests::echte_regeln_sind_die_aus_gc_rs ... ok
test nogc::tests::hat_no_gc_erkennt_das_attribut ... ok
test nogc::tests::interne_namen_loesen_keinen_fehler_aus ... ok
test nogc::tests::modulname_wird_lesbar_gemeldet ... ok
test nogc::tests::regel1_gc_allokation_ist_verboten ... ok
test nogc::tests::regel2_aufruf_ohne_no_gc_ist_verboten ... ok
test nogc::tests::regel2_markierter_aufruf_ist_erlaubt ... ok
test nogc::tests::regel3_schreiben_in_gc_feld_ist_verboten ... ok
test nogc::tests::regel3_zuweisung_an_oertliche_veraenderliche_ist_erlaubt ... ok
test nogc::tests::tiefe_verschachtelung_wird_erreicht ... ok
test nogc::tests::unmarkierte_funktion_wird_nicht_geprueft ... ok
test attrs::tests::nicht_umgesetzte_attribute_melden_weiter_einen_fehler ... ok
test attrs::tests::nur_must_consume_ist_umgesetzt ... ok
test result: ok. 134 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 6. Honestly open — what does NOT work

* **Rules (i) and (iii) cannot be triggered in any Firn program today**,
  because `compiler/src/gc.rs` (module `gckern`) returns `false` for both
  queries in its skeleton version, and therefore there is neither
  `gc class` nor `Gc[T]`. The check is wired up and **demonstrated in the
  compiler** (the three module tests above substitute predictions for the
  two queries and check the message, the line and the column); the compiler
  itself always uses `Regeln::echt()`, i.e. `gc.rs` (test
  `echte_regeln_sind_die_aus_gc_rs`).
  As soon as `gc.rs` answers, (i) and (iii) take effect without a further
  change. The two finished negative programs for that lie in
  `tests/nogc_waits_on_gc/` including a `README.md`; they then belong,
  unchanged, in `tests/neg/`. They are deliberately **not** there yet:
  `test.sh` would otherwise check them against a message of the parser and
  would thereby establish something other than what it says on the label.
* The checker sees the **whole, flat program** after module merging
  and monomorphization. Separate compilation units with
  interface files do not exist (a limit of the module system,
  `modules.rs`), and therefore neither does a `#[no_gc]` check across
  library boundaries without source text.
* Calls through function pointers do not exist in stage 0
  (`ExprKind::Call` always carries a name). As soon as they do, rule (ii)
  needs an extension — today there is no loophole there, but no provision
  either.
* `#[no_gc]` produces **no** code and changes nothing in the lowering; it
  is a purely static promise. The collector itself, `gc.stats()`, the
  insertion barrier and incremental collection belong to `gckern`.

## 7. Run by ourselves

```
cargo build --release --manifest-path compiler/Cargo.toml   # 0 Warnungen
cargo test  --release --manifest-path compiler/Cargo.toml   # 134 passed
bash tools/tokenizer/run.sh                                 # 6810/6810, 6809/6810
bash test.sh                                                # sections 1-9
```

Result of the complete run at the time of this completion notice:

```
== 9. HTML5 tokenizer against html5lib (tools/tokenizer/run.sh) ==
   TOTAL                       6810 /  6810 100.00 %    6809 /  6810  99.99 %

PASS 510/510
```

(The total grows with the tests of the other modules of this round; from
`nogc` come 3 positive programs x 3 build stages and 4 negative tests.)

A side finding, so that it does not get lost: in `tests/neg/` there was an
empty file with the literal name `*.fi` (0 bytes, evidently from a botched
redirection). It made `test.sh` fail and was deleted.

No `#[allow(...)]`, no blanket suppression, no `todo!()`, no
external crates, no fixed address.

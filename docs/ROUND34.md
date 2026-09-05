# Round 34 — `gc class`, `Gc[T]`, weak references, `#[no_gc]` in firnc1

## Goal
The largest remaining block of the core language: the GC machinery (model
`compiler/src/gc.rs`, `nogc.rs`, ca. 2 250 lines of Rust). The runtime
`lib/gc/gc.fi` already exists (written in Firn) and is pulled in
automatically as soon as `gc class` appears anywhere in the import graph.

## Built
- `lib/firnc1/gc.fi` (854 l.): the class registration — prefix layout for
  `extends`, strong/weak field offset tables, `Gc[C]`/`GcWeak[C]` as
  types, `gc C{...}` -> `AllocError!Gc[C]`, `x.as?[C]`, relatedness
  check for identity comparisons.
- `lib/firnc1/gctext.fi` (323 l.): the runtime `lib/gc/gc.fi` as
  embedded source text; the driver hangs it into the same tree as a module
  without an alias (root namespace: `gc_init()` is called `gc_init()`
  everywhere).
- `lib/firnc1/nogc.fi` (341 l.): the transitive `#[no_gc]` checker —
  rule (ii) from SPEC §3.5.4 across module boundaries.
- Hooks in parser (603/920/1195/2405), sema (field access through `Gc[T]`,
  `weak`/`stark`, `gc C{}`, comparisons), lower and codegen (write barriers
  on field access, allocation via the runtime).

## The find of this round (again only visible on the corpus)
`bin/firnc1.fi` looked up the words `gc`/`class`/`AllocError` via
`intern_finde` — the numbers only exist if the ROOT FILE contains the
words. If `gc class` appeared only in an imported module
(tests/560 -> modules/dom.fi), the scan ran with -1 and found nothing:
no runtime, no `AllocError` set, silent sema error.
Firnc0 has the same spot in `main.rs`, and there it says `intern_nummer`.
Now `intern_nummer` — interning is cheap and idempotent.

## Measurements
- `tools/self_compare.sh`: 169 -> **179** behaviorally identical, 0 differing,
  0 failing (all nine gc files including 510 cycle and 560 DOM cycles)
- Fixpoint: stage 2 == stage 3, character-identical, **279 201 lines** of
  assembly
- Negative tests abort like firnc0: nogc_transitiv, nogc_aufruf_ohne_attribut,
  nogc_module_boundary, gc_klasse_auf_dem_stapel, gc_multiple_inheritance,
  gc_as_nicht_verwandt (rc=1 on both sides in each case)
- New: `tests/770_gc_core.fi` + `tests/modules/kern/gccore.fi` — gc class
  only in the module, cycle survives under the root, statistics checked

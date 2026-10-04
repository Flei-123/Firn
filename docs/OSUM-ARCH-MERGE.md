# osum-arch -> main: merge check and recommendation (04.10.2026)

Branch `osum-arch` (OrientOS line, 5 commits, 12 files) merged into `main` in the worktree `arch-merge`.

## What it brings

* `#[arch(...)]` restored in both compilers, `svg.image` -> `svg.svgimage` (module name clash with `fui.image`),
* x86: **drop functions nothing can reach** (roots = every symbol named outside a function body + exports + panic handler;
  `FIRN_KEEP_ALL=1` switches it off), optional one section per function (`FIRN_FUNCTION_SECTIONS=1`),
* firnc1: `#[inline]` / `#[no_inline]` accepted as hints, `__include_str` ported, `&TABLE[i]` of a plain static is a read.

## Result of the first full run (main + osum-arch, unchanged)

No merge conflict. Three findings, all caused by the new features meeting existing checks -- none by OrientOS code:

1. **`tools/abi/run.sh` failed to link** (section 32): the pruning dropped the C-visible functions of a `-c` object
   (`abi_add_f32` ...). An object is linked into somebody else's program, so no function in it is unreachable.
   Fix: `codegen_x86::set_object_only` (set from `main.rs` for `-c` / `--object`), the pruning leaves everything in.
2. **`self_compare` / `fixpoint`: `tests/2083_png.fi` FAULTY (firnc1 rc 7: `symbol _F1.rt__ld8 is already defined`)**:
   firnc1 accepting `#[inline]` made files take part that were "not core" before. `lib/std/rt.fi` is a symlink to
   `../rt/rt.fi`; a program reaching it as `std.rt` and as `rt.rt` loaded the SAME file twice (firnc0 canonicalises the path).
   A latent firnc1 bug (reproduced on main with a symlinked module pair), not the branch's. Fix: `ml_twin` in `bin/firnc1.fi` --
   a module known under the same name with the very same octets is the same module; `tests/2095_module_twin.fi`.
3. **Two tests assumed the old behaviour**: `test_opt.sh` read the assembly of `sum`, which is inlined into `main` at release-fast
   and is now dropped as unreachable (now compiled with `FIRN_KEEP_ALL=1`); `parser_compare.sh` compares firnc1's `__include_str`
   tree, which differs from firnc0's only in the numbers of the `_fsegNN` temporaries (4 files) -- those names are numbered alike now.

Also on the way: `test.sh` ended at the first red section (an empty `grep` in the FAIL branch under `set -e`, r94) -- the display
pipelines end in `|| true` now, so a red run lists everything.

## Result after the fixes

Full run (2057 checks): **the only reds are the ones main already has** --
`tools/english/check.sh` (German words in `lib/appkit`, `lib/std/extract|safefs|tar`, `tests/2120_async_loop`: other workers),
`tools/fmt/run.sh` ("not formatted": `lib/appkit`, `lib/async`, `lib/fleitec_id`: other workers; `parser.fi` was formatted here),
`tools/k3net/run.sh` (netem, load flake) and `tools/tls/run.sh` (a real host answers with other headers); the last two pass alone.
`self_compare` 423 same / 0 faulty, fixpoint stage 2 == stage 3 (character identical), ABI against GCC 32/32.

## Recommendation: **merge**, with these notes for the OrientOS side

* The pruning is on by default for hosted x86 programs. A program that needs a function the compiler cannot see being used
  (looked up by name from outside, assembly in another file) has to keep it with `export` / `#[export_c]` or run with `FIRN_KEEP_ALL=1`.
* `-c` objects are never pruned (the ABI test depends on it).
* Branch `osum-arch` itself was not changed; the fixes are separate commits on top (`arch-merge`).
* `dns-pic` is NOT part of this and stays unmerged (old line).

# superopt

Offline superoptimizer for the x86-64 text the register allocator emits.

1. `corpus.sh DIR` builds the benchmark programs (bench/firn, bench/firn_vs_c/firn; release-fast and
   release-safe), bin/firnc1.fi and OpenPlan with `--keep-asm`.
2. `mine.py OUT.json FILES...` collects straight-line register windows of 2-6 instructions, renames
   registers by first use (`%0`, `%1d`, ...) and records which registers and whether the flags are dead
   after each occurrence (same liveness scan as `compiler/src/superopt/liveness.rs`).
3. `search.py WINDOWS.json FOUND.json [LIMIT MIN_COUNT BUDGET_S]` enumerates shorter sequences
   (length 1, and length 2 for windows of 3+), filters with 64 random/edge inputs on full 64-bit
   registers (numpy) and proves each survivor with z3 over 64-bit bitvectors for all inputs.
   Flags are never compared: a rule whose original or replacement writes flags requires dead flags,
   so overflow checks (`jo`) of release-safe block it.
4. `emit.py FOUND.json rules.rs rule_tests.rs rules.json` writes the rule table
   `compiler/src/superopt/rules.rs` and one test per rule.

z3 and numpy: `python3 -m venv V && V/bin/pip install z3-solver numpy`, then `PY=V/bin/python`.
Whole pipeline: `WORK=<dir> PY=<python> /root/jarvis/bin/heavy bash tools/superopt/run.sh`.
Kill switch in the compiler: `FIRN_NO_SUPEROPT=1`.

# Fixpoint and corpus check

- `tools/fixpoint.sh`: stages 1-3 (firnc0 -> .firnc1 -> .firnc2 -> .firnc3), stage 2 == stage 3 byte-identical, then `self_compare.sh` with `.firnc2` over the corpus (sequential, over 90 min).
- `FIXPOINT_STAGES_ONLY=1`: stop after the stage comparison.
- `FIXPOINT_EXPORT=<path>`: copy the stage-2 compiler there.
- `SELF_COMPARE_SHARD=k/N`: `tools/self_compare.sh` runs only corpus files with sorted index mod N == k. `FIRNC1=<compiler>` selects the compiler under test.
- `tools/fixpoint_shards.sh [N=6] [JOBS=2]`: stages once, then N shards via `bin/heavy`, at most JOBS at a time, prints per-shard output and summed counters (SAME BEHAVIOUR / DIFFERING / FAULTY / NOT CORE / DEFER / COMPTIME / CODEGEN MISSING / SKIPPED). Exit 1 when DIFFERING or FAULTY is non-zero.
- Do not switch or delete the worktree while shards run; the test dirs are `mktemp -d` and removed on exit.

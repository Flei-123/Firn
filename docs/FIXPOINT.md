# Fixpoint and corpus check

- `tools/fixpoint.sh`: stages 1-3 (firnc0 -> .firnc1 -> .firnc2 -> .firnc3), stage 2 == stage 3 byte-identical, then `self_compare.sh` with `.firnc2` over the corpus (sequential, over 90 min).
- `FIXPOINT_STAGES_ONLY=1`: stop after the stage comparison.
- `FIXPOINT_EXPORT=<path>`: copy the stage-2 compiler there.
- `SELF_COMPARE_SHARD=k/N`: `tools/self_compare.sh` runs only corpus files with sorted index mod N == k. `FIRNC1=<compiler>` selects the compiler under test.
- `tools/fixpoint_shards.sh [N=6] [JOBS=2]`: stages once, then N shards via `bin/heavy`, at most JOBS at a time, prints per-shard output and summed counters (SAME BEHAVIOUR / DIFFERING / FAULTY / NOT CORE / DEFER / COMPTIME / CODEGEN MISSING / SKIPPED). Exit 1 when DIFFERING or FAULTY is non-zero.
- Do not switch or delete the worktree while shards run; the test dirs are `mktemp -d` and removed on exit.

## Corpus status (main 155412178, 8 shards)

- Stages: stage 2 == stage 3, character-identical (1226501 lines of assembly).
- Shards 0-7 of 8, `.firnc2` against firnc0: SAME BEHAVIOUR 1189, DIFFERING 0, FAULTY 0, NOT CORE 215, COMPTIME 1, DEFER 0, CODEGEN MISSING 0, SKIPPED 114 (1519 files).
- A shard with a high SKIPPED count and few SAME BEHAVIOUR (for example 4 SAME, 186 SKIPPED) is invalid: the worktree was removed while it ran. Rerun that shard.

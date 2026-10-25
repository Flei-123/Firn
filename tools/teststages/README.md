# test.sh stages

`bash test.sh --stage N` runs one stage; `tools/teststages/run.sh [--heavy|--parallel|--list]` runs all.
`stages.txt` maps section ids to stages (12 stages). An id may carry a shard: `3@2/8` runs shard 2 of 8 of section 3
(programs are dealt round robin via TEST_SHARD_INDEX / TEST_SHARD_COUNT). One stage holds at most one shard of an id.
Ids missing from stages.txt run in the last stage.

Measured 25.10.2026 through heavy at load 15-45 (wall seconds, inflated by load), previous 10-stage layout
with section 3 in 4 shards: stage 1: 1961, 2: 2695 (shard 2/4 about 2360), 4: 2691, 5: 1847 (section 34: 1436),
6: 3801 (section 16 self_compare: 2619), 8: >3500 (section 43 aarch64: 2126, section 24: 732).
Section 3 therefore runs in 8 shards, sections 16/17, 34 and 43 are alone or paired with one shard.
Longest remaining pieces: section 16 (stage 10) and section 43 (stage 11).

Known red on main (not regressions): sema_compare, fir_compare (tests/diff/layout_00.fi), ffi win_wgl_probe under Wine,
aarch64 run.sh, tools/checked/run.sh (sub_i8 message), fixpoint, js run, english, fmt.

Measured 25.10.2026 (12-stage layout, load 25-38, heavy queue waits included): stage 9: 1313 s (red: parser_compare, fixed; fmt/english are stage 12).
Stages 6, 8, 10, 11, 12 ran 55+ min wall at this load (stage 8 section 3 shard 8: about 1180 s before section 34; stage 11 aarch64 974 of 999 identical).
Stage 12 red: english (r59) and fmt (r48), both old.

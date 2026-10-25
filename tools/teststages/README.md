# test.sh stages

`bash test.sh --stage N` runs one stage; `tools/teststages/run.sh [--heavy|--parallel|--list]` runs all.
`stages.txt` maps section ids to stages. An id may carry a shard: `3@2/4` runs shard 2 of 4 of section 3
(programs are dealt round robin via TEST_SHARD_INDEX / TEST_SHARD_COUNT).

Measured 25.10.2026, all 11 old stages in parallel through heavy at load 25-45 (wall seconds, inflated by load):
stage 2: 452, stage 3: 1888 (section 34: 1436), stage 4: 529, stage 9: 602, stage 10: 534,
stage 5: >515, stage 7: >1029 (section 24: 831), stage 1: section 3 (1007 programs x 4 runs) alone >40 min.
Section 3 was therefore split into 4 shards and spread over stages 1-4.

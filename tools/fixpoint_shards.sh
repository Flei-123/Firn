#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/.."
SHARDS=${1:-6}
JOBS=${2:-2}
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
export FIXPOINT_STAGES_ONLY=1 FIXPOINT_EXPORT="$OUT/firnc2"
bash tools/fixpoint.sh || { echo "fixpoint stages failed"; exit 1; }
export FIRNC1="$OUT/firnc2"
HEAVY=${FIXPOINT_HEAVY:-/root/jarvis/bin/heavy}
[ -x "$HEAVY" ] || HEAVY=
k=0
running=0
while [ "$k" -lt "$SHARDS" ]; do
    SELF_COMPARE_SHARD=$k/$SHARDS $HEAVY bash tools/self_compare.sh > "$OUT/shard$k.txt" 2>&1 &
    k=$((k+1))
    running=$((running+1))
    if [ "$running" -ge "$JOBS" ]; then
        wait -n
        running=$((running-1))
    fi
done
wait
rc=0
for k in $(seq 0 $((SHARDS-1))); do
    echo "== shard $k/$SHARDS"
    sed 's/^/  /' "$OUT/shard$k.txt"
done
for key in "SAME BEHAVIOUR" "DIFFERING" "FAULTY" "NOT CORE" "DEFER" "COMPTIME" "CODEGEN MISSING" "SKIPPED"; do
    total=$(cat "$OUT"/shard*.txt | awk -v k="$key:" 'index($0,k)==1 {split(substr($0,length(k)+1),a," "); s+=a[1]} END{print s+0}')
    echo "TOTAL $key: $total"
    case "$key" in DIFFERING|FAULTY) [ "$total" -ne 0 ] && rc=1;; esac
done
[ "$rc" -eq 0 ] && echo "CORPUS:    .firnc2 behaves like firnc0 (sharded)"
exit $rc

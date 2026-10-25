#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
HEAVY="${HEAVY:-/root/jarvis/bin/heavy}"
LOGS="${STAGE_LOG_DIR:-$(pwd)/.test-stage-logs}"
last=$(bash tools/teststages/select.sh --count) || exit 2

use_heavy=0
parallel=0
stages=()
for arg in "$@"; do
    case "$arg" in
        --heavy) use_heavy=1 ;;
        --parallel) use_heavy=1; parallel=1 ;;
        --list) bash tools/teststages/select.sh --list; exit 0 ;;
        *[!0-9]*|'') echo "usage: run.sh [--heavy|--parallel|--list] [stage ...]" >&2; exit 2 ;;
        *) stages+=("$arg") ;;
    esac
done
if [ "${#stages[@]}" -eq 0 ]; then
    stages=($(seq 1 "$last"))
fi
mkdir -p "$LOGS"

run_stage() {
    local n="$1" log="$LOGS/stage$1.log"
    if [ "$use_heavy" -eq 1 ] && [ -x "$HEAVY" ]; then
        HEAVY_TIMELIMIT="${HEAVY_TIMELIMIT:-75}" "$HEAVY" bash test.sh --stage "$n" > "$log" 2>&1
    else
        bash test.sh --stage "$n" > "$log" 2>&1
    fi
    echo $? > "$LOGS/stage$n.rc"
}

rm -f "$LOGS"/stage*.rc
if [ "$parallel" -eq 1 ]; then
    for n in "${stages[@]}"; do run_stage "$n" & done
    wait
else
    for n in "${stages[@]}"; do run_stage "$n"; done
fi

failed=0
for n in "${stages[@]}"; do
    rc=$(cat "$LOGS/stage$n.rc" 2>/dev/null || echo 99)
    summary=$(grep -E '^(PASS|FAIL) ' "$LOGS/stage$n.log" | tail -n 1)
    echo "stage $n: rc=$rc ${summary:-no summary} ($LOGS/stage$n.log)"
    [ "$rc" -eq 0 ] || failed=1
done
exit "$failed"

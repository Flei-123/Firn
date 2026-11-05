#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
STAGES=$(seq -s " " 1 "$(bash tools/teststages/select.sh --count)")
OUT=
KNOWN=$ROOT/tools/release/known_red.txt
FIXPOINT=1
usage() { echo "usage: gate.sh --out STAMP [--stages 'N ...'] [--known FILE] [--no-fixpoint]" >&2; exit 2; }
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT=$2; shift 2 ;;
        --stages) STAGES=$2; shift 2 ;;
        --known) KNOWN=$2; shift 2 ;;
        --no-fixpoint) FIXPOINT=0; shift ;;
        *) usage ;;
    esac
done
[ -n "$OUT" ] || usage
[ -z "$(git status --porcelain --untracked-files=no)" ] || { echo "working tree is not clean" >&2; exit 1; }
LOGS=$(mktemp -d)
trap 'rm -rf "$LOGS"' EXIT
COMMIT=$(git rev-parse HEAD)
RESULT=green
NEW_RED=()
failed_items() {
    awk '/^FAIL [0-9]+\/[0-9]+ failed:/ {on=1; next} on && /^  [^ ]/ {sub(/^  /, ""); sub(/ failed \(see .*$/, ""); sub(/[ \t]+$/, ""); print}' "$1"
}
for s in $STAGES; do
    /root/jarvis/bin/heavy bash test.sh --stage "$s" > "$LOGS/stage$s.log" 2>&1
    rc=$?
    if [ $rc -eq 0 ]; then
        echo "stage $s green"
        continue
    fi
    items=$(failed_items "$LOGS/stage$s.log")
    if [ -z "$items" ]; then
        echo "stage $s rc=$rc without a failure list"
        RESULT=red
        NEW_RED+=("stage$s:rc$rc")
        continue
    fi
    while IFS= read -r item; do
        if grep -qxF -- "$item" "$KNOWN" 2>/dev/null; then
            echo "stage $s known red: $item"
        else
            echo "stage $s NEW red: $item"
            RESULT=red
            NEW_RED+=("stage$s:$item")
        fi
    done <<< "$items"
done
if [ $FIXPOINT = 1 ]; then
    if FIXPOINT_STAGES_ONLY=1 /root/jarvis/bin/heavy bash tools/fixpoint.sh > "$LOGS/fixpoint.log" 2>&1; then
        echo "fixpoint green"
    else
        echo "fixpoint red"
        RESULT=red
        NEW_RED+=("fixpoint")
    fi
fi
{
    echo "commit=$COMMIT"
    echo "result=$RESULT"
    echo "stages=$STAGES"
    echo "fixpoint=$FIXPOINT"
    echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for r in "${NEW_RED[@]:-}"; do [ -n "$r" ] && echo "red=$r"; done
} > "$OUT"
echo "gate $RESULT -> $OUT"
[ "$RESULT" = green ]

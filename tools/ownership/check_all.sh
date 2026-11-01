#!/bin/bash
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
HEAVY=${HEAVY:-/root/jarvis/bin/heavy}
MODE=${OWN_MODE:-check}
OUT=${OWN_OUT:-}
BASE=${OWN_BASE:-}
JOBS=${JOBS:-1}
ONLY=()
CMD=run
while [ $# -gt 0 ]; do
  case $1 in
    --mode) MODE=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    --base) BASE=$2; shift 2 ;;
    --jobs) JOBS=$2; shift 2 ;;
    --list) cut -d'|' -f1 "$HERE/steps.txt"; exit 0 ;;
    --summary) CMD=summary; shift ;;
    -*) echo "usage: check_all.sh [--mode check|on|off] [--out DIR] [--base DIR] [--jobs N<=3] [--list] [--summary] [step...]" >&2; exit 2 ;;
    *) ONLY+=("$1"); shift ;;
  esac
done
[ "$JOBS" -gt 3 ] && JOBS=3
[ -n "$OUT" ] || OUT=${OWN_OUT_ROOT:-/root/jarvis/data/staff_scratch/own-check}/$MODE
mkdir -p "$OUT"
cd "$ROOT" || exit 2

if [ "$CMD" = summary ]; then
  if [ -n "$BASE" ]; then python3 -I "$HERE/summarize.py" "$OUT" "$BASE"; else python3 -I "$HERE/summarize.py" "$OUT"; fi
  exit $?
fi

run_step() {
  local name=$1 minutes=$2 cmd=$3
  rm -f "$OUT/$name.rc"
  HEAVY_TIMELIMIT=$minutes "$HEAVY" env FIRNC_OWNERSHIP="$MODE" OUT="$OUT" bash -c "$cmd" > "$OUT/$name.log" 2>&1
  echo $? > "$OUT/$name.rc"
}

while IFS='|' read -r name minutes cmd; do
  [ -n "$name" ] || continue
  if [ ${#ONLY[@]} -gt 0 ]; then
    keep=0
    for o in "${ONLY[@]}"; do [ "$o" = "$name" ] && keep=1; done
    [ $keep = 1 ] || continue
  fi
  while [ "$(jobs -rp | wc -l)" -ge "$JOBS" ]; do sleep 5; done
  run_step "$name" "$minutes" "$cmd" &
done < "$HERE/steps.txt"
wait
python3 -I "$HERE/summarize.py" "$OUT" ${BASE:+"$BASE"}

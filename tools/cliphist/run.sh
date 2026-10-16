#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/cliphist/run.sh -- lib/std/cliphist.fi + lib/fui/cliphistview.fi held against the outside.
#   1. the history rules and the file format against the Python model (tools/cliphist/model.py), all four levels
#      (CLIPHIST_OPS=600 CLIPHIST_SEEDS="1 2 3")
#   2. the watcher and the view against a real X11 clipboard (private Xvfb): copy, take-in, duplicates, html,
#      password-manager hint dropped / kept, timer path, view rows, pin, pick, delete
# Exit 0 = 0 differences, 0 failed checks.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$PWD/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-cliphist.XXXXXX"); trap 'rm -rf "$W"; [ -n "${XP:-}" ] && kill $XP 2>/dev/null' EXIT
ERR=0
echo "== 1. model cross-check"
for stage in "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast" "release-fast:--opt-level=release-fast" "release-safe:--opt-level=release-safe"; do
  n=${stage%%:*}; o=${stage#*:}
  $FIRNC $o -o "$W/check.$n" tools/cliphist/check_main.fi 2> "$W/err" || { echo "FAIL compile $n"; head -5 "$W/err"; ERR=1; continue; }
  for s in ${CLIPHIST_SEEDS:-1 2 3}; do
    python3 tools/cliphist/model.py gen $s ${CLIPHIST_OPS:-600} "$W/ops" "$W/exp"
    "$W/check.$n" "$W/ops" "$W/res" && out=$(python3 tools/cliphist/model.py cmp "$W/exp" "$W/res" | tail -1) || out="crash"
    echo "  $n seed $s: $out"; [ "$out" = "0 differences" ] || ERR=1
  done
done
echo "== 2. watcher + view against X11 (Xvfb)"
if command -v Xvfb >/dev/null; then
  D=$((300 + RANDOM % 80)); Xvfb ":$D" -screen 0 640x480x24 -nolisten tcp >/dev/null 2>&1 & XP=$!
  sleep 1
  for stage in "dev-fast:--opt-level=dev-fast" "release-fast:--opt-level=release-fast" "release-safe:--opt-level=release-safe"; do
    n=${stage%%:*}; o=${stage#*:}
    $FIRNC $o -o "$W/watch.$n" tools/cliphist/watch_main.fi 2> "$W/err" || { echo "FAIL compile watch $n"; head -5 "$W/err"; ERR=1; continue; }
    out=$(DISPLAY=":$D" FIRN_CLIPBOARD=native timeout 90 "$W/watch.$n" 2>&1); code=$?
    echo "  $n: $(echo "$out" | tail -1) (exit $code)"; [ $code -eq 0 ] || ERR=1
  done
else echo "  skip: no Xvfb"; fi
[ $ERR -eq 0 ] && echo "CLIPHIST PASSED" || echo "CLIPHIST FAILED"
exit $ERR

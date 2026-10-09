#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/dialog/colordlg_run.sh -- THE PROOF FOR lib/fui/colordlg.fi (the colour dialog).
#
#   bash tools/dialog/colordlg_run.sh [proof-folder]
#
# 1. tools/fui/colordlg_main.fi, built unoptimised, runs without a window: the colour maths on a dense
#    raster, the dialog driven with synthetic pointer and key events (swatches, field, bar, the seven entry
#    fields, Old | New, buttons, Esc, Enter, custom colours and their store in a HOME of its own, accessibility
#    names and the audit, light and dark theme, the error ring).         last line:  COLORDLG PASSED
# 2. the same program built with --opt-level=release-fast: `speed` prints the FRAME_MS lines; every frame
#    has to be <= 16 ms at the dialog's size (620 x 460) and at 1240 x 720.   last line:  COLORDLG SPEED PASSED
# 3. tools/dialog/colordlg_live.py: the REAL dialog (tools/dialog/colordlg_live_main.fi) in a window on a
#    private Xvfb, operated with xdotool and read back from the server with xwd.   last line:
#    "colordlg live: all checks passed"  (or SKIP when Xvfb / xdotool / xwd / PIL are missing)
# 4. the tree rules for the new files: firnfmt -c and the English checks.
#
# The proof pictures of 1 and 3 go to <proof-folder> when it is given. Everything else lives in a
# temporary directory that is removed at the end. Exit code 0 = everything passed.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
PROOF="${1:-}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
FAIL=0
[ -n "$PROOF" ] && mkdir -p "$PROOF"

echo "== 1. colordlg_main (checks without a window) =="
"$FIRNC" --opt-level=dev -o "$W/colordlg" tools/fui/colordlg_main.fi || exit 1
mkdir -p "$W/home"
COLORDLG_TEST=1 HOME="$W/home" "$W/colordlg" ${PROOF:+"$PROOF"} | tee "$W/check.out" | grep -v '^  ok ' | tail -n 40
tail -n 1 "$W/check.out" | grep -q '^COLORDLG PASSED$' || { echo "FAILED: colordlg_main"; FAIL=1; }
echo "   $(grep -c '^  ok ' "$W/check.out") checks ok, $(grep -c '^  WRONG' "$W/check.out") wrong"

echo
echo "== 2. FRAME_MS (release-fast) =="
"$FIRNC" --opt-level=release-fast -o "$W/colordlg_fast" tools/fui/colordlg_main.fi || exit 1
"$W/colordlg_fast" speed | tee "$W/speed.out" | grep -v '^  ok '
tail -n 1 "$W/speed.out" | grep -q '^COLORDLG SPEED PASSED$' || { echo "FAILED: speed"; FAIL=1; }

echo
echo "== 3. colordlg_live (a real window on Xvfb) =="
"$FIRNC" --opt-level=dev -o "$W/colordlg_live_main" tools/dialog/colordlg_live_main.fi || exit 1
python3 tools/dialog/colordlg_live.py "$W/colordlg_live_main" ${PROOF:+"$PROOF"} | tee "$W/live.out" | grep -v '^  OK '
if grep -q 'SKIP' "$W/live.out"; then
    echo "   (skipped)"
else
    tail -n 1 "$W/live.out" | grep -q '^colordlg live: all checks passed$' || { echo "FAILED: live"; FAIL=1; }
    echo "   $(grep -c '^  OK ' "$W/live.out") live checks ok"
fi

echo
echo "== 4. firnfmt -c and the English checks =="
"$FIRNC" -o "$W/firnfmt" tools/fmt/firnfmt.fi || exit 1
"$W/firnfmt" -c lib/fui/colordlg.fi lib/fui/app.fi tools/fui/colordlg_main.fi tools/dialog/colordlg_live_main.fi \
    && echo "   canonical" || { echo "FAILED: firnfmt -c"; FAIL=1; }
python3 tools/english/check.py | tail -n 1
python3 tools/english/check_comments.py | tail -n 1

echo
if [ "$FAIL" -eq 0 ]; then echo "COLORDLG RUN PASSED"; else echo "COLORDLG RUN FAILED"; fi
exit "$FAIL"

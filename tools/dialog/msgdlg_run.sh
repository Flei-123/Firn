#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/dialog/msgdlg_run.sh -- THE ACCEPTANCE RUN OF THE MESSAGE BOX (lib/fui/msgdlg.fi).
#
#     sh tools/dialog/msgdlg_run.sh [directory for the pictures]
#
# 1. tools/fui/msgdlg_main.fi, built unoptimised, runs every check without a window
#    (16 cases of 4 kinds x 4 button sets, keys, mouse, texts, scrolling, contrast in 6 themes,
#    accessibility, scales 1.04 .. 2). Expected last line: MSGDLG PASSED. The first argument of
#    the program is a directory for its pictures.
# 2. the same program built with --opt-level=release-fast and run with `speed`: every FRAME_MS line
#    (first paint, steady frame, hover frame, scroll step; a short and a 4,000 character text; at
#    the dialog's size and at 1240x720) has to be <= 16. Expected last line: MSGDLG PASSED.
# 3. the audit: `FUI_AUDIT=1 msgdlg_live 1 3 t x` opens no window and prints the accessibility audit,
#    which has to say "0 unnamed".
# 4. tools/dialog/msgdlg_live.py: the box in a real window on a private Xvfb (xdotool, xwd).
#    Expected last line: MSGDLG LIVE PASSED (or "SKIP ..." when Xvfb / xdotool / xwd / PIL / python-xlib
#    are missing).
#
# Builds are small (about 10 s each); the whole run is a few minutes, most of it the live part
# (about 60 windows are opened one after the other). Everything lives in a directory of its own,
# removed at the end; only the pictures asked for stay.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
PICS="${1:-}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
FAIL=0

echo "== 1. msgdlg_main (no window) =="
"$FIRNC" --opt-level=dev -o "$W/msgdlg" tools/fui/msgdlg_main.fi
mkdir -p "$W/png"
"$W/msgdlg" "$W/png" > "$W/msgdlg.out" || FAIL=1
grep -c '^  ok ' "$W/msgdlg.out" | sed 's/$/ checks ok/'
grep 'WRONG' "$W/msgdlg.out" || true
tail -1 "$W/msgdlg.out"
if [ -n "$PICS" ]; then
    mkdir -p "$PICS"
    cp "$W/png"/*.png "$PICS"/ 2>/dev/null || true
fi

echo "== 2. speed (release-fast, FRAME_MS) =="
"$FIRNC" --opt-level=release-fast -o "$W/msgdlg_fast" tools/fui/msgdlg_main.fi
"$W/msgdlg_fast" speed > "$W/speed.out" || FAIL=1
grep '^FRAME_MS' "$W/speed.out"
tail -1 "$W/speed.out"

echo "== 3. the audit =="
"$FIRNC" --opt-level=dev -o "$W/msgdlg_live" tools/dialog/msgdlg_live_main.fi
FUI_AUDIT=1 "$W/msgdlg_live" 1 3 t x | tee "$W/audit.out"
grep -q ' 0 unnamed' "$W/audit.out" || { echo "  FAIL  the audit found an unnamed control"; FAIL=1; }

echo "== 4. the real window (Xvfb) =="
python3 tools/dialog/msgdlg_live.py "$W/msgdlg_live" ${PICS:+"$PICS"} || FAIL=1

if [ "$FAIL" -ne 0 ]; then
    echo "MSGDLG RUN FAILED"
    exit 1
fi
echo "MSGDLG RUN PASSED"

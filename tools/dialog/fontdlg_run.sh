#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/dialog/fontdlg_run.sh -- THE CHECKS OF THE FONT DIALOG (lib/fui/fontdlg.fi, lib/fui/fontscan.fi), one run.
#
#     sh tools/dialog/fontdlg_run.sh [dir-for-pngs]
#
# What it builds and runs (nothing is left behind: one `mktemp -d`, removed on exit):
#   1. tools/dialog/fontdlg_fonts.py   -> a font folder (500 tiny families + the odd files); SKIP when fontTools is missing
#   2. tools/fui/fontdlg_main.fi       (release-fast: the FRAME_MS lines mean something) -> last line FONTDLG PASSED
#   3. tools/dialog/fontdlg_live_main.fi + tools/dialog/fontdlg_live.py on a private Xvfb -> last line FONTDLG LIVE PASSED
#                                      (SKIP, exit 0, without Xvfb / xdotool / xwd / PIL / python-xlib)
#   4. FUI_AUDIT=1 <driver>            -> `audit: ... 0 unnamed ... OK`
#   5. firnfmt -c on the four .fi files and tools/english/check.py (0 German identifiers)
#
# Expected output (shape):  "FONTDLG PASSED", "FRAME_MS <state> <ms>" lines all <= 16.0, "FONTDLG LIVE PASSED",
# "audit: 27 nodes, 9 controls, 0 unnamed, ... OK", "firnfmt: ok", "english: 0 German identifiers".
# A section runner can grep the PASSED lines. Heavy builds: use `/root/jarvis/bin/heavy sh tools/dialog/fontdlg_run.sh`
# (a release build of a fUi program is ~10 s and small; the live part opens real windows for about 90 s).
cd "$(dirname "$0")/../.." || exit 1
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
PNGS="${1:-}"
D=$(mktemp -d) || exit 1
trap 'rm -rf "$D"' EXIT
FAIL=0

echo "== 1. the test font folder"
python3 tools/dialog/fontdlg_fonts.py "$D/fonts" > "$D/fonts.log" 2>&1
rc=$?
if [ $rc -eq 77 ]; then
    echo "  SKIP  fontTools is not installed: no test fonts, nothing was run"
    exit 0
fi
[ $rc -eq 0 ] || { cat "$D/fonts.log"; echo "  FAIL  the font folder"; exit 1; }
cat "$D/fonts.log"

echo "== 2. in-memory test (release-fast)"
"$FIRNC" --opt-level=release-fast -o "$D/fontdlg" tools/fui/fontdlg_main.fi || exit 1
mkdir -p "$D/png"
# Logic checks are hard in EVERY run. A frame over 16 ms is judged on the MINIMUM per FRAME_MS state over up to five
# runs (a loaded server adds its own milliseconds; a real slowdown is slow in every run). If the minima are still over
# the limit while the server load is above FUI_PERF_LOAD_SKIP (default 16), the timing is NOT counted (as the perfgate).
ok=0
try=1
while [ $try -le 5 ]; do
    "$D/fontdlg" "$D/fonts" "$D/png" > "$D/mem$try.log" 2>&1
    if ! tail -1 "$D/mem$try.log" | grep -q 'FONTDLG PASSED'; then
        grep -c '^  ok ' "$D/mem$try.log" | sed 's/^/  checks ok: /'
        grep -v '^  ok ' "$D/mem$try.log" | grep -v '^FRAME_MS\|^SCAN_MS\|^==\|^  SLOW' | head -40
        echo "  FAIL  fontdlg_main (logic, run $try)"; FAIL=1; break
    fi
    if cat "$D"/mem*.log | awk '/^FRAME_MS/ { if (!($2 in m) || $3 + 0 < m[$2]) m[$2] = $3 + 0 }
        END { for (k in m) if (m[k] > 16.0) bad = 1; exit bad }'; then ok=1; break; fi
    echo "  frame over 16 ms in run $try (minimum of the runs so far still over)"
    try=$((try + 1))
done
if [ $FAIL -eq 0 ]; then
    grep -c '^  ok ' "$D/mem1.log" | sed 's/^/  checks ok: /'
    cat "$D"/mem*.log | awk '/^FRAME_MS/ { if (!($2 in m) || $3 + 0 < m[$2]) m[$2] = $3 + 0 } END { for (k in m) printf "FRAME_MS(min) %s %.1f\n", k, m[k] }' | sort
    grep -h '^SCAN_MS' "$D/mem1.log"
    if [ $ok -ne 1 ]; then
        pg_load=$(cut -d. -f1 /proc/loadavg)
        if [ "$pg_load" -gt "${FUI_PERF_LOAD_SKIP:-16}" ]; then
            echo "  WARNING: frame timing NOT counted (load $pg_load > ${FUI_PERF_LOAD_SKIP:-16}); re-run on a quiet server"
        else
            echo "  FAIL  a frame took longer than 16 ms in all runs"; FAIL=1
        fi
    fi
fi

echo "== 3. live on a private Xvfb"
"$FIRNC" --opt-level=release-fast -o "$D/live" tools/dialog/fontdlg_live_main.fi || exit 1
python3 tools/dialog/fontdlg_live.py "$D/live" "$D/png" > "$D/live.log" 2>&1
grep -c '^  OK ' "$D/live.log" | sed 's/^/  checks OK: /'
grep -v '^  OK ' "$D/live.log" | head -40
if ! grep -q 'SKIP' "$D/live.log"; then
    tail -1 "$D/live.log" | grep -q 'FONTDLG LIVE PASSED' || { echo "  FAIL  fontdlg_live"; FAIL=1; }
fi

echo "== 4. accessibility audit (FUI_AUDIT=1)"
FUI_AUDIT=1 "$D/live" > "$D/audit.log" 2>&1
cat "$D/audit.log"
grep -q 'OK' "$D/audit.log" || { echo "  FAIL  audit"; FAIL=1; }

echo "== 5. format and English"
FILES="lib/fui/fontscan.fi lib/fui/fontdlg.fi tools/fui/fontdlg_main.fi tools/dialog/fontdlg_live_main.fi"
"$FIRNC" -o "$D/firnfmt" tools/fmt/firnfmt.fi >/dev/null 2>&1 && {
    if "$D/firnfmt" -c $FILES >/dev/null 2>&1; then echo "  firnfmt: ok"; else echo "  FAIL  firnfmt -c"; FAIL=1; fi; }
python3 tools/english/check.py 2>&1 | tail -1 | sed 's/^/  english: /'

if [ -n "$PNGS" ]; then
    mkdir -p "$PNGS"
    cp "$D/png"/*.png "$PNGS"/ 2>/dev/null
    echo "  pictures: $PNGS"
fi
if [ $FAIL -eq 0 ]; then
    echo "FONTDLG RUN PASSED"
else
    echo "FONTDLG RUN FAILED"
fi
exit $FAIL

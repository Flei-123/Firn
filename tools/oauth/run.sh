#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/oauth/run.sh -- lib/auth (jose, oauth, msa).
#
#   1. oauth_main.fi and msa_main.fi build in three stages (opt, --no-opt, dev-fast)
#   2. tools/oauth/check.py: the OAuth client against fake_idp.py (PKCE, loopback
#      redirect, device flow, refresh, id_token refusals, keyring)
#   3. tools/oauth/check_msa.py: the Microsoft/Xbox/Minecraft chain against
#      fake_msa.py and -- when there is a route -- the real hosts with bogus
#      credentials (section L)
#   4. the Windows builds under Wine, the same checks
#
# The in-process half is tests/2150 (lib/auth/jose.fi against tokens signed by
# Python's `cryptography`), tests/2151 and tests/2152; test.sh runs them.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for SRC in tools/oauth/oauth_main.fi tools/oauth/msa_main.fi; do
    BASE=$(basename "$SRC" .fi)
    for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
        NAME=${STAGE%%:*}
        OPT=${STAGE#*:}
        if ! "$FIRNC" $OPT -o "$WORK/${BASE}_$NAME" "$SRC" 2> "$WORK/b.log"; then
            echo "  FAIL $SRC does not compile ($NAME)"
            grep -v RWX "$WORK/b.log" | head -5
            rc=1
        fi
    done
done
[ $rc -eq 0 ] || exit 1
echo "   oauth_main and msa_main built: opt, --no-opt, dev-fast"
python3 tools/oauth/check.py "$WORK/oauth_main_opt" > "$WORK/o.log" 2>&1 || { rc=1; grep -v "^  OK" "$WORK/o.log" | head -30; }
echo "   check.py (opt): $(tail -1 "$WORK/o.log")"
python3 tools/oauth/check_msa.py "$WORK/msa_main_opt" > "$WORK/m.log" 2>&1 || { rc=1; grep -v "^  OK" "$WORK/m.log" | head -30; }
echo "   check_msa.py (opt): $(tail -1 "$WORK/m.log")"
for NAME in noopt dev; do
    python3 tools/oauth/check.py "$WORK/oauth_main_$NAME" a b c f g h i j k > "$WORK/o_$NAME.log" 2>&1 || { rc=1; grep -v "^  OK" "$WORK/o_$NAME.log" | head -20; }
    python3 tools/oauth/check_msa.py "$WORK/msa_main_$NAME" a b c d e f g > "$WORK/m_$NAME.log" 2>&1 || { rc=1; grep -v "^  OK" "$WORK/m_$NAME.log" | head -20; }
    echo "   $NAME: $(tail -1 "$WORK/o_$NAME.log") | $(tail -1 "$WORK/m_$NAME.log")"
done

WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    echo "== the Windows builds (x86_64-windows) under Wine =="
    export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
    export WINEDEBUG=${WINEDEBUG:--all}
    WB=1
    for SRC in tools/oauth/oauth_main.fi tools/oauth/msa_main.fi; do
        BASE=$(basename "$SRC" .fi)
        if ! "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$WORK/$BASE.exe" "$SRC" 2> "$WORK/w.log"; then
            echo "  FAIL $SRC does not build for Windows"
            grep -v RWX "$WORK/w.log" | head -5
            rc=1
            WB=0
        fi
    done
    if [ "$WB" = 1 ]; then
        ROOT=$(pwd)
        (cd "$WORK" && RUNNER="$WINE" python3 "$ROOT/tools/oauth/check.py" "$WORK/oauth_main.exe" a b c d f g h i k > "$WORK/wo.log" 2>&1) || { rc=1; grep -v "^  OK" "$WORK/wo.log" | head -20; }
        (cd "$WORK" && RUNNER="$WINE" python3 "$ROOT/tools/oauth/check_msa.py" "$WORK/msa_main.exe" a b c d e f h > "$WORK/wm.log" 2>&1) || { rc=1; grep -v "^  OK" "$WORK/wm.log" | head -20; }
        echo "   windows: $(tail -1 "$WORK/wo.log") | $(tail -1 "$WORK/wm.log")"
    fi
else
    echo "   SKIP the Windows build: Wine or mingw is missing"
fi
exit $rc

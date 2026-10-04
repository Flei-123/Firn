#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/db/run.sh -- lib/db against SQLite (Python's sqlite3 module, a real SQLite 3):
#
#   parser      70 statements accepted/refused like SQLite's parser          check_parse.py
#   expressions 22,000 constant expressions, bit for bit                     check_expr.py
#   B-trees     table and index trees edited by lib/db, read by SQLite       check_bt.py, check_idx.py
#   SELECT      106 queries (joins, aggregates, sub-selects, compound ...)   check_select.py
#   DML/DDL     INSERT/UPDATE/DELETE/ALTER/DROP/upsert/transactions, 1,500
#               random statements, files read by SQLite both ways            check_dml.py
#   crashes     kill -9 / SIGKILL at every commit event, recovery by SQLite
#               and by lib/db                                                check_crash.py
#   locks       SQLite and lib/db processes on one file                      check_lock.py
#   hostile     damaged files: no crash, panic or hang                       check_hostile.py
#   Windows     the same programs built for x86_64-windows, run under Wine   (RUNNER=wine)
#
# Usage: tools/db/run.sh [quick]        quick = fewer hostile files, no Windows build
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
HOSTILE=${HOSTILE_ROUNDS:-1500}
[ "${1:-}" = "quick" ] && HOSTILE=300
for t in sql_probe parse_probe expr_probe bt_probe; do
    "$FIRNC" -o "$W/$t" "tools/db/$t.fi" > "$W/$t.log" 2>&1 || { echo "  FAIL $t does not build"; grep -v RWX "$W/$t.log" | head -5; rc=1; }
done
[ $rc -eq 0 ] || exit 1
run() {
    local name=$1
    local lines=$2
    shift 2
    echo "-- $name"
    "$@" > "$W/out.log" 2>&1 && ok=0 || ok=$?
    tail -n "$lines" "$W/out.log" | sed 's/^/   /'
    if [ $ok -ne 0 ]; then
        echo "   FAILED: $name"
        rc=1
    fi
}
run "parser" 2 python3 tools/db/check_parse.py "$W/parse_probe"
run "expressions" 3 python3 tools/db/check_expr.py "$W/expr_probe"
run "table B-tree" 1 python3 tools/db/check_bt.py "$W/bt_probe"
run "index B-tree" 1 python3 tools/db/check_idx.py "$W/bt_probe"
run "SELECT" 3 python3 tools/db/check_select.py "$W/sql_probe"
run "DML, DDL, transactions" 8 python3 tools/db/check_dml.py "$W/sql_probe"
run "crashes" 8 python3 tools/db/check_crash.py "$W/sql_probe"
run "locks" 12 python3 tools/db/check_lock.py "$W/sql_probe"
run "damaged files ($HOSTILE)" 4 python3 tools/db/check_hostile.py "$W/sql_probe" "$HOSTILE"
if [ "${1:-}" != "quick" ]; then
    WINE=${WINE:-}
    if [ -z "$WINE" ]; then
        for c in wine64 wine /usr/lib/wine/wine64; do
            if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
        done
    fi
    if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
        echo "== the Windows build (x86_64-windows) under Wine =="
        export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
        export WINEDEBUG=${WINEDEBUG:--all}
        if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$W/sql_probe.exe" tools/db/sql_probe.fi 2> "$W/w.log"; then
            export RUNNER="$WINE"
            run "SELECT (Wine)" 2 python3 tools/db/check_select.py "$W/sql_probe.exe"
            run "DML, DDL, transactions (Wine)" 8 python3 tools/db/check_dml.py "$W/sql_probe.exe"
            run "crash points (Wine)" 6 python3 tools/db/check_crash.py "$W/sql_probe.exe"
            run "damaged files (Wine)" 3 python3 tools/db/check_hostile.py "$W/sql_probe.exe" 150
            unset RUNNER
        else
            echo "  FAIL the Windows build does not compile"
            grep -v RWX "$W/w.log" | head -5
            rc=1
        fi
    else
        echo "   SKIP the Windows build: Wine or mingw is missing"
    fi
fi
exit $rc

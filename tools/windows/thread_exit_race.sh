#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/thread_exit_race.sh [runs] -- how often does Wine kill a process
# that joined a thread? (see thread_exit_race.fi). Needs Wine and mingw.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn} WINEDEBUG=-all
N=${1:-300}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
"$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$W/t.exe" tools/windows/thread_exit_race.fi 2>&1 | grep -v RWX
killed=0; bad=0
for i in $(seq 1 "$N"); do
    timeout 60 wine "$W/t.exe" > /dev/null 2>&1
    rc=$?
    if [ $rc -eq 137 ]; then killed=$((killed + 1)); elif [ $rc -ne 0 ]; then bad=$((bad + 1)); fi
done
echo "runs $N: killed by SIGKILL $killed, other non-zero $bad"

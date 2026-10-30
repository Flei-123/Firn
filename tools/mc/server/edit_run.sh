#!/usr/bin/env bash
# tools/mc/server/edit_run.sh -- stage I of run.sh alone: block edits saved by the Firn server, loaded by vanilla 1.20.1
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$REPO/lib"
JARS=${MC_JARS:-/root/.w5/jars}
TOOLS="$REPO/tools/mc/server"
W=$(mktemp -d)
VEPID=""
cleanup() {
    exec 5>&- 2>/dev/null
    kill $(jobs -p) 2>/dev/null
    [ -n "$VEPID" ] && kill "$VEPID" 2>/dev/null
    sleep 1
    [ -n "$VEPID" ] && kill -9 "$VEPID" 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT
ERR=0
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
SAVE_CACHE=${SAVE_CACHE:-/root/.w5/savetest}
WORLD=${MCS_WORLD:-$SAVE_CACHE/v1201/world}
report() { while IFS= read -r l; do echo "$l"; case "$l" in *FAIL*) touch "$W/failed";; esac; done; }
$FIRNC -o "$W/mcsrv" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv"; head -5 "$W/err"; exit 1; }
source "$TOOLS/stage_edit.sh"
[ -f "$W/failed" ] && ERR=1
exit $ERR

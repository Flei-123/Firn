#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/net/epoll.sh -- async.loop EV_RDHUP / EV_ET against Python's
# select.epoll / select.poll (loopback only). Output of
# tests/2867_async_epoll_flags.fi must equal tools/net/epoll_ref.py.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
    N=${STAGE%%:*}; O=${STAGE#*:}
    if ! "$FIRNC" $O -o "$WORK/t_$N" tests/2867_async_epoll_flags.fi 2> "$WORK/b.log"; then
        echo "  FAIL build ($N)"; grep -v RWX "$WORK/b.log" | head -5; rc=1; continue
    fi
    timeout 60 "$WORK/t_$N" > "$WORK/out_$N" || { echo "  FAIL run ($N)"; rc=1; }
    python3 tools/net/epoll_ref.py > "$WORK/ref"
    if diff "$WORK/ref" "$WORK/out_$N" > "$WORK/d"; then
        echo "  ok   epoll flags ($N): $(wc -l < "$WORK/ref") lines equal Python"
    else
        echo "  FAIL epoll flags ($N) differ from Python:"; head -10 "$WORK/d"; rc=1
    fi
done
exit $rc

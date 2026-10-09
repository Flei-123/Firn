#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/billig/arm_icount.sh -- THE EXACTLY EXECUTED INSTRUCTIONS ON ARM64.
#
# The counterpart of `tools/bench90/icount.py`, which counts with callgrind on
# x86. There is no callgrind for ARM64 here (and no silicon), so qemu counts
# itself: `-singlestep -d cpu` writes one register block to the log per
# EXECUTED instruction.
#
# TWO TRAPS, both paid for dearly, both avoided here:
#
#   1. `-D /dev/stdout` does NOT write into the pipe. qemu opens the file
#      itself; what arrives is nothing. Hence a NAMED PIPE (`mkfifo`): the
#      counter reads, qemu writes, and nothing lands on the disk -- which is
#      also necessary, because a log of this kind costs about 1060 octets PER
#      INSTRUCTION.
#
#   2. The lines do NOT start with `PC=` -- `grep -c '^PC='` simply returns 0
#      without complaining. Exactly ONE field stands once per instruction in
#      the block: `X00=`. That is the counter.
#
# Usage:  bash tools/billig/arm_icount.sh <time-limit-s> <binary> [<binary> ...]
set -u
LIMIT=${1:?time limit missing}
shift
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIFO="$D/fifo"
RC="$D/rc"
for b in "$@"; do
    rm -f "$FIFO"
    mkfifo "$FIFO"
    ( timeout "$LIMIT" qemu-aarch64 -singlestep -d cpu -D "$FIFO" "$b" \
        >/dev/null 2>&1; echo $? > "$RC" ) &
    n=$(grep -c 'X00=' "$FIFO")
    wait
    if [ "$(cat "$RC")" = "124" ]; then
        echo "$(basename "$b") TIME LIMIT (>$LIMIT s, at least $n instructions)"
    else
        echo "$(basename "$b") $n"
    fi
done

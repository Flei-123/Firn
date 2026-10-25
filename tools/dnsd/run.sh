#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/dnsd/run.sh -- the authoritative DNS server (lib/net/dnsd*.fi) as a process.
#
#   1. lib/net/dnsd_main.fi builds in three stages (opt / --no-opt / dev-fast)
#   2. --check accepts the sample zone and refuses a bad one with the line number
#   3. tools/dnsd/check.py: raw UDP/TCP questions decoded here and by `dig`
#
# The in-process half is tests/5070 (the wire format) and tests/5071 (real sockets, the
# resolver of lib/net/dns.fi as the client); test.sh runs both in section 3.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
    NAME=${STAGE%%:*}
    OPT=${STAGE#*:}
    if ! "$FIRNC" $OPT -o "$WORK/dnsd_$NAME" lib/net/dnsd_main.fi 2> "$WORK/b.log"; then
        echo "  FAIL lib/net/dnsd_main.fi does not compile ($NAME)"
        grep -v RWX "$WORK/b.log" | head -5
        exit 1
    fi
done
echo "   dnsd_main built: opt, --no-opt, dev-fast"
printf 'zone a.test\nbogus x 1\n' > "$WORK/bad.conf"
"$WORK/dnsd_opt" "$WORK/bad.conf" --check 2> "$WORK/bad.err"
[ $? -eq 2 ] && grep -q 'line 2' "$WORK/bad.err" || { echo "  FAIL bad zone not refused with its line"; rc=1; }
"$WORK/dnsd_opt" tools/dnsd/zone.conf --check > "$WORK/ok.out" || { echo "  FAIL sample zone refused"; rc=1; }
for NAME in opt noopt dev; do
    python3 tools/dnsd/check.py "$WORK/dnsd_$NAME" tools/dnsd/zone.conf > "$WORK/c_$NAME.log" 2>&1 \
        || { grep -v "^  ok" "$WORK/c_$NAME.log"; rc=1; }
    tail -1 "$WORK/c_$NAME.log"
done
exit $rc

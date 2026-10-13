#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/net/gate_run.sh -- gate 137: loop flags (EV_RDHUP / EV_ET) against Python select, IPv6 DNS server
# on [::1] against a Python fake DNS, closure pin counter-check. Loopback only; needs python3.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib" FIRNC
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
bash tools/net/epoll.sh || rc=1
bash tools/net/pin_mutation.sh || rc=1
if "$FIRNC" -o "$WORK/dns_main" lib/net/dns_main.fi 2> "$WORK/b.log"; then
    python3 tools/dns/check6.py "$WORK/dns_main" | sed 's/^/  /' | grep -E 'FAIL|RESULT' || true
    python3 tools/dns/check6.py "$WORK/dns_main" > "$WORK/c6.log" 2>&1 || { echo "  FAIL dns6"; grep FAIL "$WORK/c6.log"; rc=1; }
else
    echo "  FAIL dns_main does not build"; grep -v RWX "$WORK/b.log" | head -5; rc=1
fi
exit $rc

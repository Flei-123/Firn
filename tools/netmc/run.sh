#!/usr/bin/env bash
# tools/netmc/run.sh -- net.ip6 / SRV / net.addrinfo / IPv6 sockets against Python.
# Prints `RESULT netmc: ok (...)`. Build: dev-fast, temp dir removed on exit.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB="$(pwd)/lib" "$FIRNC" --opt-level=dev-fast -o "$D/probe" tools/netmc/probe.fi || exit 1
python3 tools/netmc/netmc_test.py "$D/probe"

#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/dns/run.sh -- the DNS resolver and https BY NAME (round DNS).
#
#   1. lib/net/dns_main.fi and lib/net/http_main.fi build in three stages
#      (opt / --no-opt / dev-fast)
#   2. tools/dns/check.py: the resolver against a fake DNS server in Python
#      (what the SERVER saw is counted from its log), against the real
#      network when there is one, https by name against a hermetic Python
#      TLS server and against the real internet (piston-meta.mojang.com,
#      api.modrinth.com, badssl.com for the refusals)
#
# The in-process half of the proof is in tests/2090 (the wire format against
# real answers and hostile messages) and tests/2091 (a DNS server inside the
# test process, UDP and TCP over 127.0.0.1); test.sh runs both.
#
# No route to the internet: the live parts SKIP, the hermetic ones run.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for SRC in lib/net/dns_main.fi lib/net/http_main.fi; do
    BASE=$(basename "$SRC" .fi)
    for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
        NAME=${STAGE%%:*}
        OPT=${STAGE#*:}
        if ! "$FIRNC" $OPT -o "$WORK/${BASE}_${NAME}" "$SRC" 2> "$WORK/b.log"; then
            echo "  FAIL $SRC does not compile ($NAME)"
            grep -v RWX "$WORK/b.log" | head -5
            rc=1
        fi
    done
done
[ $rc -eq 0 ] || exit 1
echo "   dns_main and http_main built: opt, --no-opt, dev-fast"
python3 tools/dns/check.py "$WORK/dns_main_opt" "$WORK/http_main_opt" || rc=1
echo "== the same checks, dev-fast build =="
python3 tools/dns/check.py "$WORK/dns_main_dev" "$WORK/http_main_dev" | tail -1 || rc=1

# THE WINDOWS BUILD, under Wine. The same two programs are built for
# x86_64-windows: the name servers then come from GetNetworkParams
# (lib/net/dnsconf.windows.fi), the roots from the certificate store
# (lib/tls/trust.windows.fi, crypt32), the sockets from ws2_32 through the
# seam. check.py runs again, against the .exe files. Skipped when mingw or
# Wine is missing (the same condition as tools/windows/run.sh).
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
    WB=1
    for SRC in lib/net/dns_main.fi lib/net/http_main.fi; do
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
        (cd "$WORK" && RUNNER="$WINE" python3 "$ROOT/tools/dns/check.py" "$WORK/dns_main.exe" "$WORK/http_main.exe" | grep -v "^  OK" ) || rc=1
    fi
else
    echo "   SKIP the Windows build: Wine or mingw is missing"
fi
exit $rc

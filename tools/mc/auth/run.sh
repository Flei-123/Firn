#!/usr/bin/env bash
# tools/mc/auth/run.sh -- cross-check of lib/mc/mc_auth.fi (server hash, offline UUID,
# session join against a LOCAL MOCK) plus the stand-in run of the reviewed lib/auth/msa.fi
# chain (tools/oauth/check_msa.py sections a-h, no real accounts, no network).
# The real sessionserver.mojang.com is NOT called (needs a real account).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/xauth" tools/mc/auth/xauth.fi 2>"$W/err"; then
        echo "  FAIL  xauth.fi does not compile [$name]"; head -10 "$W/err"; ERR=1; continue
    fi
    if python3 -I tools/mc/auth/check.py "$W/xauth" >"$W/py.out" 2>&1; then
        echo "  ok    python cross-check [$name]: $(tail -1 "$W/py.out")"
    else
        echo "  FAIL  python cross-check [$name]"; head -15 "$W/py.out"; ERR=1
    fi
    if [ -z "$stage" ]; then
        MC_TAMPER=1 python3 -I tools/mc/auth/check.py "$W/xauth" >"$W/tamper.out" 2>&1; trc=$?
        if [ $trc -eq 1 ] && grep -q "  FAIL  " "$W/tamper.out"; then
            echo "  ok    counter-check: tampered expectations are detected ($(grep -c '  FAIL  ' "$W/tamper.out") failing checks)"
        else
            echo "  FAIL  counter-check: tampered run exit $trc"; ERR=1
        fi
    fi
done
if $FIRNC -o "$W/msa_main" tools/oauth/msa_main.fi 2>"$W/err"; then
    if python3 tools/oauth/check_msa.py "$W/msa_main" a b c d e f g h >"$W/msa.out" 2>&1; then
        echo "  ok    msa chain vs stand-in (A-H): $(tail -1 "$W/msa.out")"
    else
        echo "  FAIL  msa chain stand-in"; grep -v "^  OK" "$W/msa.out" | head -15; ERR=1
    fi
else
    echo "  FAIL  msa_main.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-auth: ok" || echo "RESULT mc-auth: FAIL"
exit $ERR

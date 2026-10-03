#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/http/run.sh -- the HTTP/1.1 server, WebSocket, SSE and pairing of
# lib/http/server.fi and the WebSocket client of lib/ws/ws.fi, against
# curl, Python's http.client/ssl, websocket-client, websockets -- and, when
# docker and the image are there, the Autobahn test suite (fuzzingclient,
# every case except 12.* and 13.*, which are permessage-deflate).
#
#   bash tools/http/run.sh              (AUTOBAHN=0 skips the suite)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=".http-work"
mkdir -p "$WORK"
rc=0
"$FIRNC" -o "$WORK/srv" tools/http/test_main.fi > "$WORK/b1.log" 2>&1 || { grep -v RWX "$WORK/b1.log" | head; exit 1; }
"$FIRNC" -o "$WORK/wscli" tools/ws/client_main.fi > "$WORK/b2.log" 2>&1 || { grep -v RWX "$WORK/b2.log" | head; exit 1; }
python3 tools/http/check.py "$WORK/srv" "$WORK/chk" 2>&1 | tail -3 || rc=1
python3 tools/ws/check.py "$WORK/wscli" "$WORK/srv" "$WORK/wschk" 2>&1 | tail -3 || rc=1
if [ "${AUTOBAHN:-1}" != "0" ] && command -v docker > /dev/null \
    && docker image inspect crossbario/autobahn-testsuite > /dev/null 2>&1; then
    PORT=$((22000 + RANDOM % 900))
    rm -rf "$WORK/ab"
    mkdir -p "$WORK/ab/config" "$WORK/ab/reports"
    cat > "$WORK/ab/config/fuzzingclient.json" <<EOF
{"outdir": "/reports", "servers": [{"agent": "firn", "url": "ws://127.0.0.1:$PORT/ws"}],
 "cases": ["*"], "exclude-cases": ["12.*", "13.*"], "exclude-agent-cases": {}}
EOF
    "$WORK/srv" "$PORT" > /dev/null 2>&1 &
    SRV=$!
    sleep 0.3
    timeout 900 docker run --rm --network host -v "$(pwd)/$WORK/ab/config:/config" \
        -v "$(pwd)/$WORK/ab/reports:/reports" crossbario/autobahn-testsuite \
        wstest -m fuzzingclient -s /config/fuzzingclient.json > "$WORK/ab/run.log" 2>&1
    kill $SRV 2> /dev/null
    python3 - "$WORK/ab/reports/index.json" <<'EOF' || rc=1
import json, sys, collections
d = json.load(open(sys.argv[1]))["firn"]
c = collections.Counter(v["behavior"] for v in d.values())
cc = collections.Counter(v["behaviorClose"] for v in d.values())
bad = [k for k, v in d.items() if v["behavior"] not in ("OK", "INFORMATIONAL")
       or v["behaviorClose"] not in ("OK", "INFORMATIONAL")]
print("   Autobahn fuzzingclient: %d cases, %s; close %s" % (len(d), dict(c), dict(cc)))
if bad:
    print("  FAIL Autobahn:", sorted(bad)[:20])
    sys.exit(1)
print("AUTOBAHN OK: %d / %d strict" % (len(d) - len(bad), len(d)))
EOF
else
    echo "   Autobahn: SKIP (docker or the image crossbario/autobahn-testsuite missing)"
fi
exit $rc

#!/usr/bin/env bash
# tools/mc/online/vanilla.sh -- the online login against the REAL official Vanilla 1.20.1 server with
# online-mode=true (tools/mcclient/online_server.sh). Mojang's own authlib in the server is pointed at the
# local MOCK session server (-Dminecraft.api.env=custom -Dminecraft.api.session.host=...), so the server
# computes the server hash and verifies the RSA-encrypted secret / verify token with ITS code, while no
# real account is needed. Heavy (jar download, ~25 s start):   /root/jarvis/bin/heavy bash <abs path>/vanilla.sh
# Called by tools/mc/online/run.sh when MC_ONLINE_VANILLA=1. Prints `RESULT mc-online-vanilla: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
W=$(mktemp -d)
MOCK_PID=""
cleanup() {
    bash tools/mcclient/online_server.sh "$W/srv" stop >/dev/null 2>&1
    [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT
TOKEN="tok-$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
UUID="$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')"
NAME="VanillaTester"
export MOCK_TOKEN="$TOKEN" MOCK_UUID="$UUID" MOCK_NAME="$NAME"
umask 077
printf 'mc_token=%s\nuuid=%s\nname=%s\n' "$TOKEN" "$UUID" "$NAME" > "$W/acct"
$FIRNC -o "$W/demo" demos/mcclient/main.fi 2>"$W/err" || { echo "  FAIL  demo does not compile"; head -5 "$W/err"; echo "RESULT mc-online-vanilla: FAIL"; exit 1; }
node tools/mc/online/server.cjs 1.20.1 "$W/ready" sessiononly > "$W/ses.log" 2>&1 &
MOCK_PID=$!
for i in $(seq 1 50); do [ -s "$W/ready" ] && break; sleep 0.1; done
SPORT=$(sed 's/.*session=\([0-9]*\).*/\1/' "$W/ready")
PORT=$(node -e 'const s=require("net").createServer();s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})')
export ONLINE_PORT=$PORT
export ONLINE_JAVA_OPTS="-Dminecraft.api.env=custom -Dminecraft.api.session.host=http://127.0.0.1:$SPORT -Dminecraft.api.auth.host=http://127.0.0.1:1 -Dminecraft.api.account.host=http://127.0.0.1:1 -Dminecraft.api.services.host=http://127.0.0.1:1"
bash tools/mcclient/online_server.sh "$W/srv" start > "$W/start.out" 2>&1 || { echo "  FAIL  vanilla server did not start"; tail -5 "$W/start.out"; echo "RESULT mc-online-vanilla: FAIL"; exit 1; }
ERR=0
MC_ONLINE=1 MC_ACCOUNT_FILE="$W/acct" MC_SESSION_BASE="http://127.0.0.1:$SPORT" timeout 90 "$W/demo" 127.0.0.1 "$PORT" x "$W/d" 4 2500 763 > "$W/out" 2>"$W/err"; rc=$?
chunks=$(sed -n 's/^metric chunks=\([0-9]*\).*/\1/p' "$W/out"); wf=$(sed -n 's/^metric walk_fail=\([0-9]*\).*/\1/p' "$W/out"); cb=$(sed -n 's/^metric chunk_bad=\([0-9]*\).*/\1/p' "$W/out")
if [ $rc -eq 0 ] && [ "${chunks:-0}" -gt 0 ] && [ "${wf:-1}" = 0 ] && [ "${cb:-1}" = 0 ]; then
    echo "  ok    real Vanilla 1.20.1 (online-mode=true) accepted the online login: $chunks columns, walk_fail 0, chunk_bad 0"
else echo "  FAIL  vanilla online login (rc $rc)"; head -6 "$W/out"; ERR=1; fi
if grep -q 'SESSION hasJoined hit' "$W/ses.log"; then echo "  ok    the Vanilla server's own server hash == ours (hasJoined hit)"; else echo "  FAIL  no hasJoined hit"; ERR=1; fi
if grep -q "$NAME joined the game" "$W/srv/server.log"; then echo "  ok    server log: $NAME joined the game (UUID from the session profile)"; else echo "  FAIL  not joined"; ERR=1; fi
# wrong token: the session server refuses, the vanilla server is never asked
printf 'mc_token=wrong\nuuid=%s\nname=%s\n' "$UUID" "$NAME" > "$W/acct_bad"
MC_ONLINE=1 MC_ACCOUNT_FILE="$W/acct_bad" MC_SESSION_BASE="http://127.0.0.1:$SPORT" timeout 60 "$W/demo" 127.0.0.1 "$PORT" x "$W/d2" 4 2500 763 > "$W/out2" 2>&1; rc=$?
if [ $rc -eq 5 ] && grep -q 'http_status=403' "$W/out2"; then echo "  ok    wrong token against the vanilla setup: 403 named"; else echo "  FAIL  wrong token (rc $rc)"; head -4 "$W/out2"; ERR=1; fi
if grep -rqF -- "$TOKEN" "$W/out" "$W/err" "$W/out2" "$W/srv/server.log" "$W/ses.log" 2>/dev/null; then echo "  FAIL  token leaked"; ERR=1; else echo "  ok    token in no output / log"; fi
[ $ERR -eq 0 ] && echo "RESULT mc-online-vanilla: ok" || echo "RESULT mc-online-vanilla: FAIL"
exit $ERR

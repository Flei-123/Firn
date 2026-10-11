#!/usr/bin/env bash
# tools/mc/online/run.sh -- end-to-end test of the ONLINE login (lib/mc/mc_client.client_login_online +
# mc_crypto + mc_auth) with the demo client (demos/mcclient/main.fi, env MC_ONLINE=1) against
#   * a local ONLINE-MODE node-minecraft-protocol server (tools/mc/online/server.cjs) whose
#     yggdrasil.hasJoined goes to
#   * a local MOCK session server (same file, HTTP on 127.0.0.1).
# The server computes the server hash with ITS OWN implementation, so a pass proves: Encryption Request
# parsed, RSA-PKCS1 secret + verify token accepted (decrypted by node-rsa), session join with the right
# hash BEFORE the answer, CFB8 both ways, chunks decoded after that.
#   1  1.20.1 positive: login ok, chunks > 0, walk_fail 0, server saw the verified player
#   2  wrong token       -> 403 reported as a session-join error, the server never saw a player
#   3  wrong verify token (server expects another) -> "did not accept the encryption response"
#   4  join accepted but hash lost (dropjoin)      -> server kick text is shown
#   5  offline client vs online server -> CL_ONLINE_MODE text;  6 no account file;  7 non-https base URL;
#   8  session server unreachable -> network error
#   9  login-only on more versions (1.8.9 1.12.2 1.16.5 1.20.2 1.21.4: incl. configuration phase and
#      the 1.20.5+ shouldAuthenticate flag where the node server speaks it)
#   10 the (random) test token appears in no output of any run
#   11 (MC_ONLINE_VANILLA=1) the REAL Vanilla 1.20.1 server in online mode, session host redirected to the mock
#      with -Dminecraft.api.session.host: tools/mc/online/vanilla.sh
# NOT covered: the real sessionserver.mojang.com / a real Microsoft account (see tools/mcclient/online_server.sh).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
W=$(mktemp -d)
SRV_PID=""
cleanup() { [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
ERR=0
ok()   { echo "  ok    $*"; }
fail() { echo "  FAIL  $*"; ERR=1; }

TOKEN="tok-$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
UUID="$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')"
NAME="OnlineTester"
export MOCK_TOKEN="$TOKEN" MOCK_UUID="$UUID" MOCK_NAME="$NAME"
umask 077
printf 'mc_token=%s\nuuid=%s\nname=%s\n' "$TOKEN" "$UUID" "$NAME" > "$W/acct"
printf 'mc_token=%s\nuuid=%s\nname=%s\n' "wrong-$TOKEN" "$UUID" "$NAME" > "$W/acct_bad"
chmod 600 "$W/acct" "$W/acct_bad"

if ! $FIRNC -o "$W/demo" demos/mcclient/main.fi 2>"$W/err"; then fail "demo client does not compile"; head -10 "$W/err"; echo "RESULT mc-online: FAIL"; exit 1; fi

# start_server <version> <mode>  -> sets PORT, SPORT
start_server() {
    rm -f "$W/ready"; : > "$W/server.log"
    node tools/mc/online/server.cjs "$1" "$W/ready" "${2:-normal}" > "$W/server.log" 2>&1 &
    SRV_PID=$!
    for i in $(seq 1 100); do [ -s "$W/ready" ] && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 0.1; done
    [ -s "$W/ready" ] || { echo "server did not start:"; head -5 "$W/server.log"; return 1; }
    PORT=$(sed 's/.*port=\([0-9]*\) .*/\1/' "$W/ready"); SPORT=$(sed 's/.*session=\([0-9]*\).*/\1/' "$W/ready")
}
stop_server() { [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; wait "$SRV_PID" 2>/dev/null; SRV_PID=""; }
# run_client <label> <protocol> <acct|-> [extra env...]
run_client() {
    local label=$1 proto=$2 acct=$3; shift 3
    if [ "$acct" = "-" ]; then
        env "$@" timeout 60 "$W/demo" 127.0.0.1 "$PORT" Offliner "$W/$label.dump" 3 1500 "$proto" > "$W/$label.out" 2> "$W/$label.err"
    else
        env MC_ONLINE=1 MC_ACCOUNT_FILE="$acct" MC_SESSION_BASE="http://127.0.0.1:$SPORT" "$@" timeout 60 "$W/demo" 127.0.0.1 "$PORT" ignored "$W/$label.dump" 3 1500 "$proto" > "$W/$label.out" 2> "$W/$label.err"
    fi
    RC=$?
}
metric() { sed -n "s/^metric $2=\([-0-9]*\).*/\1/p" "$W/$1.out" | head -1; }

# ---- 1. positive, 1.20.1
start_server 1.20.1 normal || { echo "RESULT mc-online: FAIL"; exit 1; }
run_client pos 763 "$W/acct"
chunks=$(metric pos chunks); wf=$(metric pos walk_fail)
[ $RC -eq 0 ] && ok "1.20.1 online login: rc 0" || { fail "1.20.1 online login rc=$RC"; head -5 "$W/pos.out"; }
[ "${chunks:-0}" -gt 0 ] 2>/dev/null && [ "${wf:-1}" = 0 ] && ok "chunks decoded after encryption started: $chunks columns, walk_fail 0" || fail "no chunks after login (chunks=${chunks:-?} walk_fail=${wf:-?})"
grep -q 'SESSION join accepted' "$W/server.log" && grep -q 'SESSION hasJoined hit' "$W/server.log" && ok "session join (our hash) == hash the server computed itself (hasJoined hit)" || { fail "hash mismatch / no hasJoined hit"; cat "$W/server.log" | head; }
grep -q "JOIN-OK $NAME uuid_ok=true" "$W/server.log" && ok "server verified the player (name + uuid from the session profile)" || fail "server did not log the verified player"
sleep 0.3
# ---- 2. wrong token
run_client badtok 763 "$W/acct_bad"
if [ $RC -eq 5 ] && grep -q 'session server refused the join' "$W/badtok.out" && grep -q 'http_status=403' "$W/badtok.out" && grep -q 'forbidden' "$W/badtok.out" && grep -q 'ForbiddenOperationException' "$W/badtok.out"; then
    ok "wrong access token: join 403 reported (step, status, session error)"
else fail "wrong token not reported clearly (rc $RC)"; cat "$W/badtok.out" | head -6; fi
[ "$(grep -c 'JOIN-OK' "$W/server.log")" = 1 ] && ok "server saw no second player after the refused join" || fail "server accepted a player with a wrong token"
stop_server

# ---- 3. wrong verify token
start_server 1.20.1 badtoken
run_client badvt 763 "$W/acct"
if [ $RC -eq 5 ] && grep -q 'did not accept the encryption response' "$W/badvt.out"; then ok "server rejects verify token -> 'did not accept the encryption response'"
else fail "bad verify token not reported (rc $RC)"; head -6 "$W/badvt.out"; fi
stop_server

# ---- 4. join accepted but hash not stored: server kicks
start_server 1.20.1 dropjoin
run_client drop 763 "$W/acct"
if [ $RC -eq 5 ] && grep -q 'disconnected by the server' "$W/drop.out" && grep -q 'Failed to verify username' "$W/drop.out"; then ok "server kick text shown: Failed to verify username!"
else fail "kick text not shown (rc $RC)"; head -6 "$W/drop.out"; fi

# ---- 5. offline client against the online server
run_client offl 763 -
if [ $RC -eq 5 ] && grep -q 'server wants encryption (online mode)' "$W/offl.out"; then ok "offline client: clear CL_ONLINE_MODE message"; else fail "offline client message (rc $RC)"; head -4 "$W/offl.out"; fi
# ---- 6. no account file / missing lines
run_client noacct 763 "$W/does-not-exist"
if [ $RC -eq 8 ] && grep -q 'cannot read MC_ACCOUNT_FILE' "$W/noacct.out"; then ok "missing account file: clear setup error"; else fail "missing account file (rc $RC)"; head -3 "$W/noacct.out"; fi
printf 'mc_token=abc\nname=x\n' > "$W/acct_nouuid"
run_client nouuid 763 "$W/acct_nouuid"
if [ $RC -eq 8 ] && grep -q 'no valid uuid line' "$W/nouuid.out"; then ok "account file without uuid: clear setup error"; else fail "account file without uuid (rc $RC)"; head -3 "$W/nouuid.out"; fi
# ---- 7. non-https, non-loopback base URL is refused before anything is sent
run_client badbase 763 "$W/acct" MC_SESSION_BASE=http://example.invalid
if [ $RC -eq 5 ] && grep -q 'bad-base-url' "$W/badbase.out"; then ok "http:// to a non-loopback session server refused (bad-base-url)"; else fail "base URL check (rc $RC)"; head -4 "$W/badbase.out"; fi
stop_server
# ---- 8. session server unreachable
start_server 1.20.1 normal
run_client noses 763 "$W/acct" MC_SESSION_BASE=http://127.0.0.1:1
if [ $RC -eq 5 ] && grep -q 'session_join result=network' "$W/noses.out"; then ok "session server unreachable: network error named"; else fail "unreachable session server (rc $RC)"; head -4 "$W/noses.out"; fi
stop_server

# ---- 9. other versions: login only (incl. configuration phase / shouldAuthenticate)
for spec in "1.8.9:47" "1.12.2:340" "1.16.5:754" "1.20.2:764" "1.21.4:769"; do
    v=${spec%%:*}; p=${spec##*:}
    if ! start_server "$v" normal; then fail "$v: server did not start"; stop_server; continue; fi
    run_client "v$p" "$p" "$W/acct"
    if grep -q "JOIN-OK $NAME uuid_ok=true" "$W/server.log" && ! grep -q 'login failed' "$W/v$p.out"; then ok "$v (protocol $p): online login + encryption, server verified the player"
    else fail "$v: online login failed (rc $RC)"; head -4 "$W/v$p.out"; head -4 "$W/server.log"; fi
    stop_server
done

# ---- 10. the token never shows up
if grep -rqF -- "$TOKEN" "$W"/*.out "$W"/*.err "$W"/server.log 2>/dev/null; then fail "the test token leaked into an output"; else ok "the token appears in no client output, error output or server log"; fi

# ---- 11. optional: the REAL Vanilla 1.20.1 server in online mode (heavy; MC_ONLINE_VANILLA=1)
if [ "${MC_ONLINE_VANILLA:-0}" = 1 ]; then
    bash tools/mc/online/vanilla.sh > "$W/vanilla.out" 2>&1 && ok "real Vanilla 1.20.1 online-mode server (vanilla.sh)" || { fail "vanilla.sh"; tail -8 "$W/vanilla.out"; }
fi

[ $ERR -eq 0 ] && echo "RESULT mc-online: ok" || echo "RESULT mc-online: FAIL"
exit $ERR

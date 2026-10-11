#!/usr/bin/env bash
# tools/mc/net/run.sh -- mc_net (connect with retry, server list ping) against an independent python
# status server (own VarInt / framing / JSON):
#   * a plain MOTD string and a component MOTD with colour + bold + extra  -> protocol, name, players,
#     MOTD as plain text, round trip time;
#   * the server closes right after the handshake -> NS_CLOSED; sends broken JSON -> NS_BAD_REPLY;
#     never answers -> NS_TIMEOUT within the wait budget; nothing listens -> NS_CONNECT after the retries;
#   * retry: the server starts listening 900 ms AFTER the client began -> the retry with back-off connects.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
$FIRNC -o "$W/prog" tools/mc/net/prog.fi 2>"$W/err" || { echo "  FAIL  compile"; head -5 "$W/err"; exit 1; }
run() { # scenario attempts waits
    rm -f "$W/port"
    python3 -I tools/mc/net/mock.py "$1" "$W/port" 2>"$W/mock.err" &
    local mp=$!
    for i in $(seq 1 50); do [ -s "$W/port" ] && break; sleep 0.1; done
    "$W/prog" status "$(cat "$W/port")" "$2" "$3" > "$W/out.txt" 2>&1
    wait $mp 2>/dev/null
}
run ok-string 1 20
grep -q '^rc=0' "$W/out.txt" && grep -q 'protocol=763 online=3 max=20 latency_ok=true' "$W/out.txt" && grep -q '^name=1.20.1$' "$W/out.txt" && grep -q '^motd=A plain MOTD$' "$W/out.txt" \
    && echo "  ok    string MOTD: protocol 763, '1.20.1', 3/20 players, MOTD, latency measured" || { echo "  FAIL  string MOTD"; cat "$W/out.txt"; ERR=1; }
run ok-component 1 20
grep -q '^rc=0' "$W/out.txt" && grep -q '^motd=Hello World!$' "$W/out.txt" \
    && echo "  ok    component MOTD (colour + bold + extra) flattened to 'Hello World!'" || { echo "  FAIL  component MOTD"; cat "$W/out.txt"; ERR=1; }
run closed 1 20
grep -q '^rc=-5' "$W/out.txt" && echo "  ok    server closes after the handshake -> NS_CLOSED" || { echo "  FAIL  closed"; cat "$W/out.txt"; ERR=1; }
run garbage 1 20
grep -q '^rc=-4' "$W/out.txt" && echo "  ok    broken JSON answer -> NS_BAD_REPLY" || { echo "  FAIL  garbage"; cat "$W/out.txt"; ERR=1; }
run silent 1 5
grep -q '^rc=-3' "$W/out.txt" && echo "  ok    silent server -> NS_TIMEOUT ($(grep -o 'elapsed_ms=[0-9]*' "$W/out.txt"))" || { echo "  FAIL  silent"; cat "$W/out.txt"; ERR=1; }
# nothing listens: 3 attempts, back-off 250 + 500 ms
P=$(python3 -I -c "import socket;s=socket.socket();s.bind(('127.0.0.1',0));print(s.getsockname()[1]);s.close()")
"$W/prog" status "$P" 3 5 > "$W/out.txt" 2>&1
ms=$(grep -o 'elapsed_ms=[0-9]*' "$W/out.txt" | cut -d= -f2)
if grep -q '^rc=-1' "$W/out.txt" && [ "${ms:-0}" -ge 700 ]; then echo "  ok    nothing listens -> NS_CONNECT after 3 attempts with back-off (${ms} ms)"; else echo "  FAIL  connect refused case"; cat "$W/out.txt"; ERR=1; fi
# retry reaches a server that starts late
rm -f "$W/port"
python3 -I tools/mc/net/mock.py late-listen "$W/port" 2>"$W/mock.err" &
mp=$!
for i in $(seq 1 50); do [ -s "$W/port" ] && break; sleep 0.1; done
"$W/prog" status "$(cat "$W/port")" 8 20 > "$W/out.txt" 2>&1
wait $mp 2>/dev/null
grep -q '^rc=0' "$W/out.txt" && echo "  ok    retry with back-off reaches a server that starts listening 900 ms late ($(grep -o 'elapsed_ms=[0-9]*' "$W/out.txt"))" || { echo "  FAIL  late listen"; cat "$W/out.txt"; ERR=1; }
[ $ERR -eq 0 ] && echo "RESULT mc-net: ok" || echo "RESULT mc-net: FAIL"
exit $ERR

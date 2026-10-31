#!/usr/bin/env bash
# tools/mc/dedi/egg_run.sh -- egg check + container-like run: installer against a local HTTP server, Wings-style properties edit,
# start with a clean environment and a stdin pipe, Done line, status ping, `stop` -> exit 0. Env: FIRNC.
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$REPO/lib"
EGG=deploy/pterodactyl/egg-firn-minecraft-server.json
W=$(mktemp -d)
PIDS=""
trap 'for p in $PIDS; do kill -9 $p 2>/dev/null; done; rm -rf "$W"' EXIT
ERR=0
ok() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1"; ERR=$((ERR+1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

python3 -I tools/mc/dedi/make_egg.py >/dev/null && git diff --quiet -- $EGG 2>/dev/null; true
python3 -I tools/mc/dedi/check_egg.py $EGG || ERR=$((ERR+1))

mkdir -p "$W/rel"
$FIRNC --opt-level=release-fast -o "$W/rel/firnmc-server-linux-x86_64" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  release build"; head -5 "$W/err"; exit 1; }
(cd "$W/rel" && sha256sum firnmc-server-linux-x86_64 > SHA256SUMS)
RPORT=$(free_port)
(cd "$W/rel" && setsid nohup python3 -I -m http.server $RPORT --bind 127.0.0.1 >/dev/null 2>&1 & echo $! > "$W/http.pid")
PIDS="$PIDS $(cat $W/http.pid)"
sleep 0.8

python3 -I - "$EGG" "$W/install.sh" <<'PY'
import json, sys
egg = json.load(open(sys.argv[1]))
open(sys.argv[2], "w").write(egg["scripts"]["installation"]["script"])
PY
SRV="$W/server"
INSTALL_DIR="$SRV" FIRN_RELEASE_URL="http://127.0.0.1:$RPORT" sh "$W/install.sh" > "$W/install.log" 2>&1
check "installer downloads and verifies" "grep -q 'Checksum ok' $W/install.log && [ -x $SRV/firnmc-server ]"
echo "deadbeef  firnmc-server-linux-x86_64" > "$W/rel/SHA256SUMS"
INSTALL_DIR="$W/bad" FIRN_RELEASE_URL="http://127.0.0.1:$RPORT" sh "$W/install.sh" > "$W/install2.log" 2>&1
check "installer rejects a wrong checksum" "grep -q 'Checksum mismatch' $W/install2.log && [ ! -e $W/bad/firnmc-server ]"
INSTALL_DIR="$W/none" FIRN_RELEASE_URL="http://127.0.0.1:1" sh "$W/install.sh" > "$W/install3.log" 2>&1
check "installer fails on unreachable URL" "[ \$? -ne 0 ] || ! [ -e $W/none/firnmc-server ]"

PORT=$(free_port)
python3 -I - "$EGG" "$SRV/server.properties" "$PORT" <<'PY'
import json, sys
egg = json.load(open(sys.argv[1]))
env = {v["env_variable"]: v["default_value"] for v in egg["variables"]}
env["SERVER_MOTD"] = "Egg Test Server"
env["MAX_PLAYERS"] = "5"
find = json.loads(egg["config"]["files"])["server.properties"]["find"]
lines = []
for k, val in find.items():
    val = val.replace("{{server.build.default.port}}", sys.argv[3])
    for name, value in env.items():
        val = val.replace("{{server.build.env.%s}}" % name, value)
    lines.append("%s=%s" % (k, val))
open(sys.argv[2], "w").write("\n".join(lines) + "\n")
PY
mkfifo "$SRV/in.fifo"
(setsid nohup sleep 300 > "$SRV/in.fifo" 2>/dev/null & echo $! > "$W/hold.pid")
PIDS="$PIDS $(cat $W/hold.pid)"
(cd "$SRV" && setsid nohup env -i PATH=/usr/bin:/bin SERVER_PORT=$PORT bash -c "./firnmc-server nogui --port $PORT; echo \$? > exit.code" < in.fifo > out.log 2>&1 &)
for _ in $(seq 1 60); do grep -q 'Done (' "$SRV/out.log" 2>/dev/null && break; sleep 0.1; done
check "startup Done line matches the egg done string" "grep -q 'Done (' $SRV/out.log"
PING=$(node tools/mc/dedi/probe.cjs ping 127.0.0.1 $PORT)
check "motd and max players from the egg variables" "echo '$PING' | grep -q 'Egg Test Server' && echo '$PING' | grep -q '\"max\":5'"
echo stop > "$SRV/in.fifo"
for _ in $(seq 1 60); do [ -f "$SRV/exit.code" ] && break; sleep 0.1; done
check "stop exits 0" "[ \"\$(cat $SRV/exit.code 2>/dev/null)\" = 0 ]"
check "latest.log exists" "[ -s $SRV/logs/latest.log ]"
echo
if [ $ERR -eq 0 ]; then echo "egg: all ok"; else echo "egg: $ERR FAILED"; fi
exit $ERR

#!/usr/bin/env bash
# online_server.sh <dir> [start|stop|status|log]    (default: start)
#
# README -- LOCAL REAL VANILLA 1.20.1 SERVER WITH online-mode=true, for the LIVE test of the online login
# (lib/mc/mc_client.client_login_online) with a REAL Minecraft account. Not used by tools/mc/run.sh.
#
# What it does: downloads the official server jar (tools/mcclient/fetch_server.sh, SHA-1 verified, optional
# cache MCCLIENT_JAR_CACHE), writes eula.txt + server.properties and starts the server in the BACKGROUND
# (nohup, Java 17 is enough) on 127.0.0.1:25599 (ONLINE_PORT / ONLINE_BIND override), log in <dir>/server.log,
# pid in <dir>/server.pid. Properties: online-mode=true, white-list=false (anybody who authenticates may join),
# enforce-secure-profile=false (the Firn client sends no chat signing key), view-distance 4, peaceful, level seed
# 4242, "free" world in <dir>/world (delete it to get a new one). `stop` sends SIGTERM and waits, `status`
# prints running/not running + the "Done" line, `log` tails the log.
#
# ONLINE_JAVA_OPTS: extra JVM options. To test against a LOCAL MOCK session server instead of Mojang (what
# tools/mc/online/vanilla.sh does): ONLINE_JAVA_OPTS='-Dminecraft.api.env=custom -Dminecraft.api.session.host=http://127.0.0.1:<port>
# -Dminecraft.api.auth.host=http://127.0.0.1:1 -Dminecraft.api.account.host=http://127.0.0.1:1 -Dminecraft.api.services.host=http://127.0.0.1:1'.
#
# EULA: eula=true is written; the owner ordered running a throw-away local test instance (same as run.sh).
#
# The server itself asks https://sessionserver.mojang.com (hasJoined) -> this machine needs outgoing HTTPS
# (checked: `curl https://sessionserver.mojang.com` answers; HTTP 204 for an unknown player is the normal answer).
#
# LIVE TEST with the owner's account (nothing of it is in the repo; the token only lives in the file):
#   1. printf 'mc_token=%s\nuuid=%s\nname=%s\n' "$TOKEN" "$UUID32HEX" "$NAME" > ~/mc-account; chmod 600 ~/mc-account
#   2. bash tools/mcclient/online_server.sh /root/mc-online-srv start        # wait for 'Done' in server.log
#   3. firnc -o mcclient demos/mcclient/main.fi   (FIRNLIB=lib)
#      MC_ONLINE=1 MC_ACCOUNT_FILE=~/mc-account ./mcclient 127.0.0.1 25599 x /root/out.dump 4 2500 763
#      (the name argument is ignored in online mode: the account file's name= is used; no MC_SESSION_BASE ->
#       the real sessionserver.mojang.com with the system CA store)
#   4. expected: 'metric chunks=...' > 0; server.log shows '<name> joined the game' with the account's real UUID.
#      On failure the client names the step: session join (http status + Mojang's error text), RSA, encryption
#      rejected, or the server's disconnect text.
#   5. bash tools/mcclient/online_server.sh /root/mc-online-srv stop
set -euo pipefail
DIR="${1:?usage: online_server.sh <dir> [start|stop|status|log]}"
CMD="${2:-start}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JAVA="${JAVA:-/usr/bin/java}"
PORT="${ONLINE_PORT:-25599}"
BIND="${ONLINE_BIND:-127.0.0.1}"
MCV="${MC_VERSION:-1.20.1}"
mkdir -p "$DIR"
DIR="$(cd "$DIR" && pwd)"
PIDF="$DIR/server.pid"

running() { [ -f "$PIDF" ] && kill -0 "$(cat "$PIDF")" 2>/dev/null; }

case "$CMD" in
  start)
    if running; then echo "online_server: already running (pid $(cat "$PIDF"), port $PORT)"; exit 0; fi
    bash "$HERE/fetch_server.sh" "$DIR" "$MCV"
    echo "eula=true" > "$DIR/eula.txt"
    cat > "$DIR/server.properties" <<PROPS
server-ip=$BIND
server-port=$PORT
online-mode=true
white-list=false
enforce-whitelist=false
enforce-secure-profile=false
level-seed=4242
view-distance=4
simulation-distance=4
spawn-protection=0
max-players=4
difficulty=peaceful
network-compression-threshold=256
enable-rcon=false
enable-query=false
player-idle-timeout=0
motd=Firn online-mode test server
PROPS
    case "$MCV" in 1.8*) echo "use-native-transport=false" >> "$DIR/server.properties" ;; esac
    ( cd "$DIR" && nohup "$JAVA" -Xmx1G -Xms256M ${ONLINE_JAVA_OPTS:-} -jar server.jar nogui < /dev/null > "$DIR/server.log" 2>&1 & echo $! > "$PIDF" )
    for i in $(seq 1 180); do
      grep -q 'Done (' "$DIR/server.log" 2>/dev/null && break
      running || { tail -20 "$DIR/server.log" >&2; echo "online_server: server died during start" >&2; exit 2; }
      sleep 1
    done
    grep -q 'Done (' "$DIR/server.log" || { echo "online_server: start timeout (180 s)" >&2; exit 2; }
    grep -m1 'Done (' "$DIR/server.log"
    echo "online_server: $MCV online-mode=true listening on $BIND:$PORT (pid $(cat "$PIDF"), log $DIR/server.log)"
    ;;
  stop)
    if running; then
      kill "$(cat "$PIDF")"
      for i in $(seq 1 60); do running || break; sleep 0.5; done
      running && { kill -9 "$(cat "$PIDF")"; echo "online_server: had to kill -9"; }
      rm -f "$PIDF"; echo "online_server: stopped"
    else echo "online_server: not running"; rm -f "$PIDF"; fi
    ;;
  status)
    if running; then echo "running pid $(cat "$PIDF") port $PORT"; grep -m1 'Done (' "$DIR/server.log" || true; else echo "not running"; fi
    ;;
  log) tail -n 50 "$DIR/server.log" ;;
  *) echo "usage: online_server.sh <dir> [start|stop|status|log]" >&2; exit 2 ;;
esac

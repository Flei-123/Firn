#!/usr/bin/env bash
# tools/mcclient/probe_tp.sh -- which server action makes the REAL server send entity_teleport (0x77 on 1.21.4) / sync_entity_position?
# Starts the official server (MC_VERSION, default 1.21.4, flat world), two passive probe clients (Obs = observer, Rid = rider/target),
# plays a list of scenarios (players tp'd, boat/minecart/horse with a player rider, enderman/shulker hurt, rotation, far tp, pearls ...)
# and prints, per scenario, every teleport/sync/vehicle packet the clients logged (first bytes of the body) plus the packet counts.
# Env: MC_VERSION, JAVA, PROBE_KEEP_DIR (copy of server.log + probe logs), PROBE_SCENARIOS (";"-separated own server commands run after the built-ins)
# Start via heavy (Java server). The jar is only downloaded + started, never opened.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
MCV=${MC_VERSION:-1.21.4}
# protocol + nmp name from versions.json; versions missing there (e.g. 1.21.3) need MC_NMP + MC_PROTOCOL in the environment
if [ -z "${MC_PROTOCOL:-}" ]; then
    read -r PROTO NMP < <(node -e '
      const v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).versions[process.argv[2]];
      if (!v) process.exit(1);
      console.log(v.protocol + " " + (v.nmp || process.argv[2]));
    ' "$HERE/../mcgen/versions.json" "$MCV") || { echo "probe_tp: $MCV not in versions.json (set MC_NMP + MC_PROTOCOL)"; exit 2; }
else
    PROTO=$MC_PROTOCOL; NMP=${MC_NMP:-$MCV}
fi
export MC_NMP=$NMP MC_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; P1=""; P2=""
cleanup() {
    [ -n "${PROBE_KEEP_DIR:-}" ] && { mkdir -p "$PROBE_KEEP_DIR"; cp "$W"/*.log "$W"/*.txt "$PROBE_KEEP_DIR"/ 2>/dev/null; }
    exec 3>&- 4>&- 5>&- 2>/dev/null
    [ -n "$P1" ] && kill "$P1" 2>/dev/null; [ -n "$P2" ] && kill "$P2" 2>/dev/null
    [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT
mkdir -p "$W/srv"
bash "$HERE/fetch_server.sh" "$W/srv" "$MCV" || exit 2
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-type=minecraft\:flat
view-distance=4
simulation-distance=4
spawn-protection=0
difficulty=easy
spawn-monsters=true
spawn-animals=true
enable-rcon=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; echo "probe_tp: server did not start"; exit 2; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false" "mobGriefing false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
BOAT=oak_boat; case "$MCV" in 1.21.1|1.20*|1.1*) BOAT=boat ;; esac   # boats got one entity type per wood in 1.21.2
mkfifo "$W/in_obs" "$W/in_rid"; exec 4<>"$W/in_obs"; exec 5<>"$W/in_rid"
node "$HERE/probe_tp.cjs" 127.0.0.1 "$PORT" Obs "$W/obs.txt" < "$W/in_obs" > "$W/obs.out" 2> "$W/obs.err" & P1=$!
sleep 4
node "$HERE/probe_tp.cjs" 127.0.0.1 "$PORT" Rid "$W/rid.txt" < "$W/in_rid" > "$W/rid.out" 2> "$W/rid.err" & P2=$!
for i in $(seq 1 60); do grep -q 'Rid joined' "$W/server.log" && break; sleep 1; done
sleep 4
S() { echo "$*" >&3; sleep "${SLEEP:-1.2}"; }
mark() { echo "== $1"; echo "MARK $1" >> "$W/obs.txt"; echo "MARK $1" >> "$W/rid.txt"; }
R="execute as Rid at @s run"
S "tp Obs 0 -60 0 0 0"; S "tp Rid 3 -60 3 0 0"
mark "player_tp_abs";   S "tp Rid 20 -60 20"; S "tp Rid 3 -60 3"
mark "player_tp_rel_rot"; S "tp Rid ~2 ~ ~ 90 20"
mark "player_tp_far";   S "tp Rid 300 -60 300"; sleep 3; S "tp Rid 3 -60 3"; sleep 2
mark "boat_ride"
S "$R summon $BOAT ~ ~ ~"; S "ride Rid mount @e[type=$BOAT,limit=1]"
mark "boat_tp_with_rider"; S "tp @e[type=$BOAT] 8 -60 8"; S "tp @e[type=$BOAT] ~ ~ ~ 90 0"; S "tp @e[type=$BOAT] 40 -60 40"
mark "boat_tp_rot"; S "tp @e[type=$BOAT] 20 -60 20 90 30"; S "tp @e[type=$BOAT] 22 -60 20 180 -45"; S "rotate @e[type=$BOAT] 135 10"; S "tp @e[type=$BOAT] ~1 ~ ~ facing 0 -60 0"
mark "boat_dismount"; S "ride Rid dismount"; S "tp Rid 3 -60 3"; S "kill @e[type=$BOAT]"
mark "minecart_ride"
S "$R summon minecart ~ ~ ~"; S "ride Rid mount @e[type=minecart,limit=1]"
mark "minecart_tp_with_rider"; S "tp @e[type=minecart] 8 -60 8"; S "tp @e[type=minecart] ~ ~ ~ 180 0"
S "ride Rid dismount"; S "kill @e[type=minecart]"
mark "horse_ride"
S "$R summon horse ~ ~ ~ {Tame:1b,NoAI:1b}"; S "ride Rid mount @e[type=horse,limit=1]"
mark "horse_tp_with_rider"; S "tp @e[type=horse] 8 -60 8"; S "tp @e[type=horse] ~ ~ ~ 45 10"
S "ride Rid dismount"; S "kill @e[type=horse]"
mark "mob_rides_mob"
S "$R summon pig ~ ~ ~ {NoAI:1b,Tags:[\"vp\"]}"; S "$R summon chicken ~ ~ ~ {NoAI:1b,Tags:[\"vc\"]}"; S "ride @e[tag=vc,limit=1] mount @e[tag=vp,limit=1]"
S "tp @e[tag=vp] ~ ~ ~ 60 0"; S "tp @e[tag=vp] 60 -60 60"; S "kill @e[tag=vp]"; S "kill @e[tag=vc]"
mark "enderman_hurt"
S "$R summon enderman ~3 ~ ~ {Tags:[\"em\"]}"; for i in 1 2 3 4 5 6; do S "damage @e[tag=em] 1"; done; sleep 2; S "kill @e[tag=em]"
mark "shulker_hurt"
S "$R summon shulker ~3 ~ ~ {Tags:[\"sh\"],NoAI:0b}"; for i in 1 2 3 4 5 6; do S "damage @e[tag=sh] 1"; done; sleep 2; S "kill @e[tag=sh]"
mark "pearl"
S "$R summon ender_pearl ~ ~2 ~ {Motion:[0.5d,0.1d,0.5d]}"; sleep 4
mark "knockback_pig"
S "$R summon pig ~2 ~ ~ {Tags:[\"kb\"]}"; S "damage @e[tag=kb] 1 minecraft:player_attack by Rid"; sleep 2; S "kill @e[tag=kb]"
mark "armorstand_tp_rot"
S "$R summon armor_stand ~2 ~ ~ {Tags:[\"as\"]}"; S "tp @e[tag=as] ~ ~ ~ 33 7"; S "tp @e[tag=as] 90 -60 90"; S "kill @e[tag=as]"
mark "item_frame_item"
S "$R summon item ~ ~ ~ {Item:{id:\"minecraft:stone\",count:1}}"; sleep 2; S "kill @e[type=item]"
if [ -n "${PROBE_SCENARIOS:-}" ]; then
    mark "custom"; IFS=';' read -ra EX <<< "$PROBE_SCENARIOS"; for c in "${EX[@]}"; do S "$c"; done
fi
sleep 2
echo quit >&4; echo quit >&5; sleep 2
echo "== server: movement complaints"; grep -E "moved too quickly|moved wrongly|Exception" "$W/server.log" | head -3
for f in obs rid; do
    echo "######## $f: logged packets (id from the first varint of the body)"
    python3 -I - "$W/$f.txt" <<'PY'
import sys, re
for l in open(sys.argv[1]):
    l = l.rstrip("\n")
    if l.startswith("MARK") or l.startswith("count "):
        print(l); continue
    if " nmp_entity_teleport " in l:
        print("   nmp: " + l.split(" ", 2)[2][:200]); continue
    m = re.match(r"(\d+) (\S+) len=(\d+) hex=(\w+)", l)
    if m:
        h = m.group(4)
        print("   %-22s len=%s id=0x%s body=%s" % (m.group(2), m.group(3), h[:2], h[:120]))
PY
done

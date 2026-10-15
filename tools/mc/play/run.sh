#!/usr/bin/env bash
# tools/mc/play/run.sh -- demos/mcplay end to end: Xvfb + official 1.20.1 server (creative, offline mode) + mcplay + xdotool.
# Flies/walks ~8 s (W + mouse look), clicks (break + place), quits by MCPLAY_RUN_S, writes a screenshot PNG + hash.
# Needs Xvfb, xdotool, java, network for the server jar (tools/mcclient/fetch_server.sh). Prints `RESULT mc-play: ok`.
# Env: MCPLAY_PNG_OUT (copy target of the screenshot), MC_VIEW (4), FIRNC, MCPLAY_KEEP_DIR (keep wavs/outs/pngs), MCPLAY_SKIP_GL/AUDIO/PSNR/SURVIVAL=1. Start it via heavy in a full run (java server).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PA_PID=""; REC_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.wav "$W"/*.out "$W"/*.png "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$REC_PID" ] && kill "$REC_PID" 2>/dev/null; [ -n "$PA_PID" ] && kill "$PA_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play: FAIL"; exit 1; }
for t in Xvfb xdotool java; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
# sound bank (cached: tools/sounds/gen is deterministic); FIRN_AUDIO=file writes the mix to a WAV
# keyed by the recipe hash: a changed tools/sounds/mc.json regenerates the bank (the plain cache dir was never refreshed)
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds-$(sha256sum tools/sounds/mc.json | cut -c1-10)}
if [ ! -s "$SND/bank.json" ]; then
    $FIRNC -o "$W/sndgen" tools/sounds/gen/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "tools/sounds/gen does not compile"; }
    mkdir -p "$SND"; "$W/sndgen" tools/sounds/mc.json "$SND" >/dev/null 2>&1 || fail "sound bank generation"
fi
export MCPLAY_SOUNDS="$SND"
MCV=1.20.1; VIEW=${MC_VIEW:-4}
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
for gr in "doDaylightCycle false" "doMobSpawning false" "doWeatherCycle false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((90 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
# r223: the script waits on the client's SIM time (`progress` lines: ticks, pos, ground, walked_mm), never on the wall clock -- under load (fps 4) a
# fixed sleep walked nothing / acted before the join. The client ends when the script is done (QUIT_FILE); MCPLAY_RUN_S is only the safety net.
FIRN_AUDIO="file:$W/day.wav" MCPLAY_AUDIO_LOG=1 MCPLAY_PROGRESS=1 MCPLAY_QUIT_FILE="$W/quit" MCPLAY_GIVE="iron_pickaxe:1,stone:64" MCPLAY_RUN_S=${MCPLAY_DAY_MAX_S:-420} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/shot.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Player > "$W/play.out" 2>&1 &
PP=$!
# last `progress` field NAME of the day client (empty until the first line)
wj() { local i; for i in $(seq 1 1200); do grep -q "$1 joined the game" "$W/server.log" 2>/dev/null && return 0; sleep 0.1; done; echo "  WARN  $1 did not join within 120 s"; return 1; }   # r223: wait for the join, not a blind sleep
prog() { grep -o "progress .*" "$W/play.out" 2>/dev/null | tail -1 | grep -o " $1=[^ ]*" | cut -d= -f2; }
# wait (wall limit $2 s, default 120) until the client simulated $1 more ticks
wait_ticks() { local t0 t; t0=$(prog ticks); t0=${t0:-0}; for i in $(seq 1 $(( ${2:-120} * 10 ))); do t=$(prog ticks); [ "${t:-0}" -ge $(( t0 + $1 )) ] && return 0; kill -0 "$PP" 2>/dev/null || return 1; sleep 0.1; done; echo "  WARN  wait_ticks $1 timed out"; return 1; }
for i in $(seq 1 1200); do grep -q "Player joined the game" "$W/server.log" 2>/dev/null && break; sleep 0.1; done
for i in $(seq 1 1200); do [ "$(prog pos)" = 1 ] || [ "$(prog pos)" = true ] && [ "$(prog chunks)" -gt 10 ] 2>/dev/null && break; sleep 0.1; done
# flat stone platform high above the terrain (2 layers, air above): the walk is never blocked by a hill/tree and the place target is a floor block well in
# front of the player (a block placed inside the player's own hitbox is refused by the server)
for cmd in "fill -20 119 -20 20 120 20 stone" "fill -20 121 -20 20 128 20 air" "tp Player 0.5 121 0.5"; do echo "$cmd" >&3; done
# standing on the platform (y 121.000, on ground) before anything else happens
for i in $(seq 1 600); do [ "$(prog ground)" != 0 ] && [ "$(prog ground)" != false ] && [ "$(prog y_mm)" = 121000 ] && break; sleep 0.1; done
wait_ticks 5
xdotool mousemove 320 180 2>/dev/null
xdotool mousemove_relative -- 40 5; wait_ticks 6
# walk 60 SIM ticks (3 s of game time): 60 ticks * ~0.216 m = ~12.9 m whatever the frame rate
xdotool keydown w; wait_ticks 60; xdotool keyup w; wait_ticks 6
xdotool key space; wait_ticks 12
xdotool mousemove_relative -- 0 230; wait_ticks 16
xdotool click 1; wait_ticks 16
xdotool key 2; wait_ticks 6
[ "${MCPLAY_DEBUG:-0}" = 1 ] && { for q in SelectedItemSlot Inventory abilities; do echo "data get entity Player $q" >&3; done; echo "execute at Player if entity @e[type=!minecraft:player,distance=..12]" >&3; sleep 0.5; }
xdotool click 3; wait_ticks 16
xdotool key 3; wait_ticks 6
xdotool click 4; wait_ticks 6
xdotool mousemove_relative -- 100 0; wait_ticks 20
touch "$W/quit"
wait $PP; rc=$?
grep -v "mcplay: progress" "$W/play.out" > "$W/play.clean" && mv "$W/play.clean" "$W/play.out"   # the sim-time progress lines are only for the script above
sed 's/^/  /' "$W/play.out" | tail -8
ERR=0
[ $rc -eq 0 ] || { echo "  FAIL  mcplay exit code $rc"; ERR=1; }
grep -q "joined the game" "$W/server.log" || { echo "  FAIL  server never saw the join"; ERR=1; }
grep -qE "moved wrongly|moved too quickly" "$W/server.log" && { echo "  FAIL  server rejected our movement"; ERR=1; }
FR=$(grep -o 'frames=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); CH=$(grep -o 'chunks=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); TK=$(grep -o 'ticks=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${FR:-0}" -gt 20 ] || { echo "  FAIL  too few frames (${FR:-0})"; ERR=1; }
[ "${CH:-0}" -gt 10 ] || { echo "  FAIL  too few chunks (${CH:-0})"; ERR=1; }
[ "${TK:-0}" -gt 20 ] || { echo "  FAIL  too few ticks (${TK:-0})"; ERR=1; }
[ -s "$W/shot.png" ] || { echo "  FAIL  no screenshot"; ERR=1; }
# light + time of day: update_time packets were applied, the 5x5 light region loaded, noon = daylight 256
grep -q 'light=true region=true' "$W/play.out" || { echo "  FAIL  light region not loaded"; ERR=1; }
TP=$(grep -o 'time_packets=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); DL=$(grep -o 'daylight=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${TP:-0}" -gt 0 ] || { echo "  FAIL  no update_time packet applied (${TP:-0})"; ERR=1; }
[ "${DL:-0}" -ge 200 ] || { echo "  FAIL  noon daylight too low (${DL:-0})"; ERR=1; }
# sound: bank loaded, walk/break/place sounds mixed into a non-empty WAV (the server cannot be heard; the file is the proof)
AB=$(grep -o 'audio bytes=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); AP=$(grep -o ' played=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
AS=$(grep -o 'sounds=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); AC=$(grep -o 'crc=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${AB:-0}" -gt 100000 ] || { echo "  FAIL  audio output too small (${AB:-0} bytes)"; ERR=1; }
[ "${AS:-0}" -ge 2 ] || { echo "  FAIL  too few sounds triggered (${AS:-0})"; ERR=1; }
[ "${AP:-0}" -ge 1 ] || { echo "  FAIL  no voice played (${AP:-0})"; ERR=1; }
if [ -s "$W/day.wav" ]; then
    python3 tools/mc/play/audio_check.py "$W/day.wav" "$W/play.out" --min-events 2 || ERR=1
else echo "  FAIL  no day.wav"; ERR=1; fi
# inventory (Set Creative Slot answered by the server: updates seen, none malformed, first hotbar slots filled)
IU=$(grep -o 'inv updates=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); IB=$(grep -o ' bad=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${IU:-0}" -gt 0 ] && [ "${IB:-1}" -eq 0 ] || { echo "  FAIL  inventory updates=${IU:-0} bad=${IB:-?}"; ERR=1; }
grep -qE 'hotbar=[1-9][0-9]*x[1-9][0-9]*,[1-9][0-9]*x[1-9]' "$W/play.out" || { echo "  FAIL  hotbar not filled: $(grep -o 'hotbar=[^ ]*' "$W/play.out" | head -1)"; ERR=1; }
# the real actions made their sounds: break + place (server block update) + at least one step, each logged by mc_audio (MCPLAY_AUDIO_LOG)
grep -qE 'audiolog .* ev=block\.[a-z_]+\.break ' "$W/play.out" || { echo "  FAIL  no block break sound logged"; ERR=1; }
grep -qE 'audiolog .* ev=block\.[a-z_]+\.place ' "$W/play.out" || { echo "  FAIL  no block place sound logged (server refused the place? run with MCPLAY_DEBUG=1: dbg place / dbg block_change lines)"; ERR=1; }
grep -qE 'audiolog .* ev=block\.[a-z_]+\.step ' "$W/play.out" || { echo "  FAIL  no footstep sound logged"; ERR=1; }
# footsteps follow the walked DISTANCE (not frames): one per 1.67 blocks even at 3-6 fps; the walk is on a flat platform so nothing blocks it
WM=$(grep -o 'walked_mm=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); WS=$(grep -o ' steps=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${WM:-0}" -ge 6000 ] || { echo "  FAIL  walked only ${WM:-0} mm on the platform"; ERR=1; }
[ "${WS:--9}" -ge $(( ${WM:-0} / 1667 - 1 )) ] || { echo "  FAIL  ${WS:-?} step sounds for ${WM:-0} mm walked (expected >= $(( ${WM:-0} / 1667 - 1 )))"; ERR=1; }
# the engine's peak stats (hold value since start) show the mix was not silent
PK=$(grep -o 'peak_l100=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${PK:-0}" -gt 0 ] || { echo "  FAIL  engine peak_l stats are 0 (no sound reached the mix)"; ERR=1; }
echo "  audio bytes=${AB:-?} crc=${AC:-?} sounds=${AS:-?} played=${AP:-?} walked_mm=${WM:-?} steps=${WS:-?} peak_l100=${PK:-?}"
if [ "${MCPLAY_ONLY_DAY:-0}" = 1 ]; then grep -E "dbg|walk|audiolog.*(step|place|break)" "$W/play.out" | head -${MCPLAY_DAY_LINES:-40}; exit $ERR; fi
# second short run at midnight: server time drives the sky (daylight 0) and the picture differs from noon
echo "time set 18000" >&3; sleep 1
MCPLAY_RUN_S=6 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/night.png" \
    "$W/mcplay" 127.0.0.1 "$PORT" Player2 > "$W/night.out" 2>&1; nrc=$?
[ $nrc -eq 0 ] || { echo "  FAIL  night run exit code $nrc"; ERR=1; }
ND=$(grep -o 'daylight=[0-9]*' "$W/night.out" | head -1 | cut -d= -f2)
[ "${ND:-256}" -le 20 ] || { echo "  FAIL  midnight daylight too high (${ND:-?})"; ERR=1; }
NH=$(grep -o 'hash=[0-9a-f]*' "$W/night.out" | head -1); DH=$(grep -o 'hash=[0-9a-f]*' "$W/play.out" | head -1)
[ -n "$NH" ] && [ "$NH" != "$DH" ] || { echo "  FAIL  night picture equals day picture"; ERR=1; }
# GL path (MCPLAY_GL=1): glwindow (EGL, Mesa llvmpipe = software, NO real GPU) + gl_mc/gl_scene, frame shown without read back
if [ "${MCPLAY_SKIP_GL:-0}" != 1 ]; then
echo "time set day" >&3; sleep 1
MCPLAY_GL=1 MCPLAY_VSYNC=0 FIRN_AUDIO=null MCPLAY_RUN_S=8 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/gl.png" \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player3 > "$W/gl.out" 2>&1; grc=$?
sed 's/^/  gl: /' "$W/gl.out" | tail -7
[ $grc -eq 0 ] || { echo "  FAIL  GL run exit code $grc"; ERR=1; }
grep -q 'mcplay: gl renderer=' "$W/gl.out" || { echo "  FAIL  GL path did not start"; ERR=1; }
GF=$(grep -o 'frames=[0-9]*' "$W/gl.out" | head -1 | cut -d= -f2); GC=$(grep -o 'chunks=[0-9]*' "$W/gl.out" | head -1 | cut -d= -f2)
[ "${GF:-0}" -gt 5 ] || { echo "  FAIL  GL: too few frames (${GF:-0})"; ERR=1; }
[ "${GC:-0}" -gt 10 ] || { echo "  FAIL  GL: too few chunks (${GC:-0})"; ERR=1; }
[ "$(stat -c %s "$W/gl.png" 2>/dev/null || echo 0)" -gt 1500 ] || { echo "  FAIL  GL: no screenshot"; ERR=1; }
GH=$(grep -o 'hash=[0-9a-f]*' "$W/gl.out" | head -1)
echo "  gl frames=${GF:-?} chunks=${GC:-?} $GH"
if [ -n "${MCPLAY_GL_PNG_OUT:-}" ] && [ -s "$W/gl.png" ]; then cp "$W/gl.png" "$MCPLAY_GL_PNG_OUT"; fi
fi
# ---- sound probe: 12 known sounds through the real engine/bank/file sink, found again in the dump (tools/mc/play/audio_check.py)
if [ "${MCPLAY_SKIP_AUDIO:-0}" != 1 ]; then
echo "time set day" >&3; sleep 1
MCPLAY_AMBIENT_DARK=1 MCPLAY_AMBIENT_X=100 MCPLAY_AUDIO_PROBE=1 MCPLAY_AUDIO_LOG=1 FIRN_AUDIO="file:$W/probe.wav" MCPLAY_RUN_S=11 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player4 > "$W/probe.out" 2>&1; prc=$?
[ $prc -eq 0 ] || { echo "  FAIL  probe run exit code $prc"; ERR=1; }
python3 tools/mc/play/audio_check.py "$W/probe.wav" "$W/probe.out" --probe --ambient --min-events 12 || ERR=1
fi
# ---- device sink: FIRN_AUDIO=device through a private PulseAudio daemon with a null sink; the monitor is recorded (parec) and checked
if [ "${MCPLAY_SKIP_AUDIO:-0}" != 1 ] && [ "${MCPLAY_SKIP_PULSE:-0}" != 1 ] && command -v pulseaudio >/dev/null && command -v parec >/dev/null; then
PAD="$W/pa"; mkdir -p "$PAD"; chmod 700 "$PAD"
XDG_RUNTIME_DIR="$PAD" pulseaudio -n --daemonize=no --exit-idle-time=-1 --disallow-exit --log-target=file:"$W/pulse.log" \
    -L "module-native-protocol-unix socket=$PAD/native auth-anonymous=1" -L "module-null-sink sink_name=mcnull rate=44100 channels=2" >/dev/null 2>&1 & PA_PID=$!
for i in $(seq 1 20); do [ -S "$PAD/native" ] && break; sleep 0.5; done
if [ -S "$PAD/native" ]; then
    export PULSE_SERVER="unix:$PAD/native"
    parec -d mcnull.monitor --format=s16le --rate=44100 --channels=2 --file-format=wav "$W/dev.wav" 2>/dev/null & REC_PID=$!
    sleep 1
    MCPLAY_AUDIO_PROBE=1 FIRN_AUDIO=device MCPLAY_RUN_S=11 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player7 > "$W/dev.out" 2>&1; drc=$?
    sleep 1; kill "$REC_PID" 2>/dev/null; wait "$REC_PID" 2>/dev/null
    [ $drc -eq 0 ] || { echo "  FAIL  device run exit code $drc"; ERR=1; }
    grep -q "no sound output" "$W/dev.out" && { echo "  FAIL  device sink did not open"; ERR=1; }
    python3 tools/mc/play/audio_check.py "$W/dev.wav" "$W/dev.out" --bursts 9 || ERR=1
else echo "  FAIL  private pulseaudio did not start: $(tail -2 "$W/pulse.log" 2>/dev/null)"; ERR=1; fi
unset PULSE_SERVER
kill "$PA_PID" 2>/dev/null
fi
# ---- GL vs CPU picture of the SAME scene in the real play path (water pool, glass wall, tinted glass, backdrop): PSNR
if [ "${MCPLAY_SKIP_PSNR:-0}" != 1 ]; then
echo "time set day" >&3; echo "gamerule spawnRadius 0" >&3; sleep 1
for mode in cpu gl; do
    PN=Psnr$mode; GLF=0; [ $mode = gl ] && GLF=1
    MCPLAY_GL=$GLF MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_RUN_S=${MCPLAY_PSNR_S:-26} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/ps_$mode.png" \
        timeout 180 "$W/mcplay" 127.0.0.1 "$PORT" "$PN" > "$W/ps_$mode.out" 2>&1 &
    PP=$!
    wj "$PN"; sleep 2
    for cmd in "fill ~-3 ~-3 ~4 ~3 ~0 ~8 stone" "fill ~-2 ~-2 ~5 ~2 ~0 ~7 water" "fill ~-4 ~-2 ~11 ~4 ~4 ~11 glass" "fill ~-1 ~-2 ~9 ~1 ~3 ~9 blue_stained_glass" "fill ~-6 ~-2 ~14 ~6 ~6 ~14 red_wool"; do
        echo "execute at $PN run $cmd" >&3
    done
    sleep 3
    xdotool mousemove 320 180 2>/dev/null; xdotool mousemove_relative -- 0 25; sleep 4
    wait $PP; prc=$?
    [ $prc -eq 0 ] || { echo "  FAIL  $mode picture run exit code $prc"; ERR=1; }
    echo "  $mode: $(grep -o 'frames=[0-9]*' "$W/ps_$mode.out" | head -1) $(grep -o 'chunks=[0-9]*' "$W/ps_$mode.out" | head -1) $(grep -o 'hash=[0-9a-f]*' "$W/ps_$mode.out" | head -1)"
done
if [ -s "$W/ps_cpu.png" ] && [ -s "$W/ps_gl.png" ]; then
    python3 tools/mc/play/psnr.py "$W/ps_cpu.png" "$W/ps_gl.png" --min-db "${MCPLAY_PSNR_MIN:-30}" --mask 0,0,140,50 --diff "$W/ps_diff.png" || ERR=1
    if [ -n "${MCPLAY_PSNR_OUT:-}" ]; then cp "$W/ps_cpu.png" "${MCPLAY_PSNR_OUT}_cpu.png"; cp "$W/ps_gl.png" "${MCPLAY_PSNR_OUT}_gl.png"; cp "$W/ps_diff.png" "${MCPLAY_PSNR_OUT}_diff.png"; fi
else echo "  FAIL  psnr: missing picture"; ERR=1; fi
fi
# ---- survival: iron pickaxe from /give, look down, hold the left button -> a block is dug with the table time (mc_dig)
if [ "${MCPLAY_SKIP_SURVIVAL:-0}" != 1 ]; then
MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_RUN_S=${MCPLAY_SURV_S:-24} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
    timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" Player6 > "$W/surv.out" 2>&1 &
PP=$!
wj Player6; sleep 1
echo "gamemode survival Player6" >&3; echo "give Player6 iron_pickaxe 1" >&3; echo "give Player6 stone 32" >&3
sleep 2
xdotool mousemove 320 180 2>/dev/null; xdotool mousemove_relative -- 0 300; xdotool key 1; sleep 0.5
xdotool mousedown 1; sleep 4; xdotool mouseup 1; sleep 1
wait $PP; src=$?
[ $src -eq 0 ] || { echo "  FAIL  survival run exit code $src"; ERR=1; }
SD=$(grep -o ' dig=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2); SMS=$(grep -o 'dig_ms=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2)
SU=$(grep -o 'inv updates=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2); SB=$(grep -o ' bad=[0-9]*' "$W/surv.out" | head -1 | cut -d= -f2)
echo "  survival dig=${SD:-?} dig_ms=${SMS:-?} inv_updates=${SU:-?} bad=${SB:-?} $(grep -o 'hotbar=[^ ]*' "$W/surv.out" | head -1)"
[ "${SD:-0}" -ge 1 ] || { echo "  FAIL  survival: nothing dug"; ERR=1; }
[ "${SU:-0}" -gt 0 ] && [ "${SB:-1}" -eq 0 ] || { echo "  FAIL  survival: inventory updates=${SU:-0} bad=${SB:-?}"; ERR=1; }
fi
echo stop >&3; sleep 1
echo "  day daylight=${DL:-?} night daylight=${ND:-?} night $NH"
grep -o 'hash=[0-9a-f]*' "$W/play.out" | sed 's/^/  /'
if [ -n "${MCPLAY_PNG_OUT:-}" ] && [ -s "$W/shot.png" ]; then cp "$W/shot.png" "$MCPLAY_PNG_OUT"; echo "  PNG: $MCPLAY_PNG_OUT"; fi
[ $ERR -eq 0 ] && echo "RESULT mc-play: ok" || echo "RESULT mc-play: FAIL"
exit $ERR

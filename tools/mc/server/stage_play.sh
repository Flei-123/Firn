#!/usr/bin/env bash
# sourced by run.sh: stage G, real mcplay --singleplayer (flat and saved world), screenshot not empty
for t in Xvfb python3; do command -v $t >/dev/null || { echo "  FAIL  missing tool: $t"; ERR=1; return 0; }; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { echo "  FAIL  compile mcplay"; head -5 "$W/err"; ERR=1; return 0; }
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 > "$W/xvfb.log" 2>&1 &
XVPID=$!
sleep 2
export DISPLAY=:$DN MCPLAY_PROTOCOL=763 MC_VANILLA_JAR=${MC_VANILLA_JAR:-$JARS/client-1.20.1.jar}
cat > "$W/pix.py" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(len(set(im.crop((0, 60, 640, 360)).getdata())))
PY
play_once() {
    local name=$1 shot=$2; shift 2
    local t0=$(date +%s%3N)
    env MCPLAY_GL=0 MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_RUN_S=${PLAY_RUN_S:-12} MCPLAY_VIEW=4 MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$shot" \
        timeout 200 "$W/mcplay" --singleplayer "$@" > "$W/$name.out" 2>&1
    PLAY_RC=$?
    local sp=$(sed -n 's/.*spawn unix_ms=\([0-9]*\).*/\1/p' "$W/$name.out" | head -1)
    SPAWN_MS=""
    [ -n "$sp" ] && SPAWN_MS=$(( sp - t0 ))
}
check_play() {
    local name=$1 shot=$2
    local cols=$(grep 'stream workers' "$W/$name.out" | sed -n 's/.*cols=\([0-9]*\).*/\1/p' | tail -1)
    local colors=$(python3 -I "$W/pix.py" "$shot" 2>/dev/null | tail -1)
    if [ "$PLAY_RC" -eq 0 ] && grep -q 'Player joined the game' "$W/$name.out" && [ -n "$SPAWN_MS" ] && [ "${colors:-0}" -ge 40 ] && [ "${cols:-0}" -ge 20 ]; then
        echo "  ok    $name: joined, spawn after ${SPAWN_MS} ms, columns=$cols, picture colors=$colors"
    else
        echo "  FAIL  $name: rc=$PLAY_RC spawn_ms=${SPAWN_MS:-none} columns=${cols:-0} colors=${colors:-0}"; tail -3 "$W/$name.out" | sed 's/^/        /'; touch "$W/failed"
    fi
}
listed=$("$W/mcplay" --list-versions 2>/dev/null)
if python3 -I -c 'import json,sys; d=json.loads(sys.argv[1])["versions"]; m=json.load(open(sys.argv[2]))["versions"]; j={k for k,v in m.items() if v["status"]=="joinable"}; assert {e["name"] for e in d}==j and all(e["client"] for e in d)' "$listed" "$REPO/tools/mcgen/supported.json" 2>/dev/null; then echo "  ok    mcplay --list-versions equals the joinable entries of supported.json"; else echo "  FAIL  mcplay --list-versions: $listed"; ERR=1; fi
play_once sp_flat "$W/sp_flat.png"
check_play sp_flat "$W/sp_flat.png"
mkdir -p "$W/spworld"
cp -r "$WORLD" "$W/spworld/world"
rm -f "$W/spworld/world/session.lock"
play_once sp_save "$W/sp_save.png" "$W/spworld/world"
check_play sp_save "$W/sp_save.png"
if [ -f "$W/sp_flat.png" ] && [ -f "$W/sp_save.png" ] && ! cmp -s "$W/sp_flat.png" "$W/sp_save.png"; then echo "  ok    flat and saved world give different pictures"; else echo "  FAIL  flat and saved world pictures are identical"; touch "$W/failed"; fi
play_once sp_missing "$W/sp_missing.png" "$W/no_such_world"
if [ "$PLAY_RC" -ne 0 ] && grep -q 'cannot start the integrated server' "$W/sp_missing.out"; then echo "  ok    missing world directory is refused (rc=$PLAY_RC)"; else echo "  FAIL  missing world directory not refused (rc=$PLAY_RC)"; touch "$W/failed"; fi
if [ -n "${MCS_TIMING:-}" ]; then
    for i in 1 2 3; do PLAY_RUN_S=4 play_once sp_t$i "$W/sp_t$i.png"; echo "  info  spawn timing $i: ${SPAWN_MS} ms (load $(cut -d' ' -f1 /proc/loadavg))"; done
fi

#!/usr/bin/env bash
# tools/mc/play/run_packsnd.sh -- mcplay with pack sounds (MC_PACK_SOUNDS=both|original) end to end: Xvfb + official 1.20.1 server + mcplay + xdotool.
# Runs: (1) both, FILE sink: start + hot reload (key F5) with a CHANGED OGG -> sample table must not grow; (2) both, DEVICE sink (private
# PulseAudio null sink): same, the audio thread is stopped / restarted around the reload; (3) original, FILE sink: only pack events.
# (4) O key: packs off/on at runtime; (5) original start + O twice = still pack-only (no synth events). Prints `RESULT mc-packsnd: ok`. Env: FIRNC, MC_VIEW (3), MCCLIENT_JAR_CACHE, MCPLAY_SKIP_PULSE=1.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); SRV_PID=""; XV_PID=""; PA_PID=""; REC_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$REC_PID" ] && kill "$REC_PID" 2>/dev/null; [ -n "$PA_PID" ] && kill "$PA_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-packsnd: FAIL"; exit 1; }
for t in Xvfb xdotool java python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds-$(sha256sum tools/sounds/mc.json | cut -c1-10)}
if [ ! -s "$SND/bank.json" ]; then
    $FIRNC -o "$W/sndgen" tools/sounds/gen/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "tools/sounds/gen does not compile"; }
    mkdir -p "$SND"; "$W/sndgen" tools/sounds/mc.json "$SND" >/dev/null 2>&1 || fail "sound bank generation"
fi
export MCPLAY_SOUNDS="$SND"
# two packs with the same sounds.json but different OGG bytes: p.zip (start) is replaced by q.zip (reload = changed files)
python3 - "$W" tests/data/vorbis <<'EOF' || fail "pack build"
import sys, zipfile
w, d = sys.argv[1], sys.argv[2]
meta = '{"pack":{"pack_format":15,"description":"packsnd test"}}'
snd = ('{"block.stone.step":{"sounds":["custom/a","custom/b"]},"ui.button.click":{"sounds":["custom/b"]},'
       '"entity.zombie.ambient":{"sounds":["custom/a"]},"block.grass.step":{"sounds":["custom/b"]}}')
for name, a, b in (("p", "tone_s48k_q0.ogg", "sine_m22k_q3.ogg"), ("q", "sine_m22k_q3.ogg", "tone_s48k_q0.ogg")):
    with zipfile.ZipFile(f"{w}/{name}.zip", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("pack.mcmeta", meta)
        z.writestr("assets/minecraft/sounds.json", snd)
        z.write(f"{d}/{a}", "assets/minecraft/sounds/custom/a.ogg")
        z.write(f"{d}/{b}", "assets/minecraft/sounds/custom/b.ogg")
EOF
MCV=1.20.1; VIEW=${MC_VIEW:-3}
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
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
echo "time set day" >&3
DN=$((300 + RANDOM % 100))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0

# run <name> <mode> <sink> <reload 0|1> : mcplay in the background, optional F5 key after 9 s with q.zip swapped in
run() {
    local name=$1 mode=$2 sink=$3 reload=$4
    cp "$W/p.zip" "$W/cur.zip"
    MC_PACKS="$W/cur.zip" MC_PACK_SOUNDS=$mode MCPLAY_AUDIO_PROBE=1 MCPLAY_AUDIO_LOG=1 FIRN_AUDIO="$sink" MCPLAY_RUN_S=16 MCPLAY_VIEW=$VIEW \
        MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 timeout 120 "$W/mcplay" 127.0.0.1 "$PORT" "$name" > "$W/$name.out" 2>&1 &
    local pp=$!
    if [ "$reload" = 1 ]; then
        sleep 9
        cp "$W/q.zip" "$W/cur.zip"
        xdotool mousemove 320 180 2>/dev/null; xdotool key F5; sleep 0.5
    elif [ "$reload" = 2 ]; then
        # O key twice: packs off (synth bank back), packs on again
        sleep 9
        xdotool mousemove 320 180 2>/dev/null; xdotool key o; sleep 1.5; xdotool key o; sleep 0.5
    fi
    wait $pp; return $?
}
# lines: `mcplay: pack sounds mode=M events=E samples=S` -> field of the Nth line
field() { grep "pack sounds mode=" "$W/$1.out" | sed -n "${2}p" | grep -o "$3=[0-9]*" | cut -d= -f2; }
check_reload() {
    local name=$1 mode=$2 e1 e2 s1 s2 n
    n=$(grep -c "pack sounds mode=$mode" "$W/$name.out")
    [ "$n" -ge 2 ] || { echo "  FAIL  $name: expected start + reload lines, got $n"; ERR=1; return; }
    e1=$(field "$name" 1 events); e2=$(field "$name" 2 events); s1=$(field "$name" 1 samples); s2=$(field "$name" 2 samples)
    echo "  $name: events $e1 -> $e2, samples $s1 -> $s2"
    [ "${e1:-0}" -ge 4 ] || { echo "  FAIL  $name: too few pack events (${e1:-0})"; ERR=1; }
    [ "$e1" = "$e2" ] || { echo "  FAIL  $name: reload changed the event count"; ERR=1; }
    [ "$s1" = "$s2" ] || { echo "  FAIL  $name: sample table grew on reload ($s1 -> $s2)"; ERR=1; }
}

# (1) both + FILE sink + reload
run PsFile both "file:$W/file.wav" 1; rc=$?
[ $rc -eq 0 ] || { echo "  FAIL  PsFile exit code $rc"; ERR=1; }
check_reload PsFile 2
python3 tools/mc/play/audio_check.py "$W/file.wav" "$W/PsFile.out" --probe --min-events 12 || ERR=1

# (2) both + DEVICE sink (audio thread stop / restart around the reload)
if [ "${MCPLAY_SKIP_PULSE:-0}" != 1 ] && command -v pulseaudio >/dev/null && command -v parec >/dev/null; then
    PAD="$W/pa"; mkdir -p "$PAD"; chmod 700 "$PAD"
    XDG_RUNTIME_DIR="$PAD" pulseaudio -n --daemonize=no --exit-idle-time=-1 --disallow-exit --log-target=file:"$W/pulse.log" \
        -L "module-native-protocol-unix socket=$PAD/native auth-anonymous=1" -L "module-null-sink sink_name=mcnull rate=44100 channels=2" >/dev/null 2>&1 & PA_PID=$!
    for i in $(seq 1 20); do [ -S "$PAD/native" ] && break; sleep 0.5; done
    if [ -S "$PAD/native" ]; then
        export PULSE_SERVER="unix:$PAD/native"
        # the device run is timing sensitive (real sink, load on the box): one retry, the first failure stays in the log (r174)
        dev_try=1
        while :; do
            ERR_SAVE=$ERR; ERR=0
            parec -d mcnull.monitor --format=s16le --rate=44100 --channels=2 --file-format=wav "$W/dev.wav" 2>/dev/null & REC_PID=$!
            sleep 1
            run PsDev both device 1; rc=$?
            sleep 1; kill "$REC_PID" 2>/dev/null; wait "$REC_PID" 2>/dev/null
            [ $rc -eq 0 ] || { echo "  FAIL  PsDev exit code $rc"; ERR=1; }
            grep -q "no sound output" "$W/PsDev.out" && { echo "  FAIL  device sink did not open"; ERR=1; }
            grep -q "did not restart" "$W/PsDev.out" && { echo "  FAIL  audio thread did not restart"; ERR=1; }
            check_reload PsDev 2
            python3 tools/mc/play/audio_check.py "$W/dev.wav" "$W/PsDev.out" --bursts 4 || ERR=1
            DERR=$ERR; ERR=$ERR_SAVE
            [ $DERR -eq 0 ] && break
            [ $dev_try -ge 2 ] && { ERR=1; break; }
            echo "  RETRY PsDev (attempt $dev_try failed, load $(cut -d' ' -f1 /proc/loadavg))"
            dev_try=$((dev_try + 1))
        done
    else echo "  FAIL  private pulseaudio did not start: $(tail -2 "$W/pulse.log" 2>/dev/null)"; ERR=1; fi
    unset PULSE_SERVER
    kill "$PA_PID" 2>/dev/null
fi

# (3) original + FILE sink: only the pack events exist (the probe names that the pack does not define stay silent)
run PsOrig original "file:$W/orig.wav" 1; rc=$?
[ $rc -eq 0 ] || { echo "  FAIL  PsOrig exit code $rc"; ERR=1; }
check_reload PsOrig 1
if grep -q "no sound bank" "$W/PsOrig.out"; then echo "  FAIL  original: no bank"; ERR=1; fi

# (4) both + FILE sink + O key twice: synth <-> packs, the sample table must not grow, the probe stays complete
run PsToggle both "file:$W/toggle.wav" 2; rc=$?
[ $rc -eq 0 ] || { echo "  FAIL  PsToggle exit code $rc"; ERR=1; }
a=$(grep -n "mcplay: sounds=synth" "$W/PsToggle.out" | head -1 | cut -d: -f1); b=$(grep -n "mcplay: sounds=both" "$W/PsToggle.out" | head -1 | cut -d: -f1)
if [ -z "$a" ] || [ -z "$b" ] || [ "$a" -ge "$b" ]; then echo "  FAIL  PsToggle: expected 'sounds=synth' then 'sounds=both' (got ${a:-none}/${b:-none})"; ERR=1
else
    t1=$(field PsToggle 1 samples); t2=$(field PsToggle 2 samples); te=$(field PsToggle 2 events)
    echo "  PsToggle: samples $t1 -> $t2, events on switch-on ${te:-?}"
    [ "${te:-0}" -ge 4 ] || { echo "  FAIL  PsToggle: pack sounds did not come back (events ${te:-0})"; ERR=1; }
    [ "$t1" = "$t2" ] || { echo "  FAIL  PsToggle: sample table grew ($t1 -> $t2)"; ERR=1; }
fi
python3 tools/mc/play/audio_check.py "$W/toggle.wav" "$W/PsToggle.out" --probe --min-events 12 || ERR=1

# (5) original + FILE sink + O key twice: start without the synth bank, packs off (synth), packs on again = ORIGINAL (the synth events must be
# gone again: every engine event is a pack event, `total` == `events`)
run PsOrigToggle original "file:$W/origtoggle.wav" 2; rc=$?
[ $rc -eq 0 ] || { echo "  FAIL  PsOrigToggle exit code $rc"; ERR=1; }
a=$(grep -n "mcplay: sounds=synth" "$W/PsOrigToggle.out" | head -1 | cut -d: -f1); b=$(grep -n "mcplay: sounds=original" "$W/PsOrigToggle.out" | head -1 | cut -d: -f1)
if [ -z "$a" ] || [ -z "$b" ] || [ "$a" -ge "$b" ]; then echo "  FAIL  PsOrigToggle: expected 'sounds=synth' then 'sounds=original' (got ${a:-none}/${b:-none})"; ERR=1
else
    o1=$(field PsOrigToggle 1 events); ot1=$(field PsOrigToggle 1 total); o2=$(field PsOrigToggle 2 events); ot2=$(field PsOrigToggle 2 total)
    echo "  PsOrigToggle: events $o1 (total $ot1) -> $o2 (total $ot2)"
    [ "${o1:-0}" -ge 4 ] && [ "$o1" = "$ot1" ] || { echo "  FAIL  PsOrigToggle: start is not pack-only (events ${o1:-?} total ${ot1:-?})"; ERR=1; }
    [ "${o2:-0}" -ge 4 ] && [ "$o2" = "$ot2" ] || { echo "  FAIL  PsOrigToggle: original after the toggle still has synth events (events ${o2:-?} total ${ot2:-?})"; ERR=1; }
fi
grep -q "did not restart" "$W/PsOrigToggle.out" && { echo "  FAIL  PsOrigToggle: audio thread did not restart"; ERR=1; }

if [ $ERR -eq 0 ]; then echo "RESULT mc-packsnd: ok"; else tail -15 "$W/PsFile.out" | sed 's/^/  /'; echo "RESULT mc-packsnd: FAIL"; exit 1; fi

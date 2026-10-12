#!/usr/bin/env bash
# tools/audio/latency/run.sh -- latency of an audio.pulse stream against a real PulseAudio daemon
# (module-null-sink in a private HOME / XDG_RUNTIME_DIR, killed and removed afterwards).
#
# For each buffer setting (tlength / prebuf / minreq / adjust_latency / lean writer) a fresh daemon is
# started, tools/audio/latency/main.fi plays 3 s and samples GET_PLAYBACK_LATENCY about every 2 ms.
# Checks: the protocol works (reply parsed, granted attributes read back), pactl agrees with the numbers,
# smaller tlength means smaller latency, and a SFX setting (<= 50 ms, never dry) exists.
# Prints the table that docs/AUDIO.md quotes.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
if ! command -v pulseaudio >/dev/null || ! command -v pactl >/dev/null; then
    echo "SKIP pulseaudio/pactl missing"
    exit 0
fi
W=$(mktemp -d)
DPID=""
stop_daemon() {
    if [ -n "$DPID" ]; then
        kill "$DPID" 2>/dev/null || true
        wait "$DPID" 2>/dev/null || true
        DPID=""
    fi
}
trap 'stop_daemon; rm -rf "$W"' EXIT
FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$W/lat" tools/audio/latency/main.fi

start_daemon() {
    local pa="$W/pa"
    rm -rf "$pa"
    mkdir -p "$pa"
    cat >"$pa/rc.pa" <<RC
load-module module-native-protocol-unix socket=$pa/native auth-anonymous=1
load-module module-null-sink sink_name=nul format=s16le rate=44100 channels=2
set-default-sink nul
RC
    HOME="$pa" XDG_RUNTIME_DIR="$pa" pulseaudio -n --daemonize=no --exit-idle-time=-1 --disallow-exit \
        --file="$pa/rc.pa" --log-target=stderr >"$pa/log" 2>&1 &
    DPID=$!
    for _ in $(seq 100); do
        if [ -S "$pa/native" ] && pactl --server="unix:$pa/native" list short sinks 2>/dev/null | grep -q nul; then
            return 0
        fi
        sleep 0.1
    done
    echo "FAIL  daemon did not start"
    cat "$pa/log" | tail -5
    exit 1
}

# name|tlength_ms|prebuf_ms|minreq_ms|adjust|keep_ms
CONFIGS="
music-250|250|-1|-1|0|0
music-100-adj|100|-1|-1|1|0
game-50-adj|50|-1|-1|1|0
game-50-noadj|50|-1|-1|0|0
sfx-30-adj|30|-1|-1|1|0
sfx-20-adj|20|-1|-1|1|0
sfx-20-adj-pb0|20|0|-1|1|0
sfx-20-adj-mr5|20|-1|5|1|0
lean-100-adj-keep15|100|0|-1|1|15
lean-50-adj-keep10|50|0|-1|1|10
"
RES="$W/res.txt"
: >"$RES"
PCTL="$W/pctl.txt"
: >"$PCTL"
printf '%-22s %7s %7s %7s %7s | %6s %6s %6s | %5s %7s %6s | %6s %6s\n' config tl_B pre_B mr_B sink_us min_us mean_us max_us samples starved first_ms q_min q_max
while IFS='|' read -r name tl pb mr adj keep; do
    [ -z "$name" ] && continue
    start_daemon
    export PULSE_SERVER="unix:$W/pa/native" HOME="$W/pa"
    "$W/lat" "$tl" "$pb" "$mr" "$adj" "$keep" 3 >"$W/out.txt" &
    LP=$!
    sleep 1.8
    # what pactl sees at the same time (Buffer Latency + Sink Latency of our stream)
    pactl list sink-inputs 2>/dev/null | awk -v n="$name" '/Buffer Latency/ {b=$3} /Sink Latency/ {s=$3} END {print n, b+0, s+0}' >>"$PCTL"
    wait "$LP" || true
    stop_daemon
    unset PULSE_SERVER
    line=$(grep '^LAT ' "$W/out.txt" || echo "LAT MISSING")
    echo "$name $line" >>"$RES"
    set -- $line
    printf '%-22s %7s %7s %7s %7s | %6s %6s %6s | %5s %7s %6s | %6s %6s\n' "$name" "${2:-?}" "${3:-?}" "${4:-?}" "${5:-?}" "${6:-?}" "${7:-?}" "${8:-?}" "${9:-?}" "${10:-?}" "${11:-?}" "${12:-?}" "${13:-?}"
done <<<"$CONFIGS"

python3 -I - "$RES" "$PCTL" <<'PY'
import sys
res = {}
for ln in open(sys.argv[1]):
    p = ln.split()
    if len(p) < 14 or p[1] != "LAT":
        res[p[0]] = None
        continue
    v = list(map(int, p[2:]))
    res[p[0]] = dict(tl=v[0], pre=v[1], mr=v[2], sink=v[3], mn=v[4], mean=v[5], mx=v[6], n=v[7], starved=v[8], first=v[9], qmin=v[10], qmax=v[11])
pc = {}
for ln in open(sys.argv[2]):
    p = ln.split()
    if len(p) == 3:
        pc[p[0]] = (int(float(p[1])), int(float(p[2])))
fails = 0
def check(name, ok, detail=""):
    global fails
    print(("  ok    " if ok else "  FAIL  ") + name + ("" if ok else "  " + detail))
    if not ok:
        fails += 1
check("every setting produced a LAT line", all(v is not None for v in res.values()), str([k for k, v in res.items() if v is None]))
good = {k: v for k, v in res.items() if v}
check("granted tlength read back (> 0) for every setting", all(v["tl"] > 0 and v["mr"] > 0 for v in good.values()))
check("sink latency announced (> 0) for every setting", all(v["sink"] > 0 for v in good.values()))
check("every setting played (>= 100 samples while playing)", all(v["n"] >= 100 for v in good.values()), str({k: v["n"] for k, v in good.items()}))
check("latency never negative / absurd", all(0 < v["mn"] <= v["mean"] <= v["mx"] < 3000000 for v in good.values()))
# pactl and our protocol client see the same stream: pactl's Buffer Latency (queued / rate) fits the granted tlength
# (Sink Latency is not compared: the null sink reports how far ahead it has rendered, which jumps around)
okp = True
bad = []
for k, v in good.items():
    if k not in pc or pc[k][0] == 0:
        okp = False
        bad.append(k)
        continue
    buf = pc[k][0]
    tl_us = v["tl"] * 1000000 // (44100 * 4)
    lo = v["qmin"]
    hi = v["qmax"]
    if not (0 < buf <= tl_us * 1.10 + 2000) or not (lo - 5000 <= buf <= hi + 5000):
        okp = False
        bad.append("%s buf=%d tl=%d ours=%d..%d" % (k, buf, tl_us, lo, hi))
check("pactl Buffer Latency is inside the range our GET_PLAYBACK_LATENCY samples saw (+-5 ms) and <= tlength", okp, str(bad))
# ordering: smaller tlength -> smaller mean latency (steady full-buffer writer)
m = lambda k: good[k]["mean"]
check("mean latency falls with tlength (250 > 100 > 50 > 20 ms)", m("music-250") > m("music-100-adj") > m("game-50-adj") > m("sfx-20-adj"),
      str([m(k) for k in ("music-250", "music-100-adj", "game-50-adj", "sfx-20-adj")]))
check("adjust_latency shrinks the device buffer (50 ms: sink latency lower than without)", good["game-50-adj"]["sink"] < good["game-50-noadj"]["sink"],
      "%d vs %d" % (good["game-50-adj"]["sink"], good["game-50-noadj"]["sink"]))
sfx = [k for k, v in good.items() if v["mean"] <= 50000 and v["mx"] <= 60000 and v["starved"] * 50 <= v["n"]]
check("a SFX setting exists: mean <= 50 ms, max <= 60 ms, dry < 2 %% of samples (%s)" % ", ".join(sfx), len(sfx) > 0)
check("prebuf 0 is granted as 0, default prebuf as > 0", good["sfx-20-adj-pb0"]["pre"] == 0 and good["sfx-20-adj"]["pre"] > 0, str(good["sfx-20-adj-pb0"]))
check("minreq 5 ms is honoured as a smaller grant than the default would give (<= 5 ms)", good["sfx-20-adj-mr5"]["mr"] <= 44100 * 4 * 5 // 1000 + 4, str(good["sfx-20-adj-mr5"]))
print("%d failures" % fails)
sys.exit(1 if fails else 0)
PY

#!/usr/bin/env bash
# tools/k3net/netem_out_repro.sh -- repeat ONLY the "netem out" measurement of
# run.sh (20 % loss on Firn -> Linux frames, 256 KiB through the python echo
# server) N times and keep evidence of every failed try: a pcap taken inside the
# Linux namespace, the driver output and the echo server output.
# Needs root, ip netns, tc netem, tcpdump.
# Usage: heavy bash tools/k3net/netem_out_repro.sh [tries] [logdir]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
TRIES=${1:-30}
LOGDIR=${2:-/root/jarvis/data/staff_wt/firn/k3net-logs}
NS=${K3_NS:-k3o}
LNS="${NS}lin"; FNS="${NS}firn"; LIF="v${NS}l"; FIF="v${NS}f"
LIP=10.7.2.1; FIP=10.7.2.2; MASK=255.255.255.0
LOSSKB=${K3_LOSS_KB:-256}
mkdir -p "$LOGDIR"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-k3r.XXXXXX")
cleanup() {
    ip netns pids "$FNS" 2>/dev/null | xargs -r kill 2>/dev/null
    ip netns pids "$LNS" 2>/dev/null | xargs -r kill 2>/dev/null
    ip netns del "$LNS" 2>/dev/null; ip netns del "$FNS" 2>/dev/null
    ip link del "$LIF" 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT
settle() { awk '{s=0.9+$1/10; if (s>5) s=5; printf "%.1f", s}' /proc/loadavg; }

sed -n "/<<'PYEOF'/,/^PYEOF/p" tools/k3net/run.sh | sed '1d;$d' > "$W/echo_srv.py"
"$FIRNC" -o "$W/drv" tools/k3net/drv.fi || exit 1
ip netns add "$LNS"; ip netns add "$FNS"
ip link add "$LIF" type veth peer name "$FIF"
ip link set "$LIF" netns "$LNS"; ip link set "$FIF" netns "$FNS"
for n in "$LNS" "$FNS"; do
    ip netns exec "$n" sysctl -qw net.ipv6.conf.all.disable_ipv6=1 >/dev/null 2>&1
    ip netns exec "$n" ip link set lo up
done
ip netns exec "$LNS" ip addr add "$LIP/24" dev "$LIF"
ip netns exec "$LNS" ip link set "$LIF" up
ip netns exec "$FNS" ip link set "$FIF" up
for pair in "$LNS $LIF" "$FNS $FIF"; do
    set -- $pair
    ip netns exec "$1" ethtool -K "$2" tx off rx off tso off gso off gro off >/dev/null 2>&1
done
F() { ip netns exec "$FNS" "$W/drv" "$FIF" "$FIP" "$MASK" "$@"; }
L() { ip netns exec "$LNS" "$@"; }
val() { awk -v k="$2" '$1==k{print $2; exit}' "$1"; }

pass=0; fail=0
for i in $(seq 1 "$TRIES"); do
    P="$W/t$i"
    ip netns exec "$FNS" tc qdisc replace dev "$FIF" root netem loss 20% || { echo "no netem"; exit 1; }
    L tcpdump -i "$LIF" -n -tt -U -w "$P.pcap" >/dev/null 2>&1 &
    TD=$!
    L python3 "$W/echo_srv.py" "$LIP" 9001 tcp > "$P.py" 2>&1 &
    PY=$!
    sleep "$(settle)"
    T0=$SECONDS
    F send "$LIP" 9001 $((LOSSKB * 1024)) 90 > "$P.drv" 2>&1
    ORC=$?
    DT=$((SECONDS - T0))
    wait $PY 2>/dev/null
    sleep 0.3; kill $TD 2>/dev/null; wait $TD 2>/dev/null; for q in $(ip netns pids "$LNS"); do kill $q 2>/dev/null; done
    b=$(val "$P.drv" echoed_back); wr=$(val "$P.drv" wrong_octets)
    if [ "$ORC" -eq 0 ] && [ "${wr:-1}" = 0 ] && [ "${b:-0}" = "$((LOSSKB * 1024))" ]; then
        pass=$((pass+1)); echo "try $i OK ${DT}s"
        if [ "$DT" -gt 20 ]; then for f in "$P".*; do cp "$f" "$LOGDIR/slow_$(date +%s)_t$i.${f##*.}"; done; fi
    else
        fail=$((fail+1)); echo "try $i FAIL ${DT}s rc=$ORC back=${b:-0} wrong=${wr:-?}"
        for f in "$P".*; do cp "$f" "$LOGDIR/fail_$(date +%s)_t$i.${f##*.}"; done
    fi
    ip netns exec "$FNS" tc qdisc del dev "$FIF" root >/dev/null 2>&1
    sleep 1
done
echo "pass=$pass fail=$fail logs=$LOGDIR"

#!/usr/bin/env bash
# tools/k3net/netem_in_repro.sh -- repeat ONLY the "netem in" measurement of
# run.sh (20 % loss on Linux -> Firn frames, 256 KiB over nc) N times and keep
# evidence of every failed try: a pcap taken inside the Linux namespace, `ss -tin`
# samples of the Linux sender (RTO, backoff, retrans), the driver output and the
# kernel TCP counters. Needs root, ip netns, tc netem, tcpdump.
# Usage: heavy bash tools/k3net/netem_in_repro.sh [tries] [logdir]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
TRIES=${1:-30}
LOGDIR=${2:-/root/jarvis/data/staff_wt/firn/k3net-logs}
NS=${K3_NS:-k3r}
LNS="${NS}lin"; FNS="${NS}firn"; LIF="v${NS}l"; FIF="v${NS}f"
LIP=10.7.1.1; FIP=10.7.1.2; MASK=255.255.255.0
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
head -c $((LOSSKB * 1024)) /dev/urandom > "$W/small.bin"

pass=0; fail=0
for i in $(seq 1 "$TRIES"); do
    L tc qdisc replace dev "$LIF" root netem loss 20% || { echo "no netem"; exit 1; }
    P="$W/t$i"
    L tcpdump -i "$LIF" -n -tt -U -w "$P.pcap" >/dev/null 2>&1 &
    TD=$!
    L nstat -n >/dev/null 2>&1
    F sink 4711 $((LOSSKB * 1024)) 60 > "$P.sink" 2>&1 &
    DRV=$!
    sleep "$(settle)"
    ( for s in $(seq 1 70); do sleep 1; echo "# t=$s"; L ss -tin dst "$FIP" 2>/dev/null; done ) > "$P.ss" 2>&1 &
    SS=$!
    T0=$SECONDS
    L timeout 60 nc -q 3 "$FIP" 4711 < "$W/small.bin" > /dev/null 2>&1
    NCRC=$?
    DT=$((SECONDS - T0))
    wait $DRV 2>/dev/null; LRC=$?
    kill $SS 2>/dev/null; wait $SS 2>/dev/null
    L nstat -a 2>/dev/null | grep -E 'TcpRetrans|TcpExtTCPTimeouts|TCPLossProbes|TCPFastRetrans|TcpExtTCPSpuriousRTOs' > "$P.nstat"
    sleep 0.3; kill $TD 2>/dev/null; wait $TD 2>/dev/null
    n=$(val "$P.sink" octets); ooo=$(val "$P.sink" out_of_order)
    if [ "$LRC" -eq 0 ] && [ "${n:-0}" = "$((LOSSKB * 1024))" ]; then
        pass=$((pass+1)); echo "try $i OK ${DT}s ooo=${ooo:-0}"
        # keep the slowest successes too (> 15 s) -- near misses
        if [ "$DT" -gt 15 ]; then cp "$P".* "$LOGDIR/" 2>/dev/null; for f in "$LOGDIR"/t$i.*; do mv "$f" "${f/t$i./slow_$(date +%s)_t$i.}"; done; fi
    else
        fail=$((fail+1)); echo "try $i FAIL ${DT}s ncrc=$NCRC drvrc=$LRC octets=${n:-0} ooo=${ooo:-0}"
        for f in "$P".*; do cp "$f" "$LOGDIR/fail_$(date +%s)_t$i.${f##*.}"; done
    fi
    L tc qdisc del dev "$LIF" root >/dev/null 2>&1
    sleep 1
done
echo "pass=$pass fail=$fail logs=$LOGDIR"

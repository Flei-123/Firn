#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/k3net/run.sh -- ROUND K3: A TCP/IP STACK, MEASURED AGAINST LINUX.
#
# `lib/net/` is Ethernet, ARP, IPv4, ICMP, UDP and TCP written in Firn
# without an allocator. A stack like that needs no driver to be built or
# to be measured -- it is a function from octet sequences to octet
# sequences -- and it needs no driver to be WRONG, either. Two ends that
# this repository wrote agree perfectly on a shared misunderstanding, so
# most of what is checked here has the LINUX KERNEL on the other side.
#
#   1. THE KERNEL PROFILE. The whole library compiles under `profile
#      kernel` to a freestanding ELF object with no undefined name other
#      than `osum_panic` and not one `syscall` instruction -- in BOTH
#      compilers. That is the hard form of "it runs without an operating
#      system underneath"; the comment in the file is not evidence, the
#      object file is.
#
#   2. BOTH ENDS IN ONE PROCESS (tools/k3net/unit.fi). Fifteen cases over
#      a simulated wire that can lose, reorder and corrupt frames on
#      purpose, deterministically. Among them: a megaoctet through 5 %
#      loss and 3 % reordering, the sequence number running over 2^32
#      mid-transfer, and a bit set proving that all ELEVEN states were
#      really entered (2047), not merely described.
#
#   3. AGAINST THE LINUX KERNEL, over a veth pair in two network
#      namespaces. On one side Linux with an address, on the other an
#      interface WITHOUT one, which only tools/k3net/drv.fi answers for,
#      through `AF_PACKET`:
#        a. `ping` from Linux is answered.
#        b. `nc` pushes a megaoctet in -- throughput in MB/s.
#        c. `nc` pushes a megaoctet through the ECHO and the md5 sums are
#           compared. netcat is from 1996 and knows nothing about this.
#        d. `curl` fetches an HTTP answer and checks status line, headers
#           and Content-Length itself.
#        e. THE OTHER DIRECTION: the Firn stack CONNECTS actively to a
#           python server on Linux, pushes a megaoctet and compares what
#           comes back.
#        f. `tc netem` drops 5 % in each direction. Everything still has
#           to arrive, whole and in order.
#        g. UDP there and back against a python socket.
#
#   4. THE COUNTER-CHECKS, without which none of the above proves
#      anything:
#        * a segment with a wrong checksum HAS to be dropped (in 2, with
#          the sharp form: the same segment with the sum repaired is
#          taken);
#        * an acknowledgement for octets that were never sent HAS to be
#          refused;
#        * WITHOUT RETRANSMISSION the transfer under `netem` HAS to stay
#          incomplete -- measured against the Linux kernel, not simulated;
#        * a port nobody listens on HAS to refuse.
#
# WHY NAMESPACES AND NOT A TAP DEVICE: `/dev/net/tun` does not exist in
# the container this is measured in and cannot be created (the `tun`
# module is not in the host's kernel). `AF_PACKET` needs no device node.
# Where even that is not available the script says so and runs part 1 and
# 2 alone, rather than reporting a green it did not earn.
#
# Environment: K3_MB (default 1), K3_LOSS_KB (default 256),
# K3_NS (default k3), K3_SKIP_LINUX=1 to leave the kernel out.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}

MB=${K3_MB:-1}
LOSSKB=${K3_LOSS_KB:-256}
NS=${K3_NS:-k3}
LNS="${NS}lin"
FNS="${NS}firn"
LIF="v${NS}l"
FIF="v${NS}f"
LIP=10.7.0.1
FIP=10.7.0.2
MASK=255.255.255.0

W=$(mktemp -d /tmp/firn-k3.XXXXXX)
cleanup() {
    pkill -f "$W/drv" 2>/dev/null
    ip netns pids "$FNS" 2>/dev/null | xargs -r kill 2>/dev/null
    ip netns pids "$LNS" 2>/dev/null | xargs -r kill 2>/dev/null
    ip netns del "$LNS" 2>/dev/null
    ip netns del "$FNS" 2>/dev/null
    ip link del "$LIF" 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT

pass=0
fail=0
STEP=$SECONDS
lap() { local d=$((SECONDS - STEP)); STEP=$SECONDS; printf '%s' "$d"; }
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }
num() { printf '        %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }

# The stage 1 compiler, rebuilt when it is older than a source -- the trap
# of rounds 35/45/46: an outdated binary measures yesterday's state.
fresh=0
[ -x "$FC1" ] || fresh=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && fresh=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { fresh=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
[ "$fresh" -eq 1 ] && { rm -f "$FC1"; "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || exit 1; }

# ===================================================================
echo "== 1. the whole stack compiles in the kernel profile =="
# ===================================================================
for stage in 0 1; do
    if [ "$stage" = 0 ]; then
        "$FIRNC" -o "$W/k$stage.o" tools/k3net/kprobe.fi 2>"$W/e$stage"
    else
        "$FC1" tools/k3net/kprobe.fi -o "$W/k$stage.o" >"$W/e$stage" 2>&1
    fi
    if [ ! -f "$W/k$stage.o" ]; then
        bad "firnc$stage: tools/k3net/kprobe.fi does not compile"
        sed 's/^/        /' "$W/e$stage" | head -8
        continue
    fi
    kind=$(readelf -h "$W/k$stage.o" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "firnc$stage: ELF type REL (a freestanding object file)" \
                        || bad "firnc$stage: ELF kind '$kind', expected REL"
    undef=$(nm -u "$W/k$stage.o" 2>/dev/null | awk '{print $NF}' | sed '/^$/d' | grep -vxF osum_panic)
    [ -z "$undef" ] && ok "firnc$stage: no undefined name other than osum_panic" \
                    || { bad "firnc$stage: undefined names"; echo "$undef" | sed 's/^/        /'; }
    # No `grep -q` on a pipe here: it leaves early, the producer dies of
    # SIGPIPE and `set -o pipefail` turns that into a random failure.
    objdump -d "$W/k$stage.o" > "$W/d$stage.txt"
    if grep -qE '^\s+[0-9a-f]+:.*\bsyscall\b' "$W/d$stage.txt"; then
        bad "firnc$stage: the object file contains a syscall -- lib/net allocates after all"
    else
        ok "firnc$stage: not one syscall instruction in the machine code"
    fi
    nm --defined-only "$W/k$stage.o" > "$W/n$stage.txt"
    missing=""
    for sym in wire__checksum wire__seq_lt tcp__tcp_input tcp__tcp_pull \
               stack__net_input stack__net_output stack__net_poll; do
        grep -q "_F$stage\.$sym\$" "$W/n$stage.txt" || missing="$missing $sym"
    done
    [ -z "$missing" ] && ok "firnc$stage: the library really is in the image" \
                      || bad "firnc$stage: missing symbols:$missing"
done
SYMS=$(nm --defined-only "$W/k0.o" 2>/dev/null | grep -c '_F0\.' || echo 0)
num "symbols in the freestanding object: $SYMS"

# ===================================================================
echo "== 2. both ends in one process, over a wire that misbehaves =="
# ===================================================================
if ! "$FIRNC" -o "$W/unit" tools/k3net/unit.fi 2>"$W/eunit"; then
    bad "tools/k3net/unit.fi does not compile"
    sed 's/^/        /' "$W/eunit" | head -10
else
    "$W/unit" > "$W/unit.log" 2>&1
    urc=$?
    sed 's/^/  /' "$W/unit.log" | sed 's/^  //'
    if [ "$urc" -eq 0 ]; then
        ok "all cases of tools/k3net/unit.fi"
    else
        bad "tools/k3net/unit.fi: $urc case(s) failed"
    fi
fi

# ===================================================================
echo "== 3. against the Linux kernel, over a veth pair =="
# ===================================================================
skip_reason=""
[ "${K3_SKIP_LINUX:-0}" = 1 ] && skip_reason="K3_SKIP_LINUX=1"
[ -z "$skip_reason" ] && ! command -v ip >/dev/null 2>&1 && skip_reason="no 'ip' command"
[ -z "$skip_reason" ] && [ "$(id -u)" != 0 ] && skip_reason="not root"
if [ -z "$skip_reason" ]; then
    ip netns add "$LNS" 2>"$W/ns.err" || skip_reason="network namespaces are not available: $(head -1 "$W/ns.err")"
fi

if [ -n "$skip_reason" ]; then
    echo "  (the Linux comparison is skipped -- $skip_reason)"
    echo "  Without it parts 1 and 2 stand on their own: they prove that the"
    echo "  stack is freestanding and that it survives a bad wire, and they do"
    echo "  NOT prove that it agrees with anybody else's TCP."
else
    ip netns add "$FNS"
    ip link add "$LIF" type veth peer name "$FIF"
    ip link set "$LIF" netns "$LNS"
    ip link set "$FIF" netns "$FNS"
    for n in "$LNS" "$FNS"; do
        ip netns exec "$n" sysctl -qw net.ipv6.conf.all.disable_ipv6=1 \
            net.ipv6.conf.default.disable_ipv6=1 >/dev/null 2>&1
        ip netns exec "$n" ip link set lo up
    done
    ip netns exec "$LNS" ip addr add "$LIP/24" dev "$LIF"
    ip netns exec "$LNS" ip link set "$LIF" up
    ip netns exec "$FNS" ip link set "$FIF" up
    # Checksum offload OFF on both ends: over a veth the kernel would
    # otherwise hand out frames whose checksum it never computed, and a
    # stack that CHECKS the checksum would rightly throw all of them away.
    for pair in "$LNS $LIF" "$FNS $FIF"; do
        set -- $pair
        ip netns exec "$1" ethtool -K "$2" tx off rx off tso off gso off gro off >/dev/null 2>&1
    done

    if ! "$FIRNC" -o "$W/drv" tools/k3net/drv.fi 2>"$W/edrv"; then
        bad "tools/k3net/drv.fi does not compile"
        sed 's/^/        /' "$W/edrv" | head -10
    else
    F() { ip netns exec "$FNS" "$W/drv" "$FIF" "$FIP" "$MASK" "$@"; }
    L() { ip netns exec "$LNS" "$@"; }
    val() { awk -v k="$2" '$1==k{print $2; exit}' "$1"; }

    head -c $((MB * 1048576)) /dev/urandom > "$W/in.bin"
    WANT=$(md5sum < "$W/in.bin" | cut -d' ' -f1)
    cat > "$W/echo_srv.py" <<'PYEOF'
import socket, sys
host, port, mode = sys.argv[1], int(sys.argv[2]), sys.argv[3]
if mode == "udp":
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port)); s.settimeout(20)
    d, a = s.recvfrom(65535)
    s.sendto(d[::-1], a)
    print("linux_udp", len(d)); s.close()
else:
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((host, port)); s.listen(4); s.settimeout(95)
    c, a = s.accept(); c.settimeout(95)
    n = 0
    try:
        while True:
            d = c.recv(65536)
            if not d: break
            c.sendall(d); n += len(d)
    except Exception as e:
        print("linux_err", e)
    try: c.shutdown(socket.SHUT_WR)
    except Exception: pass
    c.close(); s.close()
    print("linux_echoed", n)
PYEOF

    # --- 3a. ping ------------------------------------------------------
    F idle 6 > "$W/idle.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L ping -c 5 -i 0.25 -W 2 "$FIP" > "$W/ping.out" 2>&1
    PRC=$?
    wait $DRV 2>/dev/null
    got=$(awk -F', ' '/packets transmitted/{print $2}' "$W/ping.out" | awk '{print $1}')
    rtt=$(awk -F'= ' '/rtt min/{print $2}' "$W/ping.out")
    ans=$(val "$W/idle.out" icmp_echo_answered)
    if [ "$PRC" -eq 0 ] && [ "${got:-0}" = 5 ] && [ "${ans:-0}" = 5 ]; then
        ok "ping from Linux: 5 of 5 answered by the Firn stack"
        num "rtt (ms) $rtt"
    else
        bad "ping: $got received, the stack answered ${ans:-0}"
        sed 's/^/        /' "$W/ping.out" | tail -4
    fi
    arpr=$(val "$W/idle.out" arp_replies)
    [ "${arpr:-0}" -ge 1 ] && ok "ARP: Linux asked, the Firn stack answered ($arpr)" \
                           || bad "ARP: no reply went out"

    # --- 3b. nc pushes a megaoctet IN ----------------------------------
    F sink 4711 $((MB * 1048576)) 45 > "$W/sink.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L timeout 50 nc -q 2 "$FIP" 4711 < "$W/in.bin" > /dev/null 2>&1
    wait $DRV 2>/dev/null
    SRC=$?
    n=$(val "$W/sink.out" octets)
    kbs=$(val "$W/sink.out" kb_per_s)
    if [ "$SRC" -eq 0 ] && [ "${n:-0}" = "$((MB * 1048576))" ]; then
        ok "nc pushes $MB MiB into the Firn stack, and it closes cleanly"
        num "throughput $((kbs / 1024)) MB/s ($kbs KB/s), frames in $(val "$W/sink.out" frames_in), acknowledgements out $(val "$W/sink.out" frames_out)"
    else
        bad "nc -> stack: ${n:-0} of $((MB * 1048576)) octets"
        sed 's/^/        /' "$W/sink.out"
    fi

    # --- 3c. nc through the ECHO, md5 ----------------------------------
    F serve 4711 $((MB * 1048576)) 45 > "$W/serve.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L timeout 50 nc -q 3 "$FIP" 4711 < "$W/in.bin" > "$W/out.bin" 2>/dev/null
    wait $DRV 2>/dev/null
    GOT=$(md5sum < "$W/out.bin" | cut -d' ' -f1)
    SIZE=$(stat -c%s "$W/out.bin")
    if [ "$GOT" = "$WANT" ]; then
        ok "nc through the echo: $SIZE octets there and back, md5 identical"
        num "md5 $WANT, throughput $(( $(val "$W/serve.out" kb_per_s) / 1024 )) MB/s"
    else
        bad "echo: $SIZE octets back, md5 $GOT instead of $WANT"
        sed 's/^/        /' "$W/serve.out"
    fi

    # --- 3d. curl ------------------------------------------------------
    F http 8080 20 > "$W/http.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L curl -sS --max-time 15 -D "$W/hdr.txt" "http://$FIP:8080/hello" > "$W/body.txt" 2>"$W/curl.err"
    CRC=$?
    wait $DRV 2>/dev/null
    if [ "$CRC" -eq 0 ] && grep -q '^HTTP/1.1 200 OK' "$W/hdr.txt" \
        && grep -qi '^Content-Length: 40' "$W/hdr.txt" \
        && [ "$(stat -c%s "$W/body.txt")" = 40 ]; then
        ok "curl fetches an HTTP answer: status line, headers, Content-Length"
        num "body: $(tr -d '\n' < "$W/body.txt")"
    else
        bad "curl: exit $CRC"
        sed 's/^/        /' "$W/hdr.txt" "$W/curl.err" 2>/dev/null | head -8
    fi

    # --- 3e. the Firn stack connects ACTIVELY --------------------------
    L python3 "$W/echo_srv.py" "$LIP" 9000 tcp > "$W/py1.out" 2>&1 &
    PY=$!
    sleep 0.9
    F send "$LIP" 9000 $((MB * 1048576)) 40 > "$W/send.out" 2>&1
    ERC=$?
    wait $PY 2>/dev/null
    s=$(val "$W/send.out" sent)
    b=$(val "$W/send.out" echoed_back)
    wr=$(val "$W/send.out" wrong_octets)
    if [ "$ERC" -eq 0 ] && [ "${wr:-1}" = 0 ] && [ "${b:-0}" = "$((MB * 1048576))" ]; then
        ok "the Firn stack opens the connection to Linux and gets its $MB MiB back"
        num "sent $s, back $b, wrong 0, $(grep linux_echoed "$W/py1.out" || true)"
    else
        bad "active connection: sent ${s:-0}, back ${b:-0}, wrong ${wr:-?}"
        sed 's/^/        /' "$W/send.out" "$W/py1.out" 2>/dev/null | head -12
    fi

    # --- 3f. tc netem, both directions --------------------------------
    #
    # THE TIME LIMITS BELOW ARE GENEROUS ON PURPOSE. Every mode of
    # `drv.fi` leaves its loop the moment the connection is properly
    # closed, so a limit of ninety seconds costs ninety seconds only when
    # something is really wrong. A tight limit turns a busy machine into a
    # red test, which is the worst kind of flake there is.
    #
    # A NOTE ON netem's OWN COUNTER, because it is a trap: the `dropped`
    # number in `tc -s qdisc show` does NOT count the frames the loss
    # model threw away -- it counts the ones the queue could not hold. At
    # `loss 20 %` over 185 frames it reads 2. So it is printed for
    # information and NOT used as the gate. What is used as the gate is
    # what the loss actually causes and what this round is about: holes in
    # the receive stream that had to be reassembled, and retransmissions
    # that had to go out. Both are counters inside lib/net/, both are zero
    # on a clean wire, and the runs above prove they are (out_of_order 0,
    # rexmit 0 without netem).
    netem_drops() { # namespace interface
        ip netns exec "$1" tc -s qdisc show dev "$2" 2>/dev/null \
            | tr ',()' '   ' | awk '/dropped/{for(i=1;i<=NF;i++) if($i=="dropped"){print $(i+1)+0; exit}}'
    }
    head -c $((LOSSKB * 1024)) "$W/in.bin" > "$W/small.bin"

    if ! L tc qdisc add dev "$LIF" root netem loss 20% 2>"$W/tc1.err"; then
        echo "  (tc netem is not available: $(head -1 "$W/tc1.err") -- the loss measurement is skipped)"
    else
        STEP=$SECONDS
        F sink 4711 $((LOSSKB * 1024)) 90 > "$W/lossin.out" 2>&1 &
        DRV=$!
        sleep 0.7
        L timeout 90 nc -q 3 "$FIP" 4711 < "$W/small.bin" > /dev/null 2>&1
        wait $DRV 2>/dev/null
        LRC=$?
        DIN=$(netem_drops "$LNS" "$LIF")
        L tc qdisc del dev "$LIF" root >/dev/null 2>&1
        n=$(val "$W/lossin.out" octets)
        ooo=$(val "$W/lossin.out" out_of_order)
        if [ "$LRC" -eq 0 ] && [ "${n:-0}" = "$((LOSSKB * 1024))" ] && [ "${ooo:-0}" -gt 0 ]; then
            ok "20 % of the frames from Linux dropped: all $LOSSKB KiB arrive, in order"
            num "out-of-order segments reassembled: $ooo, throughput $(val "$W/lossin.out" kb_per_s) KB/s against $(val "$W/sink.out" kb_per_s) KB/s on a clean wire, $(lap) s"
        else
            bad "netem in: ${n:-0} of $((LOSSKB * 1024)) octets, out of order ${ooo:-0} (netem queue drops ${DIN:-0})"
            sed 's/^/        /' "$W/lossin.out"
        fi

        ip netns exec "$FNS" tc qdisc add dev "$FIF" root netem loss 20% >/dev/null 2>&1
        L python3 "$W/echo_srv.py" "$LIP" 9001 tcp > "$W/py2.out" 2>&1 &
        PY=$!
        sleep 0.9
        STEP=$SECONDS
        F send "$LIP" 9001 $((LOSSKB * 1024)) 90 > "$W/lossout.out" 2>&1
        ORC=$?
        wait $PY 2>/dev/null
        DOUT=$(netem_drops "$FNS" "$FIF")
        b=$(val "$W/lossout.out" echoed_back)
        wr=$(val "$W/lossout.out" wrong_octets)
        rex=$(val "$W/lossout.out" rexmit)
        frex=$(val "$W/lossout.out" fast_rexmit)
        tot=$(( ${rex:-0} + ${frex:-0} ))
        if [ "$ORC" -eq 0 ] && [ "${wr:-1}" = 0 ] && [ "${b:-0}" = "$((LOSSKB * 1024))" ] && [ "$tot" -gt 0 ]; then
            ok "20 % of OUR frames dropped: the stack sends them again and everything arrives"
            num "retransmissions: $rex on the timer, $frex on three duplicate acknowledgements, $(lap) s"
        else
            bad "netem out: back ${b:-0} of $((LOSSKB * 1024)), wrong ${wr:-?}, retransmissions $tot (netem queue drops ${DOUT:-0})"
            sed 's/^/        /' "$W/lossout.out" "$W/py2.out" 2>/dev/null | head -12
        fi

        # THE COUNTER-CHECK, and it is the sharpest one in this file: the
        # very same run with retransmission switched off HAS to fail.
        L python3 "$W/echo_srv.py" "$LIP" 9002 tcp > "$W/py3.out" 2>&1 &
        PY=$!
        sleep 0.9
        F send "$LIP" 9002 $((LOSSKB * 1024)) 15 norexmit > "$W/norex.out" 2>&1
        NRC=$?
        wait $PY 2>/dev/null
        ip netns exec "$FNS" tc qdisc del dev "$FIF" root >/dev/null 2>&1
        nb=$(val "$W/norex.out" echoed_back)
        if [ "$NRC" -ne 0 ] && [ "${nb:-0}" -lt "$((LOSSKB * 1024))" ]; then
            ok "COUNTER-CHECK: without retransmission only ${nb:-0} of $((LOSSKB * 1024)) octets arrive"
        else
            bad "the counter-check does NOT strike: ${nb:-0} octets came through WITHOUT retransmission -- then the test above measured nothing"
            sed 's/^/        /' "$W/norex.out"
        fi
    fi

    # --- 3g. UDP there and back against a python socket ---------------
    F udpecho 53 6 > "$W/udp.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L python3 -c "
import socket, hashlib
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.settimeout(4)
payload = bytes((i * 37 + 11) & 255 for i in range(1400))
s.sendto(payload, ('$FIP', 53))
try:
    d, a = s.recvfrom(4096)
    print('udp_back', len(d), 'reversed_ok', int(d == payload[::-1]), 'from', a[0])
except Exception as e:
    print('udp_back 0 reversed_ok 0 err', e)
" > "$W/udpcli.out" 2>&1
    wait $DRV 2>/dev/null
    ub=$(awk '/^udp_back/{print $2}' "$W/udpcli.out")
    ur=$(awk '/^udp_back/{print $4}' "$W/udpcli.out")
    ua=$(val "$W/udp.out" udp_answered)
    if [ "${ub:-0}" = 1400 ] && [ "${ur:-0}" = 1 ] && [ "${ua:-0}" -ge 1 ]; then
        ok "UDP: 1400 octets to the Firn stack and back, reversed, checksums intact"
        num "$(cat "$W/udpcli.out"), datagrams seen $(val "$W/udp.out" udp_datagrams)"
    else
        bad "UDP: ${ub:-0} octets back, reversed ${ur:-0}"
        sed 's/^/        /' "$W/udp.out" "$W/udpcli.out" 2>/dev/null | head -8
    fi

    # --- 3h. a port nobody listens on ----------------------------------
    F idle 4 > "$W/refuse.out" 2>&1 &
    DRV=$!
    sleep 0.7
    L timeout 3 nc -w 2 "$FIP" 4999 < /dev/null > /dev/null 2>&1
    RRC=$?
    wait $DRV 2>/dev/null
    if [ "$RRC" -ne 0 ]; then
        ok "COUNTER-CHECK: a port nobody listens on refuses the connection"
    else
        bad "a closed port ACCEPTED a connection"
    fi
    fi
fi

echo
if [ "$fail" -eq 0 ]; then
    echo "K3NET: $pass proofs, 0 failures"
    exit 0
fi
echo "K3NET: $fail of $((pass + fail)) failed"
exit 1

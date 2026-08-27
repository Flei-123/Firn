#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/mcserver/run.sh -- does a real client get into the world? (round 76)
#
# THE POINT OF THIS SCRIPT IS THAT IT DOES NOT BELIEVE THE SERVER. It starts
# `demos/mcserver`, lets THREE clients loose on it and checks what comes
# back, field by field:
#
#   1. `tools/mcserver/harness.py ping` -- the server list ping. Version,
#      protocol number, player count, MOTD out of the JSON, and the Pong
#      that has to carry the same eight octets back.
#   2. `harness.py login` -- the whole handshake through to the world:
#      Login Success with a UUID that has to be version 3 and variant
#      RFC 4122, the configuration state with the registries, Join Game,
#      the chunks (whose sections have to consume EXACTLY the announced
#      octets), the position packet that ends the loading screen, and a
#      Keep Alive.
#   3. `harness.py dribble` -- the same login, but every octet in its own
#      `write` with half a millisecond in between. A server that treats one
#      `read` as one packet works on localhost and dies here. This is the
#      test that separates a length prefixed reader from a hopeful one.
#   4. `node tools/mcserver/nmp_client.cjs` -- node-minecraft-protocol, an
#      implementation nobody here wrote, with its own packet definitions out
#      of `minecraft-data`. It validates every field against them and throws
#      when something does not fit. If it reaches the world, the packets are
#      right and not merely self consistent.
#   5. sixteen logins AT THE SAME TIME -- sixteen threads, sixteen worlds,
#      about 22 MiB of chunk data. `Map` and the allocator under pressure.
#
# And the counter-checks: garbage on the socket must not crash the server,
# and a client that vanishes in the middle must not take it with it.
#
# `node` is optional. Without it point 4 is SKIPPED and said so -- not
# silently passed.
set -uo pipefail
set +m
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
W=$(mktemp -d /tmp/firn-mc.XXXXXX)
SRV=""
cleanup() { [ -n "$SRV" ] && kill -9 "$SRV" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$(pwd)/lib"

# NOTE: this runs in a SUBSHELL (`PORT=$(start_server ...)`), so a variable
# assigned in here does not reach the caller. The pid therefore goes through
# a file. That cost half an hour once; it is written down so it costs
# nobody a second one.
start_server() {                       # $1 = binary, $2 = seconds
    "$1" 0 2 "$2" > "$W/srv.log" 2>&1 &
    echo $! > "$W/srv.pid"
    local i=0 p=""
    while [ $i -lt 200 ]; do
        p=$(awk '/^mcserver: listening on /{print $4; exit}' "$W/srv.log" 2>/dev/null)
        [ -n "$p" ] && { echo "$p"; return 0; }
        sleep 0.05
        i=$((i + 1))
    done
    return 1
}

STAGES="release-fast: no-opt:--no-opt dev-fast:--opt-level=dev-fast"
[ "${MC_FAST:-0}" = "1" ] && STAGES="release-fast:"

for stage in $STAGES; do
    name=${stage%%:*}
    opt=${stage#*:}
    BIN="$W/mcserver.$name"
    if ! $FIRNC $opt -o "$BIN" demos/mcserver/main.fi 2>"$W/err"; then
        report "$name: demos/mcserver does not compile"
        sed 's/^/        /' "$W/err" | head -10
        continue
    fi

    PORT=$(start_server "$BIN" 300) || { report "$name: the server did not start"; continue; }
    SRV=$(cat "$W/srv.pid")
    echo "  $name: server on port $PORT (pid $SRV)"

    # --- 1. ping -----------------------------------------------------------
    if OUT=$(timeout 60 python3 tools/mcserver/harness.py ping 127.0.0.1 "$PORT" 2>&1); then
        echo "$OUT" | grep -E '^ping:' | sed 's/^/  '"$name"': /'
    else
        report "$name: ping"
        echo "$OUT" | sed 's/^/        /' | head -6
    fi

    # --- 2. login ----------------------------------------------------------
    if OUT=$(timeout 120 python3 tools/mcserver/harness.py login 127.0.0.1 "$PORT" Notch 2>&1); then
        echo "$OUT" | grep -E '^(login|config: registry_|config: Registry|play:|OK)' \
            | sed 's/^/  '"$name"': /'
    else
        report "$name: login"
        echo "$OUT" | sed 's/^/        /' | head -20
    fi
    # The UUID has to be the one the VANILLA server derives for the same
    # name -- captured on 2026-08-21 from `java -jar server.jar` 1.20.4.
    if ! echo "$OUT" | grep -q 'b50ad385-829d-3141-a216-7e7d7539ba7f'; then
        report "$name: the offline UUID for 'Notch' is not the one vanilla derives"
    else
        echo "  $name: the UUID for 'Notch' is identical to the vanilla one"
    fi

    # --- 3. one octet per write --------------------------------------------
    if OUT=$(timeout 120 python3 tools/mcserver/harness.py dribble 127.0.0.1 "$PORT" Drib 2>&1); then
        echo "$OUT" | grep -E '^OK' | sed 's/^/  '"$name"': dribbled, /'
    else
        report "$name: the login did not survive being dribbled out octet by octet"
        echo "$OUT" | sed 's/^/        /' | head -10
    fi

    # --- 4. node-minecraft-protocol -----------------------------------------
    # node_modules is NOT in the repository (453 MiB of minecraft-data). If
    # it is missing, one attempt to fetch it -- and if that fails too, the
    # point is SKIPPED and said so out loud, not silently passed.
    if [ ! -d tools/mcserver/node_modules/minecraft-protocol ] && command -v npm >/dev/null; then
        timeout 300 npm --prefix tools/mcserver install --no-audit --no-fund \
            minecraft-protocol >"$W/npm.log" 2>&1 || true
    fi
    if [ -d tools/mcserver/node_modules/minecraft-protocol ]; then
        NODE_PATH=tools/mcserver/node_modules
        export NODE_PATH
        if OUT=$(timeout 120 node tools/mcserver/nmp_client.cjs 127.0.0.1 "$PORT" NmpBot 2>&1); then
            echo "$OUT" | grep -E '^(nmp|OK)' | sed 's/^/  '"$name"': /'
        else
            report "$name: node-minecraft-protocol did not get into the world"
            echo "$OUT" | sed 's/^/        /' | head -12
        fi
    else
        echo "  $name: SKIPPED node-minecraft-protocol (tools/mcserver/node_modules missing;"
        echo "  $name:         npm --prefix tools/mcserver install minecraft-protocol)"
    fi

    # --- 5. sixteen at the same time ----------------------------------------
    if OUT=$(timeout 180 python3 tools/mcserver/harness.py flood 127.0.0.1 "$PORT" 16 2>&1); then
        echo "$OUT" | grep -E '^flood:' | sed 's/^/  '"$name"': /'
    else
        report "$name: sixteen logins at the same time"
        echo "$OUT" | grep -E 'FAIL|flood' | sed 's/^/        /' | head -6
    fi

    # --- 6. the endurance run, and the two counter-checks ------------------
    # Firn has no destructors. Every buffer and every socket is released by
    # hand, and a server is exactly the place where forgetting one is
    # invisible for a week. So the RSS of the process is read out of /proc
    # while thousands of connections run through it.
    SPID="$SRV"
    if [ -n "$SPID" ] && [ -r "/proc/$SPID/status" ]; then
        if OUT=$(timeout 300 python3 tools/mcserver/soak.py "$PORT" "$SPID" \
                "${MC_SOAK_PINGS:-3000}" "${MC_SOAK_LOGINS:-400}" 2>&1); then
            echo "$OUT" | sed 's/^/  '"$name"': /'
        else
            report "$name: the endurance run"
            echo "$OUT" | sed 's/^/        /' | head -8
        fi
    else
        report "$name: the pid of the server was not found (endurance run skipped)"
    fi

    # COUNTER-CHECK A: without `reap()` the server HAS to stop at the 64th
    # connection. Without this the endurance run above would prove nothing --
    # it would pass with a server that leaks thread table entries, as long as
    # the leak is not RSS. This is the bug round 76 found.
    sed 's/^                reap(base)$//' demos/mcserver/main.fi > "$W/noreap.fi"
    cp demos/mcserver/proto.fi demos/mcserver/registry.fi demos/mcserver/world.fi "$W/"
    if $FIRNC $opt -o "$W/noreap" "$W/noreap.fi" 2>"$W/err"; then
        "$W/noreap" 0 1 60 > "$W/noreap.log" 2>&1 &
        NRP=$!
        NRPORT=""
        i=0
        while [ $i -lt 200 ]; do
            NRPORT=$(awk '/^mcserver: listening on /{print $4; exit}' "$W/noreap.log" 2>/dev/null)
            [ -n "$NRPORT" ] && break
            sleep 0.05; i=$((i + 1))
        done
        NRC=$(timeout 60 python3 tools/mcserver/soak.py "$NRPORT" "$NRP" 300 1 2>&1 \
              | grep -c 'FAILED at connection 6[0-9]')
        kill -9 $NRP 2>/dev/null
        wait $NRP 2>/dev/null
        if [ "$NRC" -ge 1 ]; then
            echo "  $name: counter-check -- without reap() the server dies in the sixties, as it must"
        else
            report "$name: WITHOUT reap() the server did NOT die -- the endurance run proves nothing"
        fi
    else
        report "$name: the counter-check build (no reap) does not compile"
    fi

    # COUNTER-CHECK B: without the `bytes_free` the RSS HAS to climb.
    sed 's/^    bytes.bytes_free(&\(body\|frame\|scratch\|json\))$//' \
        demos/mcserver/main.fi > "$W/leak.fi"
    if $FIRNC $opt -o "$W/leak" "$W/leak.fi" 2>"$W/err"; then
        "$W/leak" 0 1 60 > "$W/leak.log" 2>&1 &
        LKP=$!
        LKPORT=""
        i=0
        while [ $i -lt 200 ]; do
            LKPORT=$(awk '/^mcserver: listening on /{print $4; exit}' "$W/leak.log" 2>/dev/null)
            [ -n "$LKPORT" ] && break
            sleep 0.05; i=$((i + 1))
        done
        LOUT=$(timeout 120 python3 tools/mcserver/soak.py "$LKPORT" "$LKP" 1500 200 2>&1)
        kill -9 $LKP 2>/dev/null
        wait $LKP 2>/dev/null
        if echo "$LOUT" | grep -q 'that is a leak'; then
            echo "  $name: counter-check -- without bytes_free the RSS climbs, as it must"
            echo "$LOUT" | grep -E '^(ping|login) ' | sed 's/^/  '"$name"':     /'
        else
            report "$name: WITHOUT bytes_free the RSS stayed flat -- the measurement is broken"
            echo "$LOUT" | sed 's/^/        /' | head -6
        fi
    else
        report "$name: the counter-check build (leak) does not compile"
    fi

    # --- the counter-checks --------------------------------------------------
    # a) garbage instead of a handshake
    for junk in 'ff ff ff ff ff ff' '00' 'ff ff ff ff 7f 41 41 41'; do
        python3 - "$PORT" "$junk" <<'PY' 2>/dev/null
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), 5)
s.sendall(bytes(int(x, 16) for x in sys.argv[2].split()))
s.settimeout(3)
try:
    s.recv(64)
except OSError:
    pass
s.close()
PY
    done
    # b) a client that vanishes in the middle of the login
    python3 - "$PORT" <<'PY' 2>/dev/null
import socket, struct, sys
def vi(n):
    n &= 0xFFFFFFFF; o = bytearray()
    while True:
        b = n & 0x7F; n >>= 7
        o.append(b | 0x80 if n else b)
        if not n:
            return bytes(o)
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), 5)
p = vi(0) + vi(765) + vi(9) + b"localhost" + struct.pack(">H", 25565) + vi(2)
s.sendall(vi(len(p)) + p)
s.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
s.close()   # RST in the middle of the login
PY
    sleep 0.5
    # after all that the server still has to answer
    if timeout 30 python3 tools/mcserver/harness.py ping 127.0.0.1 "$PORT" >/dev/null 2>&1; then
        echo "  $name: counter-checks -- garbage and a torn connection survived, still answering"
    else
        report "$name: the server did not survive the counter-checks"
    fi

    kill -9 "$SRV" 2>/dev/null
    wait "$SRV" 2>/dev/null
    SRV=""
done

if [ "$ERRORS" -eq 0 ]; then
    echo "RESULT mcserver: ok"
    exit 0
fi
echo "RESULT mcserver: $ERRORS errors"
exit 1

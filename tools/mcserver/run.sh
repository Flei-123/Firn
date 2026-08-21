#!/usr/bin/env bash
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

start_server() {                       # $1 = binary, $2 = seconds
    "$1" 0 2 "$2" > "$W/srv.log" 2>&1 &
    SRV=$!
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

    PORT=$(start_server "$BIN" 180) || { report "$name: the server did not start"; continue; }
    echo "  $name: server on port $PORT"

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

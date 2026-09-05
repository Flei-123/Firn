#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/net/run.sh -- the sockets against the OUTSIDE (round 76).
#
# `tests/1600_net_echo.fi` runs in section 3 of test.sh and pushes 1 MiB
# between a server thread and a client IN THE SAME PROCESS. That is
# necessary and it is not enough: both ends are this repository, and two
# ends that misunderstand the same thing agree perfectly.
#
# Here the other end is somebody else's:
#
#   1. `nc` pushes 1 MiB of random octets through the echo server and the
#      checksums are compared. netcat is from 1996 and knows nothing about
#      this repository.
#   2. `curl` fetches an HTTP answer. curl checks the status line, the
#      headers and Content-Length; if the answer is malformed it says so.
#   3. THROUGHPUT, measured, not claimed: how many MiB/s go through the
#      loopback there and back.
#   4. SIXTEEN connections AT THE SAME TIME -- one thread per connection,
#      and all sixteen have to get their own data back.
#
# And the counter-checks, without which the rest proves nothing:
#   * a connection to a port on which nothing listens has to FAIL,
#   * a server that is killed in the middle must not leave the client
#     hanging,
#   * the process must survive a write to a peer that is gone (MSG_NOSIGNAL).
#
# All of it in THREE build stages.
set -uo pipefail
set +m
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
W=$(mktemp -d /tmp/firn-net.XXXXXX)
trap 'rm -rf "$W"; jobs -p | xargs -r kill 2>/dev/null' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$(pwd)/lib"

MB=${NET_MB:-1}
head -c $((MB * 1048576)) /dev/urandom > "$W/in.bin"
WANT=$(md5sum < "$W/in.bin" | cut -d' ' -f1)

# Waits for the port line the server prints, at most 10 s.
wait_port() {
    local f="$1" i=0 p=""
    while [ $i -lt 200 ]; do
        p=$(awk '/^port /{print $2; exit}' "$f" 2>/dev/null)
        [ -n "$p" ] && { echo "$p"; return 0; }
        sleep 0.05
        i=$((i + 1))
    done
    return 1
}

for stage in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}
    BIN="$W/echo.$name"
    if ! $FIRNC $opt -o "$BIN" tools/net/echo.fi 2>"$W/err"; then
        report "$name: tools/net/echo.fi does not compile"
        sed 's/^/        /' "$W/err" | head -8
        continue
    fi

    # --- 1. nc ------------------------------------------------------------
    "$BIN" 0 1 > "$W/e1.log" 2>&1 &
    SRV=$!
    PORT=$(wait_port "$W/e1.log") || { report "$name: the server printed no port"; kill $SRV 2>/dev/null; continue; }
    if ! timeout 60 nc -q 2 127.0.0.1 "$PORT" < "$W/in.bin" > "$W/out.bin" 2>/dev/null; then
        report "$name: nc failed"
    fi
    wait $SRV 2>/dev/null
    GOT=$(md5sum < "$W/out.bin" | cut -d' ' -f1)
    SIZE=$(stat -c%s "$W/out.bin")
    if [ "$GOT" = "$WANT" ]; then
        echo "  $name: nc  $SIZE octets there and back, md5 identical ($WANT)"
    else
        report "$name: nc got $SIZE octets, md5 $GOT instead of $WANT"
    fi

    # --- 2. curl ----------------------------------------------------------
    "$BIN" 0 1 http > "$W/e2.log" 2>&1 &
    SRV=$!
    PORT=$(wait_port "$W/e2.log") || { report "$name: no port (http)"; kill $SRV 2>/dev/null; continue; }
    CURL=$(curl -sS --max-time 20 -w '%{http_code} %{size_download}' \
        -o "$W/curl.body" "http://127.0.0.1:$PORT/" 2>"$W/curl.err")
    wait $SRV 2>/dev/null
    if [ "$CURL" = "200 24" ] && [ "$(cat "$W/curl.body")" = "a socket written in Firn" ]; then
        echo "  $name: curl HTTP 200, 24 octets, body as expected"
    else
        report "$name: curl said '$CURL' ($(head -c 100 "$W/curl.err"))"
    fi

    # --- 3./4. throughput and sixteen at the same time --------------------
    "$BIN" 0 16 > "$W/e3.log" 2>&1 &
    SRV=$!
    PORT=$(wait_port "$W/e3.log") || { report "$name: no port (load)"; kill $SRV 2>/dev/null; continue; }
    python3 tools/net/load.py "$PORT" 16 "$MB" > "$W/load.txt" 2>&1
    LRC=$?
    wait $SRV 2>/dev/null
    if [ $LRC -eq 0 ]; then
        sed 's/^/  '"$name"': /' "$W/load.txt"
    else
        report "$name: the load test failed"
        sed 's/^/        /' "$W/load.txt" | head -8
    fi

    # --- the counter-checks ------------------------------------------------
    # a) nothing listens there
    if timeout 5 nc -w 2 -z 127.0.0.1 1 2>/dev/null; then
        report "$name: a connection to port 1 SUCCEEDED"
    fi
    # b) the server is gone in the middle of the transfer -- the client has
    #    to come back, not hang. tests/1500 checks the same thing from the
    #    other side (a write to a gone peer must not raise SIGPIPE).
    "$BIN" 0 1 > "$W/e4.log" 2>&1 &
    SRV=$!
    PORT=$(wait_port "$W/e4.log") || { report "$name: no port (kill)"; kill $SRV 2>/dev/null; continue; }
    ( sleep 0.3; kill -9 $SRV 2>/dev/null ) &
    timeout 20 python3 - "$PORT" <<'PY'
import socket, sys, time
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), 10)
s.settimeout(10)
data = b"x" * 65536
try:
    for _ in range(400):
        s.sendall(data)
        s.recv(65536)
        time.sleep(0.002)
except (BrokenPipeError, ConnectionResetError, OSError):
    pass
sys.exit(0)
PY
    if [ $? -ne 0 ]; then
        report "$name: the client hung when the server was killed"
    else
        echo "  $name: counter-checks (closed port refuses, killed server does not hang)"
    fi
    wait 2>/dev/null
done

if [ "$ERRORS" -eq 0 ]; then
    echo "RESULT net: ok"
    exit 0
fi
echo "RESULT net: $ERRORS errors"
exit 1

#!/usr/bin/env bash
cd "$(dirname "$0")/../.." || exit 2
export FIRNLIB="$PWD/lib"
FIRNC="${FIRNC:-$PWD/compiler/target/release/firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"; [ -n "$XPID" ] && kill "$XPID" 2>/dev/null; [ -n "$PPID2" ] && kill "$PPID2" 2>/dev/null' EXIT
command -v Xvfb >/dev/null 2>&1 || { echo "SKIP: no Xvfb"; exit 0; }
command -v xdotool >/dev/null 2>&1 || { echo "SKIP: no xdotool"; exit 0; }
"$FIRNC" --no-cache --opt-level=dev-fast -o "$D/focus" tools/window/focus_main.fi || exit 1
N=180
while [ -e "/tmp/.X$N-lock" ]; do N=$((N+1)); done
Xvfb ":$N" -screen 0 640x480x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=":$N"
"$D/focus" >"$D/out" 2>&1 &
PPID2=$!
for _ in $(seq 100); do grep -q '^H ' "$D/out" && break; sleep 0.1; done
read -r _ A B < <(grep '^H ' "$D/out")
sleep 0.5
xdotool windowfocus "$A"; sleep 0.4
xdotool windowfocus "$B"; sleep 0.4
xdotool windowfocus "$A"; sleep 0.4
wait "$PPID2"
RC=$?
PPID2=
cat "$D/out"
exit $RC

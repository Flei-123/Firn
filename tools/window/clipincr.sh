#!/usr/bin/env bash
cd "$(dirname "$0")/../.." || exit 2
export FIRNLIB="$PWD/lib"
FIRNC="${FIRNC:-$PWD/compiler/target/release/firnc}"
D=$(mktemp -d)
OWNER=
XPID=
trap 'rm -rf "$D"; [ -n "$OWNER" ] && kill "$OWNER" 2>/dev/null; [ -n "$XPID" ] && kill "$XPID" 2>/dev/null' EXIT
command -v Xvfb >/dev/null 2>&1 || { echo "SKIP: no Xvfb"; exit 0; }
FIRN_NO_CACHE=1 "$FIRNC" --opt-level=dev-fast -o "$D/clipincr" tools/window/clipincr_main.fi || exit 1
printf 'hello w\xc3\xb6rld \xe2\x82\xac\nsecond line' >"$D/text"
printf '<b>bold</b> <i>\xc3\xa9</i><table><tr><td>1</td></tr></table>' >"$D/html"
head -c 1470878 /dev/urandom >"$D/png"
N=190
while [ -e "/tmp/.X$N-lock" ]; do N=$((N+1)); done
Xvfb ":$N" -screen 0 640x480x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=":$N"
"$D/clipincr" serve "$D/text" "$D/html" "$D/png" x >"$D/owner.out" 2>&1 &
OWNER=$!
for _ in $(seq 50); do grep -q READY "$D/owner.out" && break; sleep 0.1; done
grep -q READY "$D/owner.out" || { echo "FAIL owner did not start"; cat "$D/owner.out"; exit 1; }
sleep 0.3
mkdir "$D/got"
timeout 60 "$D/clipincr" get "$D/got/types" "$D/got/text" "$D/got/html" "$D/got/png" >"$D/get.out" 2>&1
BAD=0
check() { if [ "$2" = 0 ]; then echo "ok   $1"; else echo "FAIL $1"; BAD=1; fi; }
grep -q DONE "$D/get.out"; check "getter finished" $?
grep -q 'text/html' "$D/got/types" && grep -q 'image/png' "$D/got/types"; check "types list html and png" $?
cmp -s "$D/text" "$D/got/text"; check "text exact" $?
cmp -s "$D/html" "$D/got/html"; check "html exact" $?
cmp -s "$D/png" "$D/got/png"; RC=$?
GOT=$(stat -c %s "$D/got/png" 2>/dev/null || echo 0)
check "png exact via INCR ($GOT of 1470878 octets)" $RC
[ "$BAD" = 0 ] && echo "PASS clipincr" || echo "FAIL clipincr"
exit $BAD

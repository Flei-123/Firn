#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export TOOL_FIRNC=$FIRNC
D=$(mktemp -d)
SERVER_PID=
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null; rm -rf "$D"; }
trap cleanup EXIT
PUB=$D/pub
export FIRN_HOME=$D/home
unset FIRNLIB FIRN_TOOLCHAIN
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL $1"; }
expect() { local what=$1; shift; if "$@" >"$D/out" 2>&1; then ok "$what"; else bad "$what"; sed 's/^/     /' "$D/out" | head -5; fi; }
expect_fail() { local what=$1; shift; if "$@" >"$D/out" 2>&1; then bad "$what (succeeded)"; else ok "$what"; fi; }
has() { grep -q -- "$2" "$D/out" && ok "$1" || { bad "$1"; sed 's/^/     /' "$D/out" | head -5; }; }
publish() { bash tools/release/publish.sh --out "$PUB" --channel stable --gate-stamp "$D/stamp" --firnc "${2:-$FIRNC}" --lib-dirs "std rt rc" --version "$1" ${3:-} >"$D/pub.log" 2>&1 || { cat "$D/pub.log"; return 1; }; }
current() { readlink "$FIRN_HOME/current" | sed 's#.*/##'; }
printf 'commit=%s\nresult=green\n' "$(git rev-parse HEAD)" > "$D/stamp"
F=$FIRN_HOME/bin/firn
FC=$FIRN_HOME/bin/firnc

expect "publish 0.1.1" publish 0.1.1
expect_fail "stable without green stamp is refused" bash tools/release/publish.sh --out "$PUB" --channel stable --firnc "$FIRNC" --lib-dirs std --version 0.1.9
expect_fail "publishing an older version is refused" publish 0.1.0
expect "manifest is valid JSON with sha256" python3 -I -c "import json,sys; m=json.load(open('$PUB/stable/manifest.json')); assert len(m['platforms']['linux-x86_64']['sha256'])==64"
expect "bootstrap install.sh (file://)" sh tools/release/install.sh "file://$PUB" stable
[ "$(current)" = 0.1.1 ] && ok "current -> 0.1.1" || bad "current -> 0.1.1 ($(current))"
expect "firn version" "$F" version; has "version names 0.1.1" "firn 0.1.1"
printf 'import std.io\nfn main() -> i32 {\n    io.print_line("hello-through-shim")\n    return 0\n}\n' > "$D/h.fi"
expect "firnc shim compiles" "$FC" "$D/h.fi" -o "$D/h"
expect "compiled program runs" "$D/h"; has "program output" hello-through-shim
expect "firn run" "$F" run "$D/h.fi"; has "firn run output" hello-through-shim
expect "firn build" "$F" build "$D/h.fi" -o "$D/h2"
expect "firn doctor" env PATH="$FIRN_HOME/bin:$PATH" "$F" doctor

expect "publish 0.1.2" publish 0.1.2
expect "firn upgrade -> 0.1.2" "$F" upgrade; has "upgrade message" "now using firn 0.1.2"
[ "$(current)" = 0.1.2 ] && ok "current -> 0.1.2" || bad "current -> 0.1.2"
expect "firn list" "$F" list; has "list shows old version" "  0.1.1"; has "list marks current" "\* 0.1.2"
expect "upgrade again is a no-op" "$F" upgrade; has "up to date" "is up to date"

mkdir -p "$D/proj/src"
echo "firn 0.1.1" > "$D/proj/firn.toolchain"
expect "project pin picks 0.1.1" bash -c "cd '$D/proj/src' && '$F' version"; has "pinned project version" "firn 0.1.1"
expect "firnc shim inside pinned project" bash -c "cd '$D/proj/src' && '$FC' '$D/h.fi' -o '$D/h3' && '$D/h3'"
echo "firn >=0.1.2" > "$D/proj2.toolchain"; mkdir -p "$D/proj2"; mv "$D/proj2.toolchain" "$D/proj2/firn.toolchain"
expect "range wish uses current" bash -c "cd '$D/proj2' && '$F' version"; has "range version" "firn 0.1.2"
echo "firn 0.9.0" > "$D/proj2/firn.toolchain"
expect_fail "missing pinned version is an error" bash -c "cd '$D/proj2' && '$F' version"
echo "firn 0.1.2" > "$D/proj2/firn.toolchain"
expect "upgrade reports stale project pins" "$F" upgrade; has "stale project listed" "proj/firn.toolchain (firn 0.1.1)"

expect "publish 0.1.3" publish 0.1.3
cp "$PUB/stable/manifest.json" "$D/good.json"
python3 -I -c "import json; p='$PUB/stable/manifest.json'; t=open(p).read(); m=json.loads(t); h=m['platforms']['linux-x86_64']['sha256']; open(p,'w').write(t.replace(h, ('0' if h[0]!='0' else '1')+h[1:]))"
expect_fail "upgrade with a wrong hash fails" "$F" upgrade; has "hash error named" "SHA-256 mismatch"
[ "$(current)" = 0.1.2 ] && ok "current stays 0.1.2 after bad hash" || bad "current stays 0.1.2 after bad hash"
[ ! -e "$FIRN_HOME/versions/0.1.3" ] && [ -z "$(ls -A "$FIRN_HOME/versions" | grep stage)" ] && ok "nothing left of the bad download" || bad "nothing left of the bad download"
cp "$D/good.json" "$PUB/stable/manifest.json"

printf '#!/bin/sh\necho broken >&2\nexit 3\n' > "$D/broken-firnc"; chmod +x "$D/broken-firnc"
expect "publish broken 0.1.4" publish 0.1.4 "$D/broken-firnc"
expect_fail "upgrade to a broken compiler fails the selftest" "$F" upgrade; has "selftest named" "selftest"
[ "$(current)" = 0.1.2 ] && ok "current stays 0.1.2 after failed selftest" || bad "current stays 0.1.2 after failed selftest"
expect "publish 0.1.5" publish 0.1.5

expect "pin 0.1.3" "$F" upgrade --pin 0.1.3; [ "$(current)" = 0.1.3 ] && ok "current -> 0.1.3 (pinned)" || bad "current -> 0.1.3 (pinned)"
expect "pinned upgrade holds" "$F" upgrade; has "pin message" "pinned to 0.1.3"
expect "unpin upgrades" "$F" upgrade --unpin; [ "$(current)" = 0.1.5 ] && ok "current -> 0.1.5 after unpin" || bad "current -> 0.1.5 after unpin"
expect "firn use 0.1.1" "$F" use 0.1.1; [ "$(current)" = 0.1.1 ] && ok "use switches" || bad "use switches"
expect "firn use 0.1.5" "$F" use 0.1.5

expect "publish 0.1.6 with floor 0.1.6" publish 0.1.6 "$FIRNC" "--min-version 0.1.6"
expect "upgrade -> 0.1.6 raises the floor" "$F" upgrade
grep -qx 'floor=0.1.6' "$FIRN_HOME/config" && ok "floor stored" || bad "floor stored"
cp "$PUB/stable/manifest-0.1.2.json" "$PUB/stable/manifest.json"
expect_fail "replayed old manifest below floor refused" "$F" install; has "floor refusal" "older than the trusted floor"

PORT=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$PUB" >/dev/null 2>&1 &
SERVER_PID=$!
cp "$PUB/stable/manifest-0.1.6.json" "$PUB/stable/manifest.json"
sleep 1
export FIRN_HOME=$D/home2
expect "bootstrap over http://127.0.0.1" sh tools/release/install.sh "http://127.0.0.1:$PORT" stable
expect "http install runs" "$FIRN_HOME/bin/firn" version; has "http version" "firn 0.1.6"

echo "toolchain e2e: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]

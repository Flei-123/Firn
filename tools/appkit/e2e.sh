#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/appkit/e2e.sh -- THE UPDATE, END TO END, against a LOCAL store.
#
# What runs: the real store tool (orientstore `store init`, `add-app`) makes a
# signed catalog in a temp directory; tools/appkit/teststore_server.py serves
# it over HTTP on a free port (and misbehaves on request: chunked bodies,
# redirects, a dropped connection, a slow link); tools/appkit/e2e/app.fi --
# a program built on lib/appkit -- is compiled several times (versions, a
# build that crashes at start, one that hangs) and driven through:
#
#   check (thread / process worker / blocking), progress, hash check,
#   a wrong hash, a wrong signature, a wrong key, a changed catalog, an
#   expired entry, a rolled-back (older) catalog, the old catalog-only
#   way, channels, the version floor, chunked/redirected/cut-off answers,
#   a cancelled download, the real replacement of the running program, the
#   new program confirming itself, and the ROLLBACK when the new program
#   crashes or hangs.
#
# Nothing here touches the live store: every address is 127.0.0.1.
#
#   bash tools/appkit/e2e.sh                       # Linux: ~2 minutes
#   E2E_TARGET=windows bash tools/appkit/e2e.sh    # the Windows build under Wine
#   FIRNC=... ORIENTSTORE_TOOL=... bash tools/appkit/e2e.sh
#
# On Windows there are no threads in Firn: the "thread" runs fall back to the
# worker process, and a hung new version is rolled back but not killed.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
STORE_TOOL=${ORIENTSTORE_TOOL:-}
if [ -z "$STORE_TOOL" ]; then
    for c in /root/orientstore-wt-apps/werkzeug/store /root/orientstore/werkzeug/store; do
        if [ -f "$c" ] && grep -q 'add-app' "$c"; then STORE_TOOL=$c; break; fi
    done
fi
if [ ! -x "$FIRNC" ] || [ -z "$STORE_TOOL" ]; then
    echo "SKIP: needs firnc and an orientstore tool with 'add-app' (set ORIENTSTORE_TOOL)"
    exit 0
fi
if ! python3 -c 'import cryptography' 2>/dev/null; then
    echo "SKIP: python3 cryptography is missing"
    exit 0
fi
STOOLDIR=$(dirname "$STORE_TOOL")

TARGET=${E2E_TARGET:-linux}
if [ "$TARGET" = windows ]; then
    if ! command -v wine >/dev/null 2>&1 || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
        echo "SKIP: needs wine and the mingw binutils for the Windows run"
        exit 0
    fi
    EXT=.exe
    ART=exe
    ZIEL=windows-x86_64
    export WINEPREFIX=${WINEPREFIX:-/root/.wine-firn} WINEDEBUG=-all
    TARGETFLAGS="--target=x86_64-windows"
else
    EXT=""
    ART=bin
    ZIEL=linux-x86_64
    TARGETFLAGS=""
fi
W=$(mktemp -d "${TMPDIR:-/tmp}/appkit-e2e.XXXXXX")
SRV_PIDS=()
cleanup() {
    for p in "${SRV_PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
    # a hung test program may still run
    pkill -f "$W/" 2>/dev/null
    [ "$TARGET" = windows ] && wineserver -k 2>/dev/null
    rm -rf "$W"
}
trap cleanup EXIT

PASS=0
FAIL=0
FAILED=""
ok()  { PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); FAILED="$FAILED\n  - $1"; printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -8; }
# expect <name> <haystack> <needle>
expect() { if printf '%s' "$2" | grep -qF -- "$3"; then ok "$1"; else bad "$1" "wanted '$3' in: $2"; fi; }
refute() { if printf '%s' "$2" | grep -qF -- "$3"; then bad "$1" "did NOT want '$3' in: $2"; else ok "$1"; fi; }

export ORIENTSTORE_REPO=$W/repo
STORE="python3 $STORE_TOOL"
mkdir -p "$W/home" "$W/art" "$W/inst" "$W/cache"

echo "== 1. the store"
$STORE init --name "E2E store" --adresse "http://127.0.0.1/" >"$W/init.log" 2>&1 || { cat "$W/init.log"; exit 1; }
KEY=$(python3 "$STORE_TOOL" key)
echo "   key $KEY"

start_server() {   # start_server <name> [server options...]; sets PORT_<name>
    local name=$1; shift
    python3 tools/appkit/teststore_server.py --root "$ORIENTSTORE_REPO" --log "$W/srv-$name.log" "$@" >"$W/srv-$name.out" 2>&1 &
    SRV_PIDS+=($!)
    local i
    for i in $(seq 1 50); do
        if grep -q '^PORT ' "$W/srv-$name.out" 2>/dev/null; then break; fi
        sleep 0.1
    done
    eval "PORT_$name=\$(sed -n 's/^PORT //p' \"\$W/srv-$name.out\" | head -1)"
}
start_server main
PORT=$PORT_main
echo "   server main on port $PORT"

echo "== 2. build the test program (versions, behaviours)"
build() {   # build <version> <behaviour> <out> [key] [channel]
    FIRN_E2E_VERSION=$1 FIRN_E2E_BEHAVIOR=$2 FIRN_E2E_STORE=http://127.0.0.1:$PORT/ \
    FIRN_E2E_KEY=${4:-$KEY} FIRN_E2E_CHANNEL=${5:-stabil} \
        "$FIRNC" $TARGETFLAGS --opt-level=dev-fast -o "$3" tools/appkit/e2e/app.fi 2>"$W/build.err" \
        || { echo "build failed:"; cat "$W/build.err"; exit 1; }
}
build 1.0.0 good "$W/art/app-1.0.0$EXT"
build 1.1.0 good "$W/art/app-1.1.0$EXT"
build 1.2.0 crash "$W/art/app-1.2.0$EXT"
build 1.3.0 hang "$W/art/app-1.3.0$EXT"
build 1.1.1-beta.1 good "$W/art/app-1.1.1-beta.1$EXT"
build 1.0.0 good "$W/art/app-wrongkey$EXT" "0000000000000000000000000000000000000000000000000000000000000001"
ls -la "$W/art" | awk 'NR>1{print "   " $5 " " $9}'

publish() {  # publish <version> [extra add-app options]
    local v=$1; shift
    $STORE add-app --art $ART --id e2eapp --fassung "$v" --ziel $ZIEL --name "E2E App" \
        --aenderungen "release $v: the notes line one
second line" "$@" "$W/art/app-$v$EXT" >"$W/publish-$v.log" 2>&1 \
        || { echo "publish $v failed"; cat "$W/publish-$v.log"; exit 1; }
}
publish 1.1.0

# the program under test: version 1.0.0 installed
APP=$W/inst/app$EXT
cp "$W/art/app-1.0.0$EXT" "$APP"
chmod +x "$APP"
winpath() { printf 'Z:%s' "$(printf '%s' "$1" | tr '/' '\\')"; }
# every "home" is a directory with its own settings and state, never a real
# user's: APPKIT_HOME relocates all of appkit's directories (Wine would
# overwrite APPDATA and LOCALAPPDATA, this variable it leaves alone)
run_in() {   # run_in <home-dir> <program> args...
    local home=$1 prog=$2; shift 2
    if [ "$TARGET" = windows ]; then
        APPKIT_HOME=$(winpath "$home") timeout 150 wine "$prog" "$@" 2>&1 | tr -d '\r'
    else
        APPKIT_HOME=$home timeout 60 "$prog" "$@" 2>&1
    fi
}
run() { local prog=$1; shift; run_in "$W/home" "$prog" "$@"; }
cfgdir() {   # cfgdir <home-dir>: where the program keeps settings.json
    echo "$1/config/e2eapp"
}
SHA11=$(sha256sum "$W/art/app-1.1.0$EXT" | cut -d' ' -f1)
THREADMODE=thread
[ "$TARGET" = windows ] && THREADMODE=process

echo "== 3. the program itself"
out=$(run "$APP" version)
expect "the installed program is 1.0.0" "$out" "VERSION 1.0.0"

echo "== 4. check: all three ways to run it"
for m in blocking $THREADMODE process; do
    out=$(run "$APP" check $m)
    expect "check ($m): update available" "$out" "RESULT update available err=0 ver=1.1.0"
    expect "check ($m): size is the file's" "$out" "size=$(stat -c %s "$W/art/app-1.1.0$EXT")"
done
out=$(run "$APP" state)
expect "state: the catalog revision is remembered" "$out" "STATE seen.revision="
expect "state: the entry was seen" "$out" "STATE entry.seen=1"
expect "state: the time of the check" "$out" "STATE check.last="

echo "== 5. fetch: download, progress, hash"
out=$(run "$APP" fetch $THREADMODE)
expect "fetch (first mode): ready" "$out" "RESULT ready to install err=0 ver=1.1.0"
expect "fetch: progress lines reached the end" "$out" "PROGRESS $(stat -c %s "$W/art/app-1.1.0$EXT") $(stat -c %s "$W/art/app-1.1.0$EXT")"
if [ -f "$APP.new" ] && [ "$(sha256sum "$APP.new" | cut -d' ' -f1)" = "$SHA11" ]; then ok "the staged file is byte for byte the published one"; else bad "the staged file"; fi
prog=$(printf '%s\n' "$out" | sed -n 's/^PROGRESS \([0-9]*\) .*/\1/p' | tr '\n' ' ')
if python3 - "$prog" <<'PY'
import sys
v = [int(x) for x in sys.argv[1].split()]
sys.exit(0 if v and v == sorted(v) and len(set(v)) == len(v) else 1)
PY
then ok "progress only goes up"; else bad "progress is monotonic" "$prog"; fi
rm -f "$APP.new"
out=$(run "$APP" fetch process)
expect "fetch (process worker): ready" "$out" "RESULT ready to install err=0 ver=1.1.0"
rm -f "$APP.new"

echo "== 6. the hash, the signature, the catalog, the key"
PAYLOAD=$ORIENTSTORE_REPO/$(python3 - <<PY
import json
d = json.load(open("$ORIENTSTORE_REPO/index.json"))
print(d["pakete"]["$ART:e2eapp"]["builds"][0]["datei"])
PY
)
cp "$PAYLOAD" "$W/payload.good"
python3 - "$PAYLOAD" <<'PY'
import sys
p = sys.argv[1]
b = bytearray(open(p, "rb").read())
b[len(b) // 2] ^= 1
open(p, "wb").write(bytes(b))
PY
out=$(run "$APP" fetch $THREADMODE)
expect "a changed byte in the download is refused (hash)" "$out" "err=7"
if [ ! -e "$APP.new" ]; then ok "and the file is deleted"; else bad "the refused download was left behind"; fi
cp "$W/payload.good" "$PAYLOAD"

cp "$ORIENTSTORE_REPO/entry.json.sig" "$W/entry.sig.good"
python3 - "$ORIENTSTORE_REPO/entry.json.sig" <<'PY'
import sys
p = sys.argv[1]
b = bytearray(open(p, "rb").read())
b[10] ^= 1
open(p, "wb").write(bytes(b))
PY
out=$(run "$APP" check $THREADMODE)
expect "a wrong signature is refused" "$out" "err=2"
cp "$W/entry.sig.good" "$ORIENTSTORE_REPO/entry.json.sig"

cp "$ORIENTSTORE_REPO/index.json" "$W/index.good"
python3 - "$ORIENTSTORE_REPO/index.json" <<'PY'
import sys
p = sys.argv[1]
t = open(p, "rb").read().replace(b"E2E App", b"E2E Evil")
open(p, "wb").write(t)
PY
out=$(run "$APP" check $THREADMODE)
expect "a changed catalog (not what the entry hashes) is refused" "$out" "err=4"
cp "$W/index.good" "$ORIENTSTORE_REPO/index.json"

build 1.0.0 good "$W/art/app-wrongkey" "0000000000000000000000000000000000000000000000000000000000000001"
out=$(run "$W/art/app-wrongkey$EXT" check $THREADMODE)
expect "a program with another trust anchor refuses the store" "$out" "err=2"

echo "== 7. freshness: expired entry, older catalog"
cp "$ORIENTSTORE_REPO/entry.json" "$W/entry.good"
cp "$ORIENTSTORE_REPO/entry.json.sig" "$W/entry.sig.good"
python3 - "$ORIENTSTORE_REPO" "$STOOLDIR" <<'PY'
import sys
sys.path.insert(0, sys.argv[2])
import eingang as EG
import katalog as K
verz = sys.argv[1]
sk = open(verz + "/schluessel.geheim", "rb").read()
roh = open(verz + "/index.json", "rb").read()
import json
rev = json.loads(roh)["revision"]
e = EG.bauen(verz, roh, rev, -3)       # expired three days ago
EG.schreiben(verz, e, sk)
PY
out=$(run "$APP" check $THREADMODE)
expect "an expired entry is refused" "$out" "err=3"
cp "$W/entry.good" "$ORIENTSTORE_REPO/entry.json"
cp "$W/entry.sig.good" "$ORIENTSTORE_REPO/entry.json.sig"
out=$(run "$APP" check $THREADMODE)
expect "the repaired store is accepted again" "$out" "RESULT update available"

# an OLDER, correctly signed catalog: keep it, publish more, put it back
mkdir -p "$W/old"
cp "$ORIENTSTORE_REPO/index.json" "$ORIENTSTORE_REPO/index.json.sig" "$ORIENTSTORE_REPO/entry.json" "$ORIENTSTORE_REPO/entry.json.sig" "$W/old/"
publish 1.1.1-beta.1 --kanal beta
out=$(run "$APP" check $THREADMODE)      # the program sees the newer revision
expect "the newer revision is seen" "$out" "RESULT update available"
cp "$W/old/index.json" "$W/old/index.json.sig" "$W/old/entry.json" "$W/old/entry.json.sig" "$ORIENTSTORE_REPO/"
out=$(run "$APP" check $THREADMODE)
expect "an older (but signed) catalog is refused (rollback attack)" "$out" "err=3"
# put the current catalog back by re-publishing the same build
python3 "$STORE_TOOL" eingang >/dev/null 2>&1
$STORE add-app --art $ART --id e2eapp --fassung 1.1.1-beta.1 --ziel $ZIEL --kanal beta --immer "$W/art/app-1.1.1-beta.1$EXT" >/dev/null 2>&1

echo "== 8. channels"
build 1.0.0 good "$W/art/app-1.0.0-beta$EXT" "$KEY" beta
out=$(run "$W/art/app-1.0.0-beta$EXT" check $THREADMODE)
expect "channel beta offers 1.1.1-beta.1" "$out" "ver=1.1.1-beta.1"
out=$(run "$APP" check $THREADMODE)
expect "channel stabil still offers 1.1.0" "$out" "ver=1.1.0"

echo "== 9. the version floor"
publish 1.1.0 --mindest 9.0.0 --immer
out=$(run "$APP" check $THREADMODE)
expect "a build below the package's floor is not offered" "$out" "err=17"
publish 1.1.0 --mindest 1.0.0 --immer
out=$(run "$APP" check $THREADMODE)
expect "with a reachable floor it is offered" "$out" "RESULT update available"

echo "== 10. a bad network: chunked, redirect, dropped, slow + cancelled"
start_server chunked --chunked
mkdir -p "$(cfgdir "$W/home")"
set_store() {   # set_store <port|none>
    if [ "$1" = none ]; then rm -f "$(cfgdir "$W/home")/settings.json"; else
        printf '{"update.store": "http://127.0.0.1:%s/", "update.healthy_s": 4}\n' "$1" >"$(cfgdir "$W/home")/settings.json"; fi
}
set_store "$PORT_chunked"
out=$(run "$APP" fetch $THREADMODE)
expect "a chunked answer (with extensions and trailer) downloads intact" "$out" "RESULT ready to install err=0 ver=1.1.0"
if [ "$(sha256sum "$APP.new" 2>/dev/null | cut -d' ' -f1)" = "$SHA11" ]; then ok "chunked: same bytes"; else bad "chunked: the bytes differ"; fi
rm -f "$APP.new"
mkdir -p "$ORIENTSTORE_REPO/mirror" && cp "$ORIENTSTORE_REPO/index.json" "$ORIENTSTORE_REPO/mirror/index.json"
start_server redir --redirect "/index.json=/mirror/index.json"
set_store "$PORT_redir"
out=$(run "$APP" check $THREADMODE)
expect "a redirect to the same catalog is followed" "$out" "RESULT update available"
start_server cut --cutoff speicher/
set_store "$PORT_cut"
out=$(run "$APP" fetch $THREADMODE)
expect "a dropped connection in the download is an error" "$out" "RESULT error"
if [ ! -e "$APP.new" ]; then ok "no half file is left"; else bad "a half file was left behind"; fi
start_server slow --slow-ms 400
set_store "$PORT_slow"
out=$(run "$APP" fetch $THREADMODE)
n=$(printf '%s\n' "$out" | grep -c '^PROGRESS ')
if [ "$n" -ge 3 ]; then ok "a slow download reports progress several times ($n lines)"; else bad "progress on a slow link" "$out"; fi
rm -f "$APP.new"
set_store "$PORT_slow"
out=$(run "$APP" fetchcancel $THREADMODE)
expect "a cancelled download ends as cancelled" "$out" "RESULT error err=13"
if [ ! -e "$APP.new" ]; then ok "a cancelled download leaves nothing"; else bad "a cancelled download left a file"; fi
set_store none

echo "== 11. the old way (catalog without entry)"
mkdir -p "$W/legacy"
cp "$ORIENTSTORE_REPO/index.json" "$ORIENTSTORE_REPO/index.json.sig" "$W/legacy/"
mkdir -p "$W/legacy/speicher"
cp -r "$ORIENTSTORE_REPO/speicher/." "$W/legacy/speicher/"
python3 tools/appkit/teststore_server.py --root "$W/legacy" --log "$W/srv-legacy.log" >"$W/srv-legacy.out" 2>&1 &
SRV_PIDS+=($!)
for i in $(seq 1 50); do grep -q '^PORT ' "$W/srv-legacy.out" && break; sleep 0.1; done
PORT_legacy=$(sed -n 's/^PORT //p' "$W/srv-legacy.out" | head -1)
rm -rf "$W/home2"; mkdir -p "$(cfgdir "$W/home2")"
printf '{"update.store": "http://127.0.0.1:%s/"}\n' "$PORT_legacy" >"$(cfgdir "$W/home2")/settings.json"
out=$(run_in "$W/home2" "$APP" check $THREADMODE)
expect "a store without entry.json works while no entry was ever seen" "$out" "RESULT update available"
out=$(run_in "$W/home2" "$APP" state)
refute "and it does not claim to have seen an entry" "$out" "entry.seen=1"
# now the same device sees a real entry, then the entry disappears
printf '{"update.store": "http://127.0.0.1:%s/"}\n' "$PORT" >"$(cfgdir "$W/home2")/settings.json"
out=$(run_in "$W/home2" "$APP" check $THREADMODE)
expect "the same device now sees the entry" "$out" "RESULT update available"
printf '{"update.store": "http://127.0.0.1:%s/"}\n' "$PORT_legacy" >"$(cfgdir "$W/home2")/settings.json"
out=$(run_in "$W/home2" "$APP" check $THREADMODE)
expect "after an entry was seen, a store without one is refused" "$out" "RESULT error"

echo "== 12. the real replacement, and the confirmation"
set_store none
out=$(run "$APP" update $THREADMODE)
expect "update: the new version is downloaded and verified" "$out" "RESULT ready to install err=0 ver=1.1.0"
expect "update: the file was replaced" "$out" "APPLY 0"
expect "update: the new version confirmed itself" "$out" "SUPERVISE healthy"
out=$(run "$APP" version)
expect "the program on disk is now 1.1.0" "$out" "VERSION 1.1.0"
if [ "$(sha256sum "$APP" | cut -d' ' -f1)" = "$SHA11" ]; then ok "and it is byte for byte the published file"; else bad "the replaced program differs from the published one"; fi
out=$(run "$APP" state)
expect "state: the update is confirmed" "$out" "STATE pending.state=confirmed"
out=$(run "$APP" check $THREADMODE)
expect "after the update: up to date (same file as the store's)" "$out" "RESULT up to date"
if [ ! -e "$APP.old" ]; then ok "the backup is cleaned up after the confirmation"; else bad "the backup file was left"; fi

echo "== 13. a new version that CRASHES: rolled back"
publish 1.2.0
out=$(run "$APP" update $THREADMODE)
expect "crash: the update was applied" "$out" "APPLY 0"
expect "crash: the supervisor rolled back" "$out" "SUPERVISE rolledback"
out=$(run "$APP" version)
expect "the program on disk is 1.1.0 again" "$out" "VERSION 1.1.0"
if [ "$(sha256sum "$APP" | cut -d' ' -f1)" = "$SHA11" ]; then ok "and it is byte for byte the good file"; else bad "the rolled back program differs from the good one"; fi
sleep 1
out=$(run "$APP" state)
expect "state: the rollback is recorded" "$out" "STATE pending.state=rolledback"
expect "state: the bad build is remembered" "$out" "STATE bad.0="
out=$(run "$APP" check $THREADMODE)
expect "the failed build is not offered again" "$out" "RESULT up to date"

echo "== 14. a new version that HANGS: rolled back after the time limit"
publish 1.3.0
printf '{"update.healthy_s": 3}\n' >"$(cfgdir "$W/home")/settings.json"
out=$(run "$APP" update $THREADMODE)
expect "hang: the update was applied" "$out" "APPLY 0"
expect "hang: the supervisor rolled back" "$out" "SUPERVISE rolledback"
out=$(run "$APP" version)
expect "the program on disk is 1.1.0 again" "$out" "VERSION 1.1.0"
sleep 1
if [ "$TARGET" = windows ]; then
    echo "  --    (Windows: the hung new version is not killed -- no OpenProcess in the import table)"
    wineserver -k 2>/dev/null
elif pgrep -f "$W/inst/app" >/dev/null 2>&1; then bad "the hung new version is still running"; pkill -f "$W/inst/app"; else ok "the hung new version was stopped"; fi

echo
echo "appkit e2e: $PASS passed, $FAIL failed"
if [ "$FAIL" -ne 0 ]; then printf "%b\n" "$FAILED"; exit 1; fi
exit 0

#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/appkit/winkit.sh -- a KIT that runs the update test on ANOTHER machine.
#
#   bash tools/appkit/winkit.sh [windows|linux] [out.zip]
#
# The end-to-end run (tools/appkit/e2e.sh) needs bash, this tree and the store
# tool. A real Windows PC has none of them -- and this server cannot put files
# on one (the helper writes text only). So the kit is a zip that needs ONLY
# Python 3 on the other side:
#
#   app-1.0.0.exe          the program that is "installed" (it asks the local store)
#   app-1.1.0.exe          the update it will find (also the file the store serves)
#   app-1.2.0.exe          a version that crashes at start
#   app-1.3.0.exe          a version that hangs
#   app-1.1.0-dev.exe      another file of version 1.1.0 (a developer's own build)
#   speicher/              the store's files (content addressed)
#   state-a|b|c/           three signed catalogs: 1.1.0; + 1.2.0; + 1.3.0
#   server: run.py plays the store on 127.0.0.1:<port.txt> itself
#   run.py                 the whole test; README.txt says how to start it
#
# Unzip it, run `py run.py` (Windows) or `python3 run.py` (Linux). 8 sections,
# the same checks as e2e.sh. The catalogs are good for 14 days.
#
# The programs are tools/appkit/e2e/app.fi built for the target with the
# store's key and http://127.0.0.1:18765/ compiled in (the test store key is
# made here and thrown away with the kit's build directory: nothing of it is
# trusted anywhere else).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
TARGET=${1:-windows}
OUT=${2:-$ROOT/build/appkit-$TARGET-kit.zip}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
STORE_TOOL=${ORIENTSTORE_TOOL:-}
if [ -z "$STORE_TOOL" ]; then
    for c in /root/orientstore/werkzeug/store; do
        if [ -f "$c" ] && grep -q 'add-app' "$c"; then STORE_TOOL=$c; break; fi
    done
fi
[ -x "$FIRNC" ] || { echo "no compiler"; exit 1; }
[ -n "$STORE_TOOL" ] || { echo "needs an orientstore tool with 'add-app' (set ORIENTSTORE_TOOL)"; exit 1; }
case "$TARGET" in
    windows) EXT=.exe; ART=exe; ZIEL=windows-x86_64; TF="--target=x86_64-windows" ;;
    linux) EXT=""; ART=bin; ZIEL=linux-x86_64; TF="" ;;
    *) echo "target: windows or linux"; exit 2 ;;
esac
PORT=18765
W=$(mktemp -d "${TMPDIR:-/tmp}/appkit-winkit.XXXXXX")
trap 'rm -rf "$W"' EXIT
K=$W/appkit-kit
mkdir -p "$K"
export ORIENTSTORE_REPO=$W/repo
STORE="python3 $STORE_TOOL"
$STORE init --name "appkit kit store" --adresse "http://127.0.0.1:$PORT/" >"$W/init.log" 2>&1 || { cat "$W/init.log"; exit 1; }
KEY=$(python3 "$STORE_TOOL" key | head -1)

build() {   # build <version> <behaviour> <out>
    FIRN_E2E_VERSION=$1 FIRN_E2E_BEHAVIOR=$2 FIRN_E2E_STORE=http://127.0.0.1:$PORT/ FIRN_E2E_KEY=$KEY FIRN_E2E_CHANNEL=stabil \
        "$FIRNC" $TF --opt-level=dev-fast -o "$3" tools/appkit/e2e/app.fi 2>"$W/build.err" \
        || { echo "build failed:"; cat "$W/build.err"; exit 1; }
}
build 1.0.0 good "$K/app-1.0.0$EXT"
build 1.1.0 good "$K/app-1.1.0$EXT"
build 1.2.0 crash "$K/app-1.2.0$EXT"
build 1.3.0 hang "$K/app-1.3.0$EXT"
build 1.1.0 dev "$K/app-1.1.0-dev$EXT"

publish() {   # publish <version> <state dir name>
    $STORE add-app --art $ART --id e2eapp --fassung "$1" --ziel $ZIEL --name "E2E App" \
        --aenderungen "release $1" "$K/app-$1$EXT" >"$W/publish-$1.log" 2>&1 \
        || { echo "publish $1 failed"; cat "$W/publish-$1.log"; exit 1; }
    mkdir -p "$K/state-$2"
    for f in index.json index.json.sig entry.json entry.json.sig; do cp "$ORIENTSTORE_REPO/$f" "$K/state-$2/$f"; done
}
publish 1.1.0 a
publish 1.2.0 b
publish 1.3.0 c
cp -r "$ORIENTSTORE_REPO/speicher" "$K/speicher"
cp tools/appkit/winkit/run.py "$K/run.py"
echo $PORT > "$K/port.txt"
EXPIRES=$(date -u -d '+14 days' +%Y-%m-%d)
cat > "$K/README.txt" <<EOF
appkit update test (kit for $TARGET), made $(date -u +%Y-%m-%d), good until $EXPIRES

Needs: Python 3 (Windows: the "py" launcher). Nothing else, nothing is installed.

    py run.py              (Windows)
    python3 run.py         (Linux)

What it does: a small web server on 127.0.0.1:$PORT plays the store; the
programs in this folder (built from tools/appkit/e2e/app.fi of the Firn tree)
check it, download from it, refuse a changed byte and a wrong signature,
replace themselves, and roll back after a version that crashes and one that
hangs. Each check prints "ok" or "FAIL"; the last line is the count and the
exit code is 0 when everything passed. Add --keep to keep the temporary folder.

If Windows Defender or SmartScreen blocks the programs, allow the folder: they
are unsigned test programs. The port $PORT must be free.
EOF
mkdir -p "$(dirname "$OUT")"
python3 - "$K" "$OUT" <<'PY'
import os, sys, zipfile
src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for top, _, files in os.walk(src):
        for f in sorted(files):
            p = os.path.join(top, f)
            z.write(p, os.path.join("appkit-kit", os.path.relpath(p, src)))
print("wrote", out, os.path.getsize(out), "octets")
PY

#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/fui/fuibuild.sh -- GATE FOR `fui new` AND `fui build` (tools/fui/cli).
#
#   bash tools/fui/fuibuild.sh
#
# A fresh project from `fui new` (every template) and the apps editor, notes, files, fm, svcdemo: native + web build from ONE source,
# native --shot against the Chromium shot of the web build (limit 0.5 %), layout of dist/, exit codes.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
export FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
FUI="bash $ROOT/tools/fui/cli/fui"
D=$(mktemp -d "${TMPDIR:-/tmp}/fuibuild.XXXXXX")
trap 'rm -rf "$D"' EXIT
fail=0

expect_exit() {
    local want=$1 name=$2
    shift 2
    "$@" >"$D/last.log" 2>&1
    local got=$?
    if [ "$got" != "$want" ]; then
        echo "FAIL $name: exit $got, wanted $want"
        tail -5 "$D/last.log"
        fail=1
    else
        echo "ok   $name"
    fi
}

expect_file() {
    [ -e "$1" ] || { echo "FAIL missing $1"; fail=1; }
}

cd "$D"
expect_exit 0 "new" $FUI new probe
for f in firn.package fui.app main.fi .gitignore data/greeting.json; do expect_file "$D/probe/$f"; done
expect_exit 2 "new refuses a non-empty folder" $FUI new probe
expect_exit 2 "new rejects a bad name" $FUI new Bad-Name
expect_exit 2 "new rejects an unknown template" $FUI new probe_x --template nope
for t in blank settings files; do
    expect_exit 0 "new --template $t" $FUI new "tpl_$t" --template "$t"
    for f in firn.package fui.app main.fi .gitignore; do expect_file "$D/tpl_$t/$f"; done
done
expect_file "$D/tpl_settings/ds_icons_set.fi"
expect_file "$D/tpl_files/ds_icons_set.fi"
expect_exit 0 "new --dir" $FUI new viadir --dir "$D/elsewhere/viadir"
expect_file "$D/elsewhere/viadir/main.fi"
expect_exit 5 "build without manifest" $FUI build "$D"
expect_exit 2 "build unknown option" $FUI build probe --bogus
expect_exit 2 "build unknown target" $FUI build probe --targets bogus
expect_exit 2 "verify needs native and web" $FUI build probe --targets native --verify
expect_exit 4 "android without SDK" env NDK=/nonexistent $FUI build probe --targets android --out "$D/noandroid"

if ! command -v chromium >/dev/null 2>&1; then
    echo "chromium missing -- web verify skipped"
    exit $fail
fi

expect_exit 0 "build + verify new project" $FUI build probe --targets native,web --verify --out "$D/out-probe"
expect_file "$D/out-probe/native/probe"
expect_file "$D/out-probe/native/data/greeting.json"
expect_file "$D/out-probe/web/index.html"
expect_file "$D/out-probe/web/probe.wasm"
expect_exit 3 "pixel gate fails above the limit" $FUI verify probe --out "$D/out-probe" --limit -1
expect_exit 0 "verify an existing build" $FUI verify probe --out "$D/out-probe"

for t in blank settings files; do
    expect_exit 0 "build + verify template $t" $FUI build "tpl_$t" --targets native,web --verify --out "$D/out-tpl-$t"
    grep -h "^RESULT" "$D/last.log"
done

for app in editor notes files fm svcdemo; do
    expect_exit 0 "build + verify $app" $FUI build "$ROOT/examples/fui_apps/$app" --targets native,web --verify --out "$D/out-$app"
    grep -h "^RESULT" "$D/last.log"
    expect_file "$D/out-$app/native/$app"
    expect_file "$D/out-$app/web/index.html"
done
exit $fail

#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/appkit/newapp_test.sh -- the GENERATOR works: tools/newapp.sh makes a
# project that builds, starts, and releases.
#
#   1. newapp.sh "Probe App" probe-app --store-key <a test key>   (files, placeholders filled)
#   2. build.sh                                    -> build/probe-app (the template compiles on fUi + fui.kit)
#   3. the program starts on an X server (Xvfb) with --selftest: 30 frames, clean exit, a log line
#   4. release.sh (dry run): builds, publishes into a throw-away store with `store add-app`,
#      verifies it -- and the catalog it made names bin:probe-app with the right version
#   5. the same for a project generated with another id/vendor (substitutions are not hard coded)
#
# Needs fui.kit (lib/fui/kit.fi) and an X server (Xvfb) for step 3; without them
# the steps that cannot run are SKIPped and the rest still runs.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
TOOL=${ORIENTSTORE_TOOL:-}
if [ -z "$TOOL" ]; then
    for c in /root/orientstore-wt-apps/werkzeug/store /root/orientstore/werkzeug/store; do
        [ -f "$c" ] && grep -q add-app "$c" && TOOL=$c && break
    done
fi
W=$(mktemp -d "${TMPDIR:-/tmp}/appkit-newapp.XXXXXX")
trap 'rm -rf "$W"' EXIT
PASS=0; FAIL=0
ok() { PASS=$((PASS+1)); printf '  ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -6; }
KEY=33f41a31ce341fc7d441406233bf10f82ca82940b956bb5eaa46bd6047aa2f5b

if [ ! -x "$FIRNC" ]; then echo "SKIP: no compiler"; exit 0; fi
if [ ! -f "$ROOT/lib/fui/kit.fi" ]; then echo "SKIP: lib/fui/kit.fi is not in this tree (the template's window needs the fUi kit)"; exit 0; fi

echo "== 1. generate"
out=$(bash tools/newapp.sh "Probe App" probe-app --dir "$W/p1" --store-key $KEY --vendor ProbeCo --android-id org.probe.app 2>&1) \
    && ok "newapp.sh ran" || bad "newapp.sh" "$out"
for f in VERSION build.sh build-windows.sh build-android.sh release.sh src/main.fi src/ui.fi src/appspec.fi locale/en.opmsg locale/de.opmsg README.md .gitignore; do
    [ -f "$W/p1/$f" ] && ok "file $f" || bad "file $f is missing"
done
if grep -rq '@[A-Z_]*@' "$W/p1" --include='*' 2>/dev/null; then bad "an unfilled placeholder is left" "$(grep -rn '@[A-Z_]*@' "$W/p1" | head -3)"; else ok "every placeholder is filled"; fi
grep -q 'const STORE_KEY: str = "'$KEY'"' "$W/p1/src/appspec.fi" && ok "the trust anchor is in appspec.fi" || bad "the store key is not in appspec.fi"
grep -q 'const VENDOR: str = "ProbeCo"' "$W/p1/src/appspec.fi" && ok "vendor, id and android id are substituted" || bad "vendor"
out=$(bash tools/newapp.sh "Again" probe-app --dir "$W/p1" --store-key $KEY 2>&1); [ $? -ne 0 ] && ok "a second run into the same directory is refused" || bad "newapp.sh overwrote a project"
out=$(bash tools/newapp.sh "Bad Id" "Bad_Id" --dir "$W/p9" --store-key $KEY 2>&1); [ $? -ne 0 ] && ok "a bad app id is refused" || bad "a bad app id was accepted"
out=$(bash tools/newapp.sh "Bad Key" bad-key --dir "$W/p8" --store-key zz 2>&1); [ $? -ne 0 ] && ok "a bad key is refused" || bad "a bad key was accepted"

echo "== 2. build"
if FIRNC=$FIRNC FIRN_ROOT=$ROOT bash "$W/p1/build.sh" >"$W/build.log" 2>&1; then ok "build.sh builds the program"; else bad "build.sh" "$(tail -5 "$W/build.log")"; fi
[ -x "$W/p1/build/probe-app" ] || { echo "no binary; stopping"; exit 1; }

echo "== 3. it starts (Xvfb, --selftest)"
if command -v xvfb-run >/dev/null 2>&1; then
    out=$(APPKIT_HOME=$W/home timeout 60 xvfb-run -a -s "-screen 0 1280x800x24" "$W/p1/build/probe-app" --selftest 2>&1)
    echo "$out" | grep -q 'SELFTEST ok frames=30' && ok "30 frames painted, clean exit" || bad "selftest" "$out"
    grep -q 'first frame' "$W/home/logs/probe-app/app.log" 2>/dev/null && ok "the log has the first frame" || bad "no log"
    grep -q 'end$' "$W/home/logs/probe-app/app.log" 2>/dev/null && ok "and the end of the run" || bad "the run did not end cleanly"
    out=$(APPKIT_HOME=$W/home "$W/p1/build/probe-app" --selftest 2>&1 </dev/null); 
else
    echo "  --    xvfb-run missing: the start is not tested"
fi

echo "== 4. release (dry run into a throw-away store)"
if [ -n "$TOOL" ] && python3 -c 'import cryptography' 2>/dev/null; then
    echo 1.2.3 > "$W/p1/VERSION"
    out=$(cd "$W/p1" && FIRN_ROOT=$ROOT FIRNC=$FIRNC ORIENTSTORE_TOOL=$TOOL bash release.sh --notes "dry run notes" 2>&1) \
        && ok "release.sh (dry run) published and verified" || bad "release.sh" "$(echo "$out" | tail -6)"
    echo "$out" | grep -q 'bin:probe-app' && ok "it published bin:probe-app" || bad "no bin:probe-app in the output"
    echo "$out" | grep -q 'Fassung     1.2.3' && ok "at version 1.2.3" || bad "wrong version"
    echo "$out" | grep -q 'Ergebnis    in Ordnung' && ok "and the store verifies" || bad "store verify"
    # a real store is never touched without --store-repo and a yes
    out=$(cd "$W/p1" && FIRN_ROOT=$ROOT FIRNC=$FIRNC ORIENTSTORE_TOOL=$TOOL bash release.sh --store-repo "$W/real" </dev/null 2>&1); 
    [ ! -d "$W/real/speicher" ] && ok "a real store needs the typed YES (nothing was written)" || bad "a store was written without a yes"
else
    echo "  --    no store tool / python3 cryptography: the release is not tested"
fi

echo "== 5. another project, same generator"
out=$(bash tools/newapp.sh "Second" second-app --dir "$W/p2" --store-key $KEY --version 0.2.0-beta.1 2>&1) && ok "newapp.sh with --version" || bad "second project" "$out"
[ "$(tr -d ' \n' < "$W/p2/VERSION")" = "0.2.0-beta.1" ] && ok "VERSION is the one given" || bad "VERSION"

echo
echo "newapp: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

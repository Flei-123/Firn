#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
FI=$(pwd)/tools/firn/firn
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-templates.XXXXXX")
trap 'rm -rf "$W"' EXIT
export FIRN_CACHE=$W/cache
PASS=0
FAIL=0

check() {
    local name=$1
    shift
    if "$@" > "$W/out" 2> "$W/err"; then
        PASS=$((PASS + 1)); printf '  %-48s ok\n' "$name"
    else
        FAIL=$((FAIL + 1)); printf '  %-48s FAIL %s\n' "$name" "$(head -3 "$W/err" "$W/out" | tr '\n' ' ')"
    fi
}

echo "== firn new TEMPLATE NAME =="
cd "$W"
check "new --list names every template" bash -c "$FI new --list | grep -c . | grep -qx 5"
check "new rejects an invalid name" bash -c "! $FI new cli bad-name 2>/dev/null"
check "new refuses an existing directory" bash -c "$FI new cli dup >/dev/null && ! $FI new cli dup 2>/dev/null"

for tpl in cli lib web-api game; do
    name=t_${tpl//-/_}
    check "$tpl: new" "$FI" new "$tpl" "$name"
    check "$tpl: manifest, README and a test exist" bash -c "cd $name && test -f firn.package && test -f README.md && ls tests/*.fi >/dev/null && ! grep -rq __NAME__ ."
    check "$tpl: fmt -c" bash -c "cd $name && $FI fmt -c \$(find src tests -name '*.fi')"
    check "$tpl: test" bash -c "cd $name && $FI test | grep -q 'failed 0'"
    if [ "$tpl" != lib ]; then
        check "$tpl: build" bash -c "cd $name && $FI build . -o $name"
    fi
done

check "cli: run prints the greeting" bash -c "t_cli/t_cli --name firn --times 2 | grep -c 'hello firn' | grep -qx 2"
check "cli: --help works" bash -c "t_cli/t_cli --help | grep -q usage"
check "game: run prints frames" bash -c "t_game/t_game | grep -q 'score'"
check "web-api: serves /hello/NAME" bash -c "
    t_web_api/t_web_api 18741 > srv.log 2>&1 & pid=\$!
    trap 'kill \$pid 2>/dev/null' EXIT
    for i in 1 2 3 4 5 6 7 8 9 10; do sleep 0.3; curl -sf localhost:18741/health >/dev/null && break; done
    curl -sf localhost:18741/hello/firn | grep -qx 'hello firn'"
check "lib: building a lib reports the missing start" bash -c "cd t_lib && $FI build . -o x 2>&1 | grep -q 'no entry point'"
check "lib: usable as a dependency" bash -c "
    $FI new cli user >/dev/null && cd user && $FI add t_lib ../t_lib >/dev/null &&
    printf 'import std.io\nimport t_lib\nfn main() -> i32 {\n    io.print_line(io.fmt_string(f\"{t_lib.clamp(99, 0, 7)}\"))\n    return 0\n}\n' > src/main.fi &&
    $FI build . -o user && ./user | grep -qx 7"

check "desktop-app: new makes a fUi app" bash -c "$FI new desktop-app t_desk | tail -1 | grep -q 'firn build' && test -f t_desk/fui.app && test -f t_desk/main.fi"
check "desktop-app: native build" bash -c "cd t_desk && $FI build main.fi -o desk && test -x desk"

echo "firn templates: $PASS ok, $FAIL errors"
[ "$FAIL" -eq 0 ]

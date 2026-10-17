#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/hotreload.sh -- DEV LOOP: rebuild and restart a fUi program when its sources change.
#
#     sh tools/fui/hotreload.sh --test                 build + run tools/fui/hotreload_main.fi
#     sh tools/fui/hotreload.sh <main.fi> [state-file] watch <main.fi> and lib/fui/*.fi, rebuild, restart
#
# The program keeps UI state in <state-file> (fui.hotreload state_save / state_load, env FUI_STATE is set
# for it); the new build loads it, so scroll position, selection and focus survive a reload.
# A failed build keeps the OLD program running and prints the compiler output.
# Everything the loop makes lives in one mktemp -d directory that is removed on exit.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
D="$(mktemp -d)"
PID=""
cleanup() {
    if [ -n "$PID" ]; then kill "$PID" 2>/dev/null || true; fi
    rm -rf "$D"
}
trap cleanup EXIT INT TERM

if [ "$1" = "--test" ]; then
    "$FIRNC" --opt-level=dev -o "$D/hotreload_main" tools/fui/hotreload_main.fi
    mkdir "$D/scratch"
    "$D/hotreload_main" "$D/scratch"
    exit $?
fi

SRC="$1"
STATE="${2:-$D/app.state}"
[ -n "$SRC" ] || { echo "usage: hotreload.sh <main.fi> [state-file] | --test"; exit 2; }

stamp() { ls -l --time-style=+%s%N "$SRC" lib/fui/*.fi 2>/dev/null | md5sum | cut -d' ' -f1; }

build_and_start() {
    if "$FIRNC" --opt-level=dev -o "$D/next" "$SRC" >"$D/build.log" 2>&1; then
        if [ -n "$PID" ]; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi
        mv "$D/next" "$D/app"
        FUI_STATE="$STATE" "$D/app" &
        PID=$!
        echo "hotreload: started $SRC (pid $PID, state $STATE)"
    else
        echo "hotreload: build failed, keeping the running program"
        head -20 "$D/build.log"
    fi
}

LAST=""
while true; do
    NOW="$(stamp)"
    if [ "$NOW" != "$LAST" ]; then
        # settle: the same stamp twice in a row (an editor or a formatter may still be writing)
        sleep 0.3
        if [ "$(stamp)" = "$NOW" ]; then
            LAST="$NOW"
            build_and_start
        fi
    fi
    sleep 0.5
done

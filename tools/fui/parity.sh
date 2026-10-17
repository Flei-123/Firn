#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/parity.sh -- HOST PARITY: headless vs X11 vs wasm32-browser vs Wine.
#
#     tools/fui/parity.sh [--no-wasm] [--no-wine]
#
# Reference = the headless frames of tools/fui/app_main.fi (no window).
#   x11   native binary on its own Xvfb (-dpi 96), 0 px must differ   (gate)
#   wasm  tools/wasm/appcheck.py: browser canvas vs the same reference (gate)
#   wine  x86_64-windows .exe under wine64                              (informational, <= 3 %)
# What cannot run prints "NOT TESTED <host>: <reason>" and is not a failure.
# Needs FIRNC (default compiler/target/release/firnc); everything lands in a
# mktemp dir that is removed at the end. Heavy: run via /root/jarvis/bin/heavy.
set -u
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
WASM=1; WINE=1
for a in "$@"; do
    case "$a" in
        --no-wasm) WASM=0 ;;
        --no-wine) WINE=0 ;;
        *) echo "parity: unknown option $a"; exit 2 ;;
    esac
done
T="$(mktemp -d "${TMPDIR:-/tmp}/fui-parity-run.XXXXXX")"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/ref" "$T/bin"
fail=0

echo "== 1. headless reference (tools/fui/app_main.fi) =="
"$FIRNC" --opt-level=dev -o "$T/app" tools/fui/app_main.fi || exit 1
"$T/app" "$T/ref" | tail -1

echo "== 2. native programs =="
"$FIRNC" --opt-level=dev -o "$T/bin/hello" examples/fui/hello_window.fi || exit 1
"$FIRNC" --opt-level=dev -o "$T/bin/form" examples/fui/form.fi || exit 1
if [ "$WINE" = 1 ]; then
    # built one by one: a failing one is reported and skipped (form.fi defines a
    # `send` that collides with the Windows seam's `send`, see roadmap)
    for n in hello form; do
        src="examples/fui/hello_window.fi"
        [ "$n" = form ] && src="examples/fui/form.fi"
        "$FIRNC" --target=x86_64-windows --opt-level=dev -o "$T/bin/$n.exe" "$src" > "$T/win-$n.log" 2>&1 \
            || echo "NOT TESTED wine $n: windows build failed ($(grep -m1 '^error' "$T/win-$n.log"))"
    done
fi

echo "== 3. x11 (+ wine) vs headless =="
if [ "$WINE" = 1 ]; then
    python3 -I tools/fui/parity.py "$T/ref" "$T/bin" --wine || fail=1
else
    python3 -I tools/fui/parity.py "$T/ref" "$T/bin" || fail=1
    echo "NOT TESTED wine: switched off"
fi

echo "== 4. wasm32-browser vs headless (tools/wasm/appcheck.py) =="
if [ "$WASM" = 1 ] && command -v chromium >/dev/null 2>&1 \
        && python3 -c "import websocket" >/dev/null 2>&1; then
    mkdir -p "$T/web"
    cp demos/webapp/index.html demos/webapp/firn.js demos/webapp/DejaVuSans.ttf "$T/web/"
    for ex in hello_window counter form touchpad notes files; do
        "$FIRNC" --opt-level=release-safe --target=wasm32-browser \
            -o "$T/web/$ex.wasm" "examples/fui/$ex.fi" || fail=1
    done
    if W="$T/shots" python3 -I tools/wasm/appcheck.py "$T/web" "$T/ref" > "$T/appcheck.log" 2>&1; then
        :
    else
        fail=1
    fi
    grep -E 'vs native|FAILED|PASSED' "$T/appcheck.log" | sed 's/^/  wasm: /'
else
    echo "NOT TESTED wasm32-browser: chromium / python websocket-client missing, or --no-wasm"
fi

[ "$fail" = 0 ] && echo "HOST PARITY PASSED" || echo "HOST PARITY FAILED"
exit $fail

#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/appdemo.sh -- THE fui.app EXAMPLES IN THE BROWSER, BUILT AND PROVEN.
#
#   1. builds demos/webapp/{hello_window,counter,form,touchpad,notes,files}.wasm out of the SAME
#      sources the native build takes (examples/fui/*.fi): --target=
#      wasm32-browser makes `import fui.apphost` find lib/@web/fui/apphost.fi
#   2. paints the native reference with tools/fui/app_main.fi
#   3. loads each module into a headless Chromium: the first picture has to
#      be the native one pixel for pixel, then the page is operated with
#      mouse and keyboard (tools/wasm/appcheck.py)
#   4. real pointers (tools/wasm/touchcheck.py)
#   5. the launcher kit as a page (tools/wasm/kitcheck.py)
#
# Needs: chromium, python3 with PIL, numpy and websocket-client.
# Usage: bash tools/wasm/appdemo.sh      (exit 0 = all of it held)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
# an own work directory per run (parallel workers shared the fixed /tmp name); removed at the end unless W= is given
if [ -z "${W:-}" ]; then
    W=$(mktemp -d "${TMPDIR:-/tmp}/firn-appdemo.XXXXXX")
    trap 'rm -rf "$W"' EXIT
fi
mkdir -p "$W/ref"
fail=0

echo "== 1. the examples as WebAssembly =="
for ex in hello_window counter form touchpad notes files; do
    "$FIRNC" --opt-level=release-safe --target=wasm32-browser \
        -o "demos/webapp/$ex.wasm" "examples/fui/$ex.fi" || exit 1
    echo "   demos/webapp/$ex.wasm  $(wc -c < "demos/webapp/$ex.wasm") octets"
done

echo "== 2. the native reference (tools/fui/app_main.fi) =="
"$FIRNC" --opt-level=dev -o "$W/app" tools/fui/app_main.fi || exit 1
"$W/app" "$W/ref" | tail -1 | sed 's/^/   /'

echo "== 3. headless Chromium: the same pixels, and the pages operated =="
W="$W/shots" python3 tools/wasm/appcheck.py demos/webapp "$W/ref" || fail=1

echo "== 4. real pointers: touch with several fingers, mouse (r111) =="
python3 tools/wasm/touchcheck.py demos/webapp || fail=1

echo "== 5. the launcher kit (examples/fui/launcher_kit.fi) in the browser =="
# One source, two platforms: tools/fui/kitlive.py proves the native window on an
# Xvfb, tools/wasm/kitcheck.py the same program as a page -- sidebar, tabs,
# tiles, toast, the dialog with its focus trap, the Markdown page and the wheel,
# read off the browser's screenshots. The module is built into $W, not into
# demos/webapp (it is 1 MB and nobody should have to commit it).
"$FIRNC" --opt-level=release-safe --target=wasm32-browser \
    -o "$W/launcher_kit.wasm" examples/fui/launcher_kit.fi || exit 1
W="$W/shots-kit" python3 tools/wasm/kitcheck.py "$W/launcher_kit.wasm" demos/webapp || fail=1

[ "$fail" = "0" ] && echo "APPDEMO PASSED" || echo "APPDEMO FAILED"
exit $fail

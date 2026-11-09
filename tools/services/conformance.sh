#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
if [ -z "${FIRNC:-}" ]; then
    FIRNC="$ROOT/compiler/target/release/firnc"
    [ -x "$FIRNC" ] || FIRNC="$HOME/.firn/current/bin/firnc"
fi
LEVELS=${LEVELS:-"dev release-safe release-fast"}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-services.XXXXXX")
trap 'rm -rf "$W"' EXIT
SRC="$ROOT/tools/services/conformance.fi"
CAPS_ALL_NATIVE=191
CAPS_HEADLESS=160
CAPS_WEB=127
fails=0

make_fakes() {
    local fake="$1"
    mkdir -p "$fake/bin" "$fake/home"
    printf 'alpha' > "$fake/a.txt"
    printf 'beta' > "$fake/b.txt"
    cat > "$fake/bin/zenity" <<'SH'
#!/bin/sh
case " $* " in
*" --file-selection "*)
    case " $* " in *" --save "*) echo "$FAKE_DIR/out.txt"; exit 0 ;; esac
    [ -e "$FAKE_DIR/opened" ] && exit 1
    : > "$FAKE_DIR/opened"
    printf '%s\n%s\n' "$FAKE_DIR/a.txt" "$FAKE_DIR/b.txt"
    exit 0 ;;
esac
exit 0
SH
    cat > "$fake/bin/xclip" <<'SH'
#!/bin/sh
case " $* " in *" -out "*) cat "$FAKE_DIR/clip" 2>/dev/null; exit 0 ;; esac
cat > "$FAKE_DIR/clip"
SH
    chmod +x "$fake/bin/zenity" "$fake/bin/xclip"
}

expect() {
    local name="$1" log="$2" rc="$3" want="$4"
    local last
    last=$(tail -n 1 "$log")
    if [ "$rc" -eq 0 ] && printf '%s' "$last" | grep -q "caps $want: .* 0 failures"; then
        echo "PASS $name: $last"
    else
        echo "FAIL $name (rc $rc): $last"
        grep '^FAIL' "$log" | head -n 20
        fails=$((fails + 1))
    fi
}

run_native() {
    local level="$1" bin="$W/native-$level"
    if ! "$FIRNC" --opt-level="$level" "$SRC" -o "$bin" > "$W/build-$level.log" 2>&1; then
        echo "FAIL native $level: build"; tail -n 5 "$W/build-$level.log"; fails=$((fails + 1)); return
    fi
    local fake="$W/fake-$level"
    make_fakes "$fake"
    env -u DISPLAY -u WAYLAND_DISPLAY APPKIT_HOME="$fake/home" "$bin" > "$W/headless-$level.log" 2>&1
    expect "native $level headless" "$W/headless-$level.log" $? "$CAPS_HEADLESS"
    env -u WAYLAND_DISPLAY DISPLAY=:services-fake FIRN_DIALOG=zenity FIRN_CLIPBOARD=xclip FAKE_DIR="$fake" \
        PATH="$fake/bin:/usr/bin:/bin" APPKIT_HOME="$fake/home" "$bin" > "$W/helpers-$level.log" 2>&1
    expect "native $level helpers" "$W/helpers-$level.log" $? "$CAPS_ALL_NATIVE"
    if [ "$(cat "$fake/out.txt" 2>/dev/null)" != "saved by services" ]; then
        echo "FAIL native $level: saved file content"; fails=$((fails + 1))
    fi
}

run_wasm() {
    local level="$1" mod="$W/web-$level.wasm"
    if ! "$FIRNC" --target=wasm32-browser --opt-level="$level" "$SRC" -o "$mod" > "$W/wbuild-$level.log" 2>&1; then
        echo "FAIL wasm $level: build"; tail -n 5 "$W/wbuild-$level.log"; fails=$((fails + 1)); return
    fi
    mkdir -p "$W/out-$level"
    FIRN_SERVICES_OUT="$W/out-$level" node "$ROOT/tools/wasm/run.mjs" "$mod" < /dev/null > "$W/web-$level.log" 2>&1
    expect "wasm $level" "$W/web-$level.log" $? "$CAPS_WEB"
    if [ "$(cat "$W/out-$level/out.txt" 2>/dev/null)" != "saved by services" ]; then
        echo "FAIL wasm $level: downloaded file content"; fails=$((fails + 1))
    fi
}

for level in $LEVELS; do
    run_native "$level"
    run_wasm "$level"
done
echo "services conformance: $fails failing runs"
[ "$fails" -eq 0 ]

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
CAPS_ALL_NATIVE=703
CAPS_HEADLESS=672
CAPS_WEB=511
CAPS_WINDOWS_HEADLESS=684
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

run_windows() {
    local level="$1" exe="$W/win-$level.exe"
    if ! command -v wine > /dev/null || ! command -v xvfb-run > /dev/null; then
        echo "SKIP windows $level: wine or xvfb-run missing"; return
    fi
    if ! "$FIRNC" --target=x86_64-windows --opt-level="$level" "$SRC" -o "$exe" > "$W/winbuild-$level.log" 2>&1; then
        echo "FAIL windows $level: build"; tail -n 5 "$W/winbuild-$level.log"; fails=$((fails + 1)); return
    fi
    ( cd "$W" && WINEPREFIX="$W/wineprefix" WINEDEBUG=-all FIRN_SERVICES_HEADLESS=1 \
        timeout 300 xvfb-run -a wine "$exe" > "$W/win-raw-$level.log" 2>&1 )
    local rc=$?
    WINEPREFIX="$W/wineprefix" wineserver -k > /dev/null 2>&1
    grep -E '^(ok|FAIL|services)' "$W/win-raw-$level.log" > "$W/win-$level.log"
    expect "windows $level headless" "$W/win-$level.log" $rc "$CAPS_WINDOWS_HEADLESS"
    run_windows_dialogs "$level"
}

run_windows_dialogs() {
    local level="$1" live="$W/win-live-$level.exe" pilot="$W/win-pilot.exe"
    if ! "$FIRNC" --target=x86_64-windows --opt-level="$level" "$ROOT/tools/services/win_live_main.fi" -o "$live" > "$W/winlive-build-$level.log" 2>&1 \
        || ! "$FIRNC" --target=x86_64-windows --opt-level="$level" "$ROOT/tools/dialog/pilot_main.fi" -o "$pilot" >> "$W/winlive-build-$level.log" 2>&1; then
        echo "FAIL windows $level dialogs: build"; tail -n 5 "$W/winlive-build-$level.log"; fails=$((fails + 1)); return
    fi
    export WINEPREFIX="${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn"
    timeout 600 python3 "$ROOT/tools/services/win_live.py" "$live" "$pilot" > "$W/winlive-$level.log" 2>&1
    local rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "PASS windows $level dialogs: $(tail -n 1 "$W/winlive-$level.log")"
    else
        echo "FAIL windows $level dialogs (rc $rc)"; grep -v '^  OK' "$W/winlive-$level.log" | tail -n 20; fails=$((fails + 1))
    fi
}

compile_android() {
    local target="$1"
    if "$FIRNC" --target="$target" --pic -c "$SRC" -o "$W/android-$target.o" > "$W/android-$target.log" 2>&1; then
        echo "PASS android $target: compiles"
    else
        echo "FAIL android $target: build"; tail -n 5 "$W/android-$target.log"; fails=$((fails + 1))
    fi
}

for level in $LEVELS; do
    run_native "$level"
    run_wasm "$level"
done
run_windows "${WINE_LEVEL:-dev}"
compile_android x86_64-android
compile_android aarch64-android
echo "services conformance: $fails failing runs"
[ "$fails" -eq 0 ]

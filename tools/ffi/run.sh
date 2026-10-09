#!/usr/bin/env bash
# Round FFI -- std.dynlib against the real thing (docs/FFI.md).
#   1. Linux: libc + libm through dlopen/dlsym, in all four build levels
#      (tests/2260 also runs in the main loop); a static program stays static.
#   2. Linux: an OpenGL ES 2 context through dlopen'd libEGL + libGLESv2,
#      first headless (EGL_PLATFORM=surfaceless), then on an X display (Xvfb).
#   3. Windows under Wine: LoadLibraryA/GetProcAddress and the Win64 call gate
#      (kernel32, msvcrt floats, user32), then a real wgl context (opengl32)
#      on Xvfb.
# Every step whose environment is missing prints SKIP and does not fail.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
D="$(mktemp -d)"
XPIDS=""
cleanup() {
    if [ -d "$D/wineprefix" ] && command -v wineserver >/dev/null 2>&1; then
        WINEPREFIX="$D/wineprefix" wineserver -k 2>/dev/null
        sleep 1
    fi
    for p in $XPIDS; do kill "$p" 2>/dev/null; done
    rm -rf "$D" 2>/dev/null || { sleep 2; rm -rf "$D"; }
}
trap cleanup EXIT
fail=0
ok()   { echo "   ok:   $1"; }
bad()  { echo "   FAIL: $1"; fail=1; }
skip() { echo "   SKIP: $1"; }

# A private Xvfb on a free display; sets DISP ("" if there is none).
start_xvfb() {
    DISP=""
    command -v Xvfb >/dev/null 2>&1 || return 0
    local fifo="$D/disp.$1"
    mkfifo "$fifo" 2>/dev/null || return 0
    Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$fifo" >/dev/null 2>&1 &
    XPIDS="$XPIDS $!"
    read -r -t 20 n <"$fifo" || n=""
    [ -n "$n" ] && DISP=":$n"
}

echo "-- Linux: libc and libm through dlopen / dlsym, four build levels"
for lvl in release-fast release-safe dev-fast no-opt; do
    flag="--opt-level=$lvl"; [ "$lvl" = no-opt ] && flag="--no-opt"
    if ! "$FIRNC" $flag -o "$D/dl.$lvl" "$ROOT/tests/2260_dynlib_linux.fi" >"$D/c.log" 2>&1; then
        bad "$lvl: compile failed: $(head -3 "$D/c.log")"; continue
    fi
    timeout -s KILL 30 "$D/dl.$lvl"; rc=$?
    [ "$rc" -eq 42 ] && ok "$lvl: exit 42" || bad "$lvl: exit $rc, expected 42"
done
if readelf -d "$D/dl.release-fast" 2>/dev/null | grep -q 'NEEDED.*libc.so.6'; then
    ok "image is dynamically linked (DT_NEEDED libc.so.6)"
else
    bad "no DT_NEEDED libc.so.6 in the image"
fi
echo 'fn main() -> i32 { return 7 }' >"$D/s.fi"
"$FIRNC" -o "$D/s" "$D/s.fi" && if readelf -l "$D/s" 2>/dev/null | grep -q INTERP; then
    bad "a program without #[link_lib] became dynamic"
else
    ok "a program without #[link_lib] stays static"
fi

echo "-- Linux: OpenGL ES 2 context through dlopen'd libEGL + libGLESv2"
if ! command -v readelf >/dev/null 2>&1 || ! ldconfig -p 2>/dev/null | grep -q 'libEGL.so.1'; then
    skip "libEGL.so.1 is not installed"
else
    "$FIRNC" -o "$D/egl" "$ROOT/tools/ffi/egl_probe.fi" >"$D/c.log" 2>&1 || bad "egl_probe compile: $(head -3 "$D/c.log")"
    if [ -x "$D/egl" ]; then
        EGL_PLATFORM=surfaceless timeout -s KILL 60 "$D/egl"; rc=$?
        [ "$rc" -eq 0 ] && ok "surfaceless: red pixel read back" || {
            [ "$rc" -ge 21 ] && [ "$rc" -le 25 ] && skip "surfaceless EGL not available here (exit $rc)" || bad "surfaceless: exit $rc"; }
        start_xvfb egl
        if [ -n "$DISP" ]; then
            env -u EGL_PLATFORM DISPLAY="$DISP" timeout -s KILL 60 "$D/egl"; rc=$?
            [ "$rc" -eq 0 ] && ok "Xvfb ($DISP): red pixel read back" || bad "Xvfb: exit $rc"
        else
            skip "Xvfb is missing"
        fi
    fi
fi

echo "-- Windows under Wine: LoadLibrary / GetProcAddress / Win64 call gate"
WINE=""
for c in wine wine64 /usr/lib/wine/wine64; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
done
if [ -z "$WINE" ] || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    skip "wine or the mingw linker is missing"
else
    export WINEPREFIX="$D/wineprefix" WINEDEBUG=-all
    start_xvfb win
    if [ -z "$DISP" ]; then
        skip "Xvfb is missing (Wine needs a display)"
    else
        for t in win_probe win_gl_probe; do
            if ! "$FIRNC" --target=x86_64-windows -o "$D/$t.exe" "$ROOT/tools/ffi/$t.fi" >"$D/c.log" 2>&1; then
                bad "$t: compile failed: $(head -3 "$D/c.log")"; continue
            fi
            DISPLAY="$DISP" timeout -s KILL 180 "$WINE" "$D/$t.exe" >/dev/null 2>&1; rc=$?
            [ "$rc" -eq 0 ] && ok "$t under Wine: exit 0" || bad "$t under Wine: exit $rc"
        done
    fi
fi
[ "$fail" -eq 0 ] && echo "ffi: all steps passed or skipped" || echo "ffi: FAILED"
exit "$fail"

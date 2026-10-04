#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/pack.sh -- the front door of the packaging tools.
#
#   bash tools/pack/pack.sh stubs                  build the Firn parts: icons tool, Windows installer stub,
#                                                  Linux self-extract stub (into build/pack/)
#   bash tools/pack/pack.sh icons IN.svg|png OUTDIR [--name N] [--bg RRGGBB]
#   bash tools/pack/pack.sh all APP-DIR VERSION [options]       everything for every platform (all.sh)
#   bash tools/pack/pack.sh <command> ...          any command of pack.py (win-installer, deb, appimage, ...;
#                                                  `pack.sh --help` lists them)
#
# Needs: the compiler (compiler/target/release/firnc), python3 (standard library only).
# Optional: x86_64-w64-mingw32 binutils (the Windows stubs), xorriso (.dmg), makensis (NSIS build),
# the Android SDK/NDK (APK).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
OUT=${PACK_BUILD:-$ROOT/build/pack}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}

newer() { # newer <target> <source dir/file>...: is any source newer than the target (or the target missing)?
    local t=$1; shift
    [ -f "$t" ] || return 0
    [ -n "$(find "$@" -newer "$t" -type f 2>/dev/null | head -1)" ]
}

build_icons() {
    mkdir -p "$OUT"
    if newer "$OUT/icons" "$HERE/icons.fi" "$ROOT/lib/pack" "$ROOT/lib/svg" "$ROOT/lib/paint"; then
        "$FIRNC" --opt-level=release-fast -o "$OUT/icons" "$HERE/icons.fi" >/dev/null 2>"$OUT/icons.log" \
            || { cat "$OUT/icons.log" >&2; echo "pack: the icons tool did not build" >&2; exit 1; }
    fi
}

build_stubs() {
    mkdir -p "$OUT"
    local what=${1:-all}
    build_icons
    if newer "$OUT/selfx" "$HERE/stub/selfx.fi" "$ROOT/lib/pack" "$ROOT/lib/std"; then
        "$FIRNC" --opt-level=release-fast -o "$OUT/selfx" "$HERE/stub/selfx.fi" >/dev/null 2>"$OUT/selfx.log" \
            || { cat "$OUT/selfx.log" >&2; echo "pack: the self-extract stub did not build" >&2; exit 1; }
    fi
    if command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
        if newer "$OUT/setup-stub.exe" "$HERE/installer" "$ROOT/lib/pack" "$ROOT/lib/fui" "$ROOT/lib/window" "$ROOT/lib/@windows" "$ROOT/compiler/target/release/firnc"; then
            ( cd "$HERE/installer" && "$FIRNC" --target=x86_64-windows --win-subsystem=windows \
                --opt-level=release-fast -o "$OUT/setup-stub.exe" installer.fi ) >/dev/null 2>"$OUT/setup.log" \
                || { cat "$OUT/setup.log" >&2
                     grep -q 'undefined reference to `Reg\|SHChangeNotify' "$OUT/setup.log" \
                         && echo "pack: the compiler is older than compiler/src/win.rs (registry imports): cargo build --release in compiler/" >&2
                     echo "pack: the installer stub did not build" >&2; exit 1; }
        fi
    else
        echo "pack: no mingw binutils: the Windows installer stub is not built" >&2
    fi
    echo "stubs in $OUT"
}

cmd=${1:-}
case "$cmd" in
    ""|-h|--help)
        sed -n '3,15p' "$0" | sed 's/^# \{0,1\}//'; echo; python3 "$HERE/pack.py" --help | sed -n '1,40p'; exit 0 ;;
    stubs) build_stubs; exit 0 ;;
    icons) shift; build_icons; exec "$OUT/icons" "$@" ;;
    all) shift; exec bash "$HERE/all.sh" "$@" ;;
    android) shift; exec bash "$HERE/android.sh" "$@" ;;
    *) exec python3 "$HERE/pack.py" "$@" ;;
esac

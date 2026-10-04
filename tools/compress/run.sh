#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/compress/run.sh -- lib/compress held against the reference tools (see
# check.py and docs/COMPRESSION.md): libzstd (the `zstd` command), liblzma (`xz`,
# python lzma), libbz2, libbrotli, liblz4 (python lz4), zlib/gzip.
#
#   1. the probe built in dev-fast (the default level): the full corpus, every
#      format, both directions, whole buffer and streaming, dictionaries, the
#      damaged-input sweep and the bomb
#   2. the probe in release-fast and release-safe: decode and encode (release-fast
#      is where the xxHash miscompilation of docs/COMPRESSION.md showed up)
#   3. net.http's Content-Encoding br/zstd/gzip/deflate against python's http.server
#   4. the AArch64 build under qemu-aarch64 and the Windows build under Wine (a
#      reduced corpus: CHECK_QUICK=1), when the tools are there
#
# COMPRESS_QUICK=1 runs step 1 and 2 with the reduced corpus too.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
need() { command -v "$1" >/dev/null 2>&1 || { echo "   SKIP: $1 is missing"; exit 0; }; }
need zstd
need xz
python3 -c "import lz4.frame, brotli" 2>/dev/null || { echo "   SKIP: python lz4 and brotli are missing"; exit 0; }
[ -n "${COMPRESS_QUICK:-}" ] && export CHECK_QUICK=1

build() { # $1 = name, rest = flags
    local name=$1; shift
    if ! "$FIRNC" "$@" -o "$W/$name" tools/compress/probe.fi > "$W/$name.log" 2>&1; then
        echo "  FAIL the probe does not build ($name)"; grep -v RWX "$W/$name.log" | head -8; return 1
    fi
}

echo "-- dev-fast: the full corpus (decode, encode, hostile, dictionaries)"
build probe_dev || exit 1
python3 tools/compress/check.py "$W/probe_dev" || rc=1

echo "-- release-fast and release-safe: decode and encode"
build probe_fast --opt-level=release-fast || exit 1
build probe_safe --opt-level=release-safe || exit 1
CHECK_PARTS=decode,encode python3 tools/compress/check.py "$W/probe_fast" || rc=1
CHECK_PARTS=decode,encode python3 tools/compress/check.py "$W/probe_safe" || rc=1

echo "-- net.http: Content-Encoding"
if "$FIRNC" -o "$W/http_main" lib/net/http_main.fi > "$W/http_main.log" 2>&1; then
    python3 tools/compress/http_check.py "$W/http_main" || rc=1
else
    echo "  FAIL http_main does not build"; grep -v RWX "$W/http_main.log" | head -5; rc=1
fi

echo "-- AArch64 under qemu-aarch64 (reduced corpus)"
if command -v qemu-aarch64 >/dev/null 2>&1; then
    if build probe_a64 --target=aarch64-linux; then
        CHECK_QUICK=1 RUNNER=qemu-aarch64 python3 tools/compress/check.py "$W/probe_a64" || rc=1
    else
        rc=1
    fi
else
    echo "   SKIP: qemu-aarch64 is missing"
fi

echo "-- Windows under Wine (reduced corpus)"
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine wine64 /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
    export WINEDEBUG=${WINEDEBUG:--all}
    if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$W/probe_win.exe" tools/compress/probe.fi > "$W/probe_win.log" 2>&1; then
        CHECK_QUICK=1 RUNNER="$WINE" python3 tools/compress/check.py "$W/probe_win.exe" || rc=1
    else
        echo "  FAIL the probe does not build for Windows"; grep -v RWX "$W/probe_win.log" | head -5; rc=1
    fi
else
    echo "   SKIP: Wine or mingw is missing"
fi
exit $rc

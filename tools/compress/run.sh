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

echo "-- Windows under Wine (smoke test: decode and encode of a few small inputs)"
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
        # CHECK_QUICK=3 and decode+encode only: a smoke test of the platform seam (files, descriptors,
        # the clock-free code paths) -- every probe run is a Wine process start
        # r572: one check.py per format, two at a time on the ONE Wine prefix (each has its own temp
        # directory; the formats are independent). The serial run was dominated by Wine start-up
        # latency, not CPU. WINE_JOBS=1 gives the old serial order.
        WJ=${WINE_JOBS:-2}
        # The real cost: wine starts a fresh wineserver (plus wineboot, services) for EVERY probe
        # run and tears it down afterwards (~4-5 s each, 0.4 s with a warm server). Keep one
        # persistent server for the whole section and stop it again at the end. Wine stages share
        # ~/.wine-firn and never run in parallel with each other, so killing it here is safe.
        wineserver -p 2>/dev/null || true
        # warm up ONCE before the parallel jobs: two cold starts at the same time raced on the
        # first process (a 120 s TIMEOUT on the very first probe run was seen)
        timeout 120 "$WINE" cmd /c exit >/dev/null 2>&1 || true
        trap 'wineserver -k 2>/dev/null || true; rm -rf "$W"' EXIT
        FMTS=(lz4 zstd xz lzma bz2 br gz auto)
        WPIDS=()
        for ((wi = 0; wi < ${#FMTS[@]}; wi++)); do
            CHECK_QUICK=3 CHECK_PARTS=decode,encode RUNNER="$WINE" python3 tools/compress/check.py "$W/probe_win.exe" "${FMTS[$wi]}" > "$W/wine_${FMTS[$wi]}.log" 2>&1 &
            WPIDS+=($!)
            if (( (wi + 1) % WJ == 0 )); then
                for wp in "${WPIDS[@]}"; do wait "$wp" || rc=1; done
                WPIDS=()
            fi
        done
        for wp in "${WPIDS[@]}"; do wait "$wp" || rc=1; done
        for f in "${FMTS[@]}"; do cat "$W/wine_$f.log"; done
    else
        echo "  FAIL the probe does not build for Windows"; grep -v RWX "$W/probe_win.log" | head -5; rc=1
    fi
else
    echo "   SKIP: Wine or mingw is missing"
fi
exit $rc

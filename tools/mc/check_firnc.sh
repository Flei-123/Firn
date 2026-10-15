#!/usr/bin/env bash
# tools/mc/check_firnc.sh [firnc] -- r220: refuse a STALE compiler before the GL stages. A firnc built before r167 (thread block behind gs for
# glibc-linked programs) replaces the main thread's fs base when the mesher threads start, so the first Mesa/libGLdispatch TLS access
# (glBindFramebuffer in the first frame of mcplay MCPLAY_GL=1) is a SIGSEGV. tests/2993 is the probe: it must exit 42.
# Fix for a stale binary: (cd /root/firn/compiler && cargo build --release) via heavy. Exit 0 = ok, 1 = stale / broken.
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
if ! FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$D/probe" tests/2993_dynlib_thread_tls.fi 2>"$D/err"; then
    head -5 "$D/err"; echo "  FAIL  check_firnc: probe 2993 does not compile with $FIRNC"; exit 1
fi
"$D/probe" >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 42 ]; then
    echo "  FAIL  check_firnc: $FIRNC is STALE (tests/2993 exit $rc, want 42): GL stages would SIGSEGV in the first frame."
    echo "        rebuild: heavy bash -c 'cd /root/firn/compiler && cargo build --release'"
    exit 1
fi
echo "  ok    check_firnc: $FIRNC has the fs/gs thread-block fix (r167)"

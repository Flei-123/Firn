#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proof of the RESULT-LOCATION GUARANTEE (SPEC.md 13.1, DESIGN_GOALS.md 6).
#
# Claim: with `let g = build(...)` and an aggregate return type the
# target address is passed through. The structure comes into being EXACTLY ONCE in the
# frame of the caller -- not additionally in the frame of the producing function and not
# through a copy.
#
# It is checked on the emitted assembly:
#   1. The frame of `build` is SMALL (< 64 KB) although the structure has 1 MB.
#   2. The frame of `main` is about 1 MB (exactly one instance).
#   3. There is no bulk copy (`rep movs`) -- nothing is shovelled around.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
SRC="tests/120_result_location.fi"
ASM="$(mktemp /tmp/result_location.XXXXXX.s)"
trap 'rm -f "$ASM"' EXIT

MB=1048576
"$FIRNC" --emit=asm -o "$ASM" "$SRC"

# Linker symbol: 'main' keeps its bare name, everything else carries the
# scheme from modules.rs (_F<scheme>.<name>, DESIGN_GOALS 4).
frame() {   # $1 = function name -> the number of bytes from 'sub rsp, N'
    awk -v n="$1" '
        $0 == n":" || $0 ~ "^_F[0-9]+\\." n ":" { inf = 1; next }
        inf && /sub rsp,/  { gsub(/,/, "", $3); print $3; exit }
    ' "$ASM"
}

R_BUILD=$(frame build)
R_MAIN=$(frame main)
COPIES=$(grep -c 'rep movs' "$ASM" || true)

echo "frame build: ${R_BUILD:-?} bytes   frame main: ${R_MAIN:-?} bytes   rep-movs: $COPIES"

ERRORS=0
if [ -z "${R_BUILD:-}" ] || [ -z "${R_MAIN:-}" ]; then
    echo "ERROR: frame size not found -- has the assembly format changed?"; exit 1
fi
if [ "$R_BUILD" -ge 65536 ]; then
    echo "ERROR: 'build' builds the 1 MB structure on its own stack ($R_BUILD bytes)."
    echo "        The result-location guarantee from SPEC.md 13.1 is violated."
    ERRORS=1
fi
if [ "$R_MAIN" -lt "$MB" ] || [ "$R_MAIN" -gt $((2 * MB)) ]; then
    echo "ERROR: 'main' has a frame of $R_MAIN bytes, expected ~$MB (exactly one instance)."
    ERRORS=1
fi
if [ "$COPIES" -ne 0 ]; then
    echo "ERROR: $COPIES bulk copies in the assembly -- it is shovelled around instead of built at the target."
    ERRORS=1
fi

[ "$ERRORS" -eq 0 ] || exit 1
echo "OK: result-location guarantee kept (build $R_BUILD B, main $R_MAIN B, no bulk copy)."

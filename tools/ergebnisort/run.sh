#!/usr/bin/env bash
# Proof of the RESULT-LOCATION GUARANTEE (SPEC.md 13.1, DESIGNZIELE.md 6).
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
SRC="tests/120_ergebnisort.fi"
ASM="$(mktemp /tmp/ergebnisort.XXXXXX.s)"
trap 'rm -f "$ASM"' EXIT

MB=1048576
"$FIRNC" --emit=asm -o "$ASM" "$SRC"

# Linker symbol: 'main' keeps its bare name, everything else carries the
# scheme from modules.rs (_F<scheme>.<name>, DESIGNZIELE 4).
rahmen() {   # $1 = Funktionsname -> Byte-Zahl aus 'sub rsp, N'
    awk -v n="$1" '
        $0 == n":" || $0 ~ "^_F[0-9]+\\." n ":" { inf = 1; next }
        inf && /sub rsp,/  { gsub(/,/, "", $3); print $3; exit }
    ' "$ASM"
}

R_BUILD=$(rahmen build)
R_MAIN=$(rahmen main)
KOPIEN=$(grep -c 'rep movs' "$ASM" || true)

echo "Rahmen build: ${R_BUILD:-?} Byte   Rahmen main: ${R_MAIN:-?} Byte   rep-movs: $KOPIEN"

FEHLER=0
if [ -z "${R_BUILD:-}" ] || [ -z "${R_MAIN:-}" ]; then
    echo "FEHLER: Rahmengroesse nicht gefunden — Assembler-Format geaendert?"; exit 1
fi
if [ "$R_BUILD" -ge 65536 ]; then
    echo "FEHLER: 'build' baut die 1-MB-Struktur auf dem eigenen Stapel ($R_BUILD Byte)."
    echo "        Die Ergebnisort-Garantie aus SPEC.md §13.1 ist verletzt."
    FEHLER=1
fi
if [ "$R_MAIN" -lt "$MB" ] || [ "$R_MAIN" -gt $((2 * MB)) ]; then
    echo "FEHLER: 'main' hat $R_MAIN Byte Rahmen, erwartet ~$MB (genau eine Ausfertigung)."
    FEHLER=1
fi
if [ "$KOPIEN" -ne 0 ]; then
    echo "FEHLER: $KOPIEN Bulk-Kopien im Assembler — es wird umgeschaufelt statt am Ziel gebaut."
    FEHLER=1
fi

[ "$FEHLER" -eq 0 ] || exit 1
echo "OK: Ergebnisort-Garantie gehalten (build $R_BUILD B, main $R_MAIN B, keine Bulk-Kopie)."

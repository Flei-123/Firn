#!/usr/bin/env bash
# Nachweis der ERGEBNISORT-GARANTIE (SPEC.md §13.1, DESIGNZIELE.md §6).
#
# Behauptung: Bei `let g = build(…)` mit aggregiertem Rueckgabetyp wird die
# Zieladresse durchgereicht. Die Struktur entsteht GENAU EINMAL im Rahmen des
# Aufrufers — nicht zusaetzlich im Rahmen der erzeugenden Funktion und nicht
# per Kopie.
#
# Geprueft wird am erzeugten Assembler:
#   1. Der Rahmen von `build` ist KLEIN (< 64 KB), obwohl die Struktur 1 MB hat.
#   2. Der Rahmen von `main` ist etwa 1 MB (genau eine Ausfertigung).
#   3. Es gibt keine Bulk-Kopie (`rep movs`) — nichts wird umgeschaufelt.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
SRC="tests/120_ergebnisort.fi"
ASM="$(mktemp /tmp/ergebnisort.XXXXXX.s)"
trap 'rm -f "$ASM"' EXIT

MB=1048576
"$FIRNC" --emit=asm -o "$ASM" "$SRC"

# Linker-Symbol: 'main' behaelt seinen nackten Namen, alles andere traegt das
# Schema aus modules.rs (_F<schema>.<name>, DESIGNZIELE 4).
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

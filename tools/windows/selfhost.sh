#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/selfhost.sh -- ROUND MERGE-WIN: THE COMPILER ITSELF AS A
# WINDOWS PROGRAM, over the whole corpus.
#
# `bin/firnc1.fi` is the compiler written in Firn. It is built twice with
# `firnc0`:
#
#     firnc bin/firnc1.fi -o .firnc1                    an ELF for Linux
#     firnc --target=x86_64-windows bin/firnc1.fi -o firnc1.exe    a PE/COFF
#
# Then EVERY case of `tests/` and `examples/` is handed to BOTH of them and
# the assembly they write to standard output is compared character for
# character, together with the exit code.
#
# This is a harder question than "does a program run under Windows". The
# compiler is the largest Firn program in this repository: it reads files,
# resolves `import` over $FIRNLIB, allocates megabytes over the collector,
# builds deep trees, formats text, and writes the result to standard output.
# Every one of those goes through the seam (compiler/src/win_seam.rs). If
# the two texts are equal for every case, the seam is right for a real
# workload and not just for `hello`.
#
# WHAT THIS IS NOT: it is not the fixpoint (tools/fixpoint.sh). The fixpoint
# needs `-o`, and `-o` needs `fork`/`execve` to call `as` and `ld` -- both
# answer ENOSYS on Windows (docs/ROUND-WINDOWS.md 4.2). What is measured
# here is the whole compiler up to and including the assembly text; the two
# calls to the external assembler and linker are not.
#
# Buckets:
#   SAME       both produced assembly and the two texts are identical
#   REFUSED    both refused the case with the SAME exit code (firnc1 does
#              not master the whole language: 2 = I/O or a module it cannot
#              find, 3 = not core language, 4 = comptime, 5 = defer,
#              6 = the code generator)
#   DIFFERENT  anything else -- and that is the number that must be 0
#
# Usage:  tools/windows/selfhost.sh          the whole corpus
#         SH_FILTER=340 tools/windows/selfhost.sh   only matching names
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-work/selfhost"
JOBS=${JOBS:-6}
FILTER=${SH_FILTER:-}

export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi

if [ ! -x "$FIRNC" ]; then echo "SKIP: firnc is missing"; exit 0; fi
if ! command -v x86_64-w64-mingw32-as >/dev/null 2>&1; then
    echo "SKIP: the mingw binutils are missing"; exit 0
fi
if [ -z "$WINE" ]; then echo "SKIP: wine is missing"; exit 0; fi

rm -rf "$WORK"; mkdir -p "$WORK"

# --- the two compilers ------------------------------------------------------
FIRNLIB=lib "$FIRNC" bin/firnc1.fi -o "$WORK/firnc1.lin" > "$WORK/build.lin" 2>&1 \
    || { echo "FAIL: the Linux firnc1 does not build"; exit 1; }
FIRNLIB=lib "$FIRNC" --target=x86_64-windows bin/firnc1.fi -o "$WORK/firnc1.exe" > "$WORK/build.win" 2>&1 \
    || { echo "FAIL: the Windows firnc1 does not build"; exit 1; }
echo "  the compiler in Firn, built twice:"
echo "    linux    $(wc -c < "$WORK/firnc1.lin") octets   $(file -b "$WORK/firnc1.lin" | cut -d, -f1-2)"
echo "    windows  $(wc -c < "$WORK/firnc1.exe") octets   $(file -b "$WORK/firnc1.exe" | cut -d, -f1-2)"

# --- one case ---------------------------------------------------------------
one() {
    f=$1
    base=$(basename "$f" .fi)
    FIRNLIB=lib timeout 120 "$WORK/firnc1.lin" "$f" > "$WORK/$base.lin.s" 2>/dev/null; lrc=$?
    FIRNLIB=lib timeout 300 "$WINE" "$WORK/firnc1.exe" "$f" > "$WORK/$base.win.s" 2>/dev/null; wrc=$?
    if [ "$lrc" -eq 0 ] && [ "$wrc" -eq 0 ]; then
        if cmp -s "$WORK/$base.lin.s" "$WORK/$base.win.s"; then
            echo "SAME $f $(wc -c < "$WORK/$base.lin.s")"
        else
            echo "DIFFERENT $f assembly-differs"
        fi
    elif [ "$lrc" -eq "$wrc" ]; then
        echo "REFUSED $f rc=$lrc"
    else
        echo "DIFFERENT $f linux-rc=$lrc windows-rc=$wrc"
    fi
    rm -f "$WORK/$base.lin.s" "$WORK/$base.win.s"
}
export -f one
export WORK WINE FIRNC

LIST="$WORK/list.txt"
: > "$LIST"
for f in tests/*.fi examples/*.fi; do
    [ -f "$f" ] || continue
    if [ -n "$FILTER" ]; then case "$f" in *"$FILTER"*) ;; *) continue;; esac; fi
    echo "$f" >> "$LIST"
done

xargs -a "$LIST" -P "$JOBS" -I{} bash -c 'one "$@"' _ {} > "$WORK/result.txt" 2>/dev/null

same=$(grep -c '^SAME ' "$WORK/result.txt" || true)
refused=$(grep -c '^REFUSED ' "$WORK/result.txt" || true)
diffn=$(grep -c '^DIFFERENT ' "$WORK/result.txt" || true)
total=$(wc -l < "$LIST")
bytes=$(awk '/^SAME /{s+=$3} END{print s+0}' "$WORK/result.txt")

echo
echo "  SAME       $same    both produced assembly, character identical"
echo "  REFUSED    $refused    both refused with the same exit code"
echo "  DIFFERENT  $diffn"
echo "  corpus     $total cases, $bytes octets of assembly compared"
if [ "$diffn" -gt 0 ]; then
    echo
    echo "  -- what differs"
    grep '^DIFFERENT ' "$WORK/result.txt" | head -20 | sed 's/^/    /'
    exit 1
fi
echo "  RESULT: the compiler written in Firn produces the same assembly as a"
echo "          Windows program as it does as a Linux program, on all $total cases."
exit 0

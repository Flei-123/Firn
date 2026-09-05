#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/programs.sh -- ROUND MERGE-WIN: the REAL programs of this
# repository, built as Windows `.exe` and RUN.
#
# `tools/windows/run.sh` answers "does a test case behave the same on both
# operating systems". This script asks the question that comes after it:
#
#     do the tools this project actually ships work as Windows programs?
#
# The list is `bin/*.fi` -- the compiler written in Firn (`firnc1`) and the
# six dump tools around it -- plus the examples. Every one of them is built
# TWICE, run TWICE, and what is compared is what the program does: its
# standard output character for character and its exit code.
#
# What each case additionally proves, beyond "it starts":
#
#   sysdump      no argument at all, pure `write` to handle 1
#   lexdump      the source comes from STANDARD INPUT (ReadFile on a
#                redirected handle, not on a file the program opened)
#   astdump      the file name comes from `argv[1]` (GetCommandLineW ->
#                the start block) and is opened (CreateFileW)
#   firdump      the same, one compiler stage further
#   semadump     type checking -- the collector runs, so the stack bounds
#                from GetCurrentThreadStackLimits are load bearing
#   layoutdump   layout and calling convention
#   firnc1       THE COMPILER ITSELF as a Windows program: it reads the
#                source, resolves `import` over $FIRNLIB, lexes, parses,
#                checks, lowers, generates code and writes the assembly to
#                standard output. If that text is character identical with
#                the one the Linux build produces, the Firn compiler runs
#                on Windows.
#
# PATHS: everything is relative to the repository root, and $FIRNLIB is set
# to the RELATIVE `lib` on both sides. Absolute Linux paths do not survive
# the crossing (docs/ROUND-WINDOWS.md 2.3), and using Wine's `Z:` drive to
# paper over that would measure Wine and not the seam.
#
# Usage:  tools/windows/programs.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-work/programs"

export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi

if [ ! -x "$FIRNC" ]; then
    echo "SKIP: firnc is missing ($FIRNC) -- cargo build --release"
    exit 0
fi
if ! command -v x86_64-w64-mingw32-as >/dev/null 2>&1; then
    echo "SKIP: the mingw binutils are missing (apt-get install binutils-mingw-w64-x86-64)"
    exit 0
fi
if [ -z "$WINE" ]; then
    echo "SKIP: wine is missing (apt-get install wine64)"
    exit 0
fi

rm -rf "$WORK"; mkdir -p "$WORK"
PASS=0; FAIL=0

# name | source | arguments | file on standard input
CASES=(
  "sysdump|bin/sysdump.fi||"
  "lexdump|bin/lexdump.fi||examples/hello.fi"
  "astdump|bin/astdump.fi|examples/hello.fi|"
  "firdump|bin/firdump.fi|examples/hello.fi|"
  "semadump|bin/semadump.fi|examples/hello.fi|"
  "layoutdump|bin/layoutdump.fi|examples/hello.fi|"
  "firnc1|bin/firnc1.fi|examples/hello.fi|"
  "firnc1-structs|bin/firnc1.fi|examples/structs.fi|"
  "firnc1-bubblesort|bin/firnc1.fi|examples/bubblesort.fi|"
  "hello|examples/hello.fi||"
  "tour|examples/tour.fi||"
  "structs|examples/structs.fi||"
  "bubblesort|examples/bubblesort.fi||"
  "fib|examples/fib.fi||"
)

printf '  %-20s %-9s %-9s %-8s %s\n' NAME LINUX WINDOWS BYTES VERDICT
for spec in "${CASES[@]}"; do
    IFS='|' read -r name src args stdin <<< "$spec"

    # --- build both sides -------------------------------------------------
    if ! FIRNLIB=lib "$FIRNC" "$src" -o "$WORK/$name.lin" > "$WORK/$name.blin" 2>&1; then
        printf '  %-20s %-9s %-9s %-8s %s\n' "$name" "-" "-" "-" "BUILD-LINUX-FAILED"
        FAIL=$((FAIL+1)); continue
    fi
    if ! FIRNLIB=lib "$FIRNC" --target=x86_64-windows "$src" -o "$WORK/$name.exe" > "$WORK/$name.bwin" 2>&1; then
        printf '  %-20s %-9s %-9s %-8s %s\n' "$name" "-" "-" "-" "BUILD-WINDOWS-FAILED"
        FAIL=$((FAIL+1)); continue
    fi

    # --- run both sides ---------------------------------------------------
    if [ -n "$stdin" ]; then
        FIRNLIB=lib timeout 300 "$WORK/$name.lin" $args < "$stdin" > "$WORK/$name.lout" 2>"$WORK/$name.lerr"; lrc=$?
        FIRNLIB=lib timeout 600 "$WINE" "$WORK/$name.exe" $args < "$stdin" > "$WORK/$name.wout" 2>"$WORK/$name.werr"; wrc=$?
    else
        FIRNLIB=lib timeout 300 "$WORK/$name.lin" $args < /dev/null > "$WORK/$name.lout" 2>"$WORK/$name.lerr"; lrc=$?
        FIRNLIB=lib timeout 600 "$WINE" "$WORK/$name.exe" $args < /dev/null > "$WORK/$name.wout" 2>"$WORK/$name.werr"; wrc=$?
    fi

    bytes=$(wc -c < "$WORK/$name.lout")
    if [ "$lrc" -eq "$wrc" ] && cmp -s "$WORK/$name.lout" "$WORK/$name.wout"; then
        printf '  %-20s %-9s %-9s %-8s %s\n' "$name" "rc=$lrc" "rc=$wrc" "$bytes" "SAME"
        PASS=$((PASS+1))
    else
        why="output differs"
        [ "$lrc" -ne "$wrc" ] && why="exit code differs"
        printf '  %-20s %-9s %-9s %-8s %s\n' "$name" "rc=$lrc" "rc=$wrc" "$bytes" "DIFFERENT ($why)"
        FAIL=$((FAIL+1))
    fi
done

echo
echo "  RESULT: $PASS of $((PASS + FAIL)) programs behave identically on both operating systems"
[ "$FAIL" -eq 0 ] || exit 1
exit 0

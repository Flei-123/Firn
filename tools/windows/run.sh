#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/run.sh -- THE CROSS CHECK OF ROUND WINDOWS.
#
# The same Firn program is compiled TWICE and both results are RUN. What is
# compared is what the program DOES: its standard output character for
# character and its exit code.
#
#   x86-64 Linux    firnc --target=x86_64-linux     -> runs natively
#   x86-64 Windows  firnc --target=x86_64-windows   -> runs under Wine
#
# The instruction set is the SAME on both sides -- that is what makes this
# comparison sharper than the aarch64 one of round 80. Everything that
# differs here differs because of the operating system: the binary format,
# the calling convention at the boundary, and the seam that answers a
# `syscall` over Win32 (compiler/src/win_seam.rs).
#
# Buckets, and none of them is swept under the carpet:
#
#   SAME        both compiled, both ran, same output and same exit code
#   DIFF        both compiled and they do not agree (or the Windows side
#               crashed, hung, or its assembler/linker refused the text)
#   NOTSUP      the WINDOWS build was refused with a clear message
#   LINUXBAD    the Linux side does not meet its own expectation from line 1
#               -- then there is nothing to compare, and the case says so
#               instead of counting as a success
#
# Every DIFF is additionally sorted into a CAUSE by `tools/windows/causes.txt`
# (a plain list of `<pattern> <cause>` lines). The list of causes is the
# point of this tool; a bare success rate would say much less.
#
# Usage:
#   tools/windows/run.sh                 all of tests/*.fi
#   tools/windows/run.sh --no-opt        the same corpus, optimiser off
#   WIN_FILTER=340 tools/windows/run.sh  only the cases whose name matches
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-work"
JOBS=${JOBS:-6}
FLAGS=""
LABEL="dev-fast"
if [ "${1:-}" = "--no-opt" ]; then
    FLAGS="--no-opt"
    LABEL="no-opt"
    shift
fi
FILTER=${WIN_FILTER:-}

# Wine: one prefix of our own, no debug chatter.
export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi

if [ ! -x "$FIRNC" ]; then
    echo "firnc is missing: $FIRNC (cargo build --release --manifest-path compiler/Cargo.toml)"
    exit 1
fi
for t in x86_64-w64-mingw32-as x86_64-w64-mingw32-ld; do
    command -v "$t" >/dev/null 2>&1 || {
        echo "SKIP: $t is missing (apt-get install binutils-mingw-w64-x86-64)"
        exit 0
    }
done
if [ -z "$WINE" ]; then
    echo "SKIP: wine is missing (apt-get install wine64)"
    exit 0
fi
"$WINE" --version >/dev/null 2>&1 || true

rm -rf "$WORK"
mkdir -p "$WORK"

# ------------------------------------------------------------------ causes
# `DIFF ... :: <why>` gets a cause out of tools/windows/causes.txt. The file
# holds `<glob> <cause>` lines; the first match wins. Anything unmatched is
# reported as UNGROUPED, which is exactly what one wants to see.
cause_of() { # $1 = "<file> <why>" -- the file name is part of the match,
             #      because some causes are known by the case and some by
             #      the message.
    local why="$1" pat rest
    while read -r pat rest; do
        case "$pat" in ''|'#'*) continue ;; esac
        # shellcheck disable=SC2254
        case "$why" in $pat) echo "$rest"; return ;; esac
    done < tools/windows/causes.txt
    echo "UNGROUPED"
}
export -f cause_of

one_case() {
    local file="$1" base exp hdr kind
    base=$(basename "$file" .fi)
    hdr=$(head -1 "$file")
    case "$hdr" in
        *expect_out:*)  kind=out;  exp=${hdr#*expect_out: } ;;
        *expect_exit:*) kind=exit; exp=${hdr#*expect_exit: } ;;
        *) echo "LINUXBAD $file :: no expectation in line 1"; return ;;
    esac

    # --- Linux: does the case work at all, and does it meet its own
    # expectation? Only then is there something to compare.
    if ! "$FIRNC" $FLAGS --target=x86_64-linux -o "$WORK/$base.lin" "$file" \
            >"$WORK/$base.lin.err" 2>&1; then
        echo "LINUXBAD $file :: linux compilation failed"; return
    fi
    local lout lrc try
    for try in 1 2; do
        lout=$(timeout 60 "$WORK/$base.lin" </dev/null 2>/dev/null); lrc=$?
        if [ "$kind" = out ]; then
            if [ "$lrc" -eq 0 ] && [ "$lout" = "$exp" ]; then break; fi
        else
            if [ "$lrc" = "$exp" ]; then break; fi
        fi
        if [ "$try" -eq 2 ]; then
            echo "LINUXBAD $file :: linux does not meet its own expectation (twice)"
            return
        fi
        sleep 2
    done

    # --- Windows
    if ! "$FIRNC" $FLAGS --target=x86_64-windows -o "$WORK/$base.exe" "$file" \
            >"$WORK/$base.win.err" 2>&1; then
        local why
        why=$(grep -m1 -E '^error: ' "$WORK/$base.win.err" | cut -c8-200)
        [ -z "$why" ] && why=$(head -1 "$WORK/$base.win.err" | cut -c1-200)
        case "$why" in
            *"not supported on windows"*|*"windows:"*|*"no meaning on windows"*)
                echo "NOTSUP $file :: $why" ;;
            *)  echo "DIFF $file :: windows build failed: $why" ;;
        esac
        return
    fi
    local wout wrc
    wout=$(timeout 180 "$WINE" "$WORK/$base.exe" </dev/null 2>/dev/null); wrc=$?
    if [ "$lrc" != "$wrc" ] || [ "$lout" != "$wout" ]; then
        sleep 1
        wout=$(timeout 180 "$WINE" "$WORK/$base.exe" </dev/null 2>/dev/null); wrc=$?
    fi
    if [ "$lrc" != "$wrc" ]; then
        echo "DIFF $file :: exit code linux=$lrc windows=$wrc"; return
    fi
    if [ "$lout" != "$wout" ]; then
        local l1 w1
        l1=$(printf '%s' "$lout" | head -c 50 | tr '\n' '|')
        w1=$(printf '%s' "$wout" | head -c 50 | tr '\n' '|')
        echo "DIFF $file :: output linux='$l1' windows='$w1'"; return
    fi
    echo "SAME $file"
}
export -f one_case
export FIRNC WORK FLAGS ROOT FIRNLIB WINE WINEPREFIX WINEDEBUG

LIST="$WORK/list.txt"
ls tests/*.fi > "$LIST"
if [ -n "$FILTER" ]; then
    grep "$FILTER" "$LIST" > "$LIST.f" && mv "$LIST.f" "$LIST"
fi
TOTAL=$(wc -l < "$LIST")

echo "== windows cross check ($LABEL, $TOTAL cases, $JOBS at a time) =="
xargs -a "$LIST" -P "$JOBS" -I{} bash -c 'one_case "$@"' _ {} | sort > "$WORK/result.$LABEL.txt"

SAME=$(grep -c '^SAME '     "$WORK/result.$LABEL.txt" || true)
DIFF=$(grep -c '^DIFF '     "$WORK/result.$LABEL.txt" || true)
NOTSUP=$(grep -c '^NOTSUP ' "$WORK/result.$LABEL.txt" || true)
LINBAD=$(grep -c '^LINUXBAD ' "$WORK/result.$LABEL.txt" || true)

# --- the causes -----------------------------------------------------------
: > "$WORK/causes.$LABEL.txt"
while IFS= read -r line; do
    f=$(echo "$line" | awk '{print $2}')
    why=${line#*:: }
    printf '%s\t%s\t%s\n' "$(cause_of "$f $why")" "$f" "$why" >> "$WORK/causes.$LABEL.txt"
done < <(grep '^DIFF ' "$WORK/result.$LABEL.txt")

echo
if [ "$DIFF" -gt 0 ]; then
    echo "  -- what does not work, grouped by cause --"
    cut -f1 "$WORK/causes.$LABEL.txt" | sort | uniq -c | sort -rn | sed 's/^/  /'
    echo
    sort "$WORK/causes.$LABEL.txt" | cut -c1-190 | sed 's/^/    /'
    echo
fi
grep '^NOTSUP '   "$WORK/result.$LABEL.txt" | cut -c1-190 | sed 's/^/  /'
grep '^LINUXBAD ' "$WORK/result.$LABEL.txt" | cut -c1-190 | sed 's/^/  /'
echo
COMPARABLE=$((SAME + DIFF + NOTSUP))
PCT=0
[ "$COMPARABLE" -gt 0 ] && PCT=$((SAME * 100 / COMPARABLE))
echo "  build stage:    $LABEL"
echo "  SAME:           $SAME"
echo "  DIFFERENT:      $DIFF"
echo "  NOT SUPPORTED:  $NOTSUP"
echo "  linux already:  $LINBAD"
echo "  RESULT: $SAME of $COMPARABLE comparable cases identical on Linux and Windows ($PCT%)"
echo "  (the Windows side ran under Wine, not on Windows)"

# The floor this round measured; a later round may only raise it.
MIN=$(cat tools/windows/minquota.txt 2>/dev/null || echo 0)
if [ "$SAME" -lt "$MIN" ]; then
    echo "FAIL only $SAME of the required $MIN cases behave identically"
    exit 1
fi
echo "PASS $SAME cases behave identically (floor $MIN)"

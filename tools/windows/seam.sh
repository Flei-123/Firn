#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/seam.sh -- ROUND MERGE-WIN: the seam's answer table, MEASURED.
#
# `tools/windows/seam.fi` calls a list of system call numbers without a side
# effect and prints `<number> <return value>` for each. The program is built
# for both operating systems and run on both; this script puts the two
# columns next to each other and says, per number, whether the seam carries
# it (BOUND) or answers -38 / ENOSYS (MISSING).
#
# Why this is worth its own tool: the list of what is bound otherwise only
# exists as source code in `compiler/src/win_seam.rs`. A table read out of
# the source says what someone MEANT to bind. This one says what the seam
# really answers.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-work/seam"

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
FIRNLIB=lib "$FIRNC" tools/windows/seam.fi -o "$WORK/seam.lin" >/dev/null 2>&1 || { echo "FAIL: linux build"; exit 1; }
FIRNLIB=lib "$FIRNC" --target=x86_64-windows tools/windows/seam.fi -o "$WORK/seam.exe" >/dev/null 2>&1 || { echo "FAIL: windows build"; exit 1; }

"$WORK/seam.lin" > "$WORK/lin.txt" 2>/dev/null; lrc=$?
timeout 300 "$WINE" "$WORK/seam.exe" > "$WORK/win.txt" 2>/dev/null; wrc=$?
if [ "$lrc" -ne 0 ] || [ "$wrc" -ne 0 ]; then
    echo "FAIL: the probe did not finish (linux rc=$lrc, windows rc=$wrc)"
    exit 1
fi

# The names, so the table reads without a syscall list next to it.
name_of() {
    case "$1" in
        0) echo read;; 1) echo write;; 2) echo open;; 3) echo close;;
        4) echo stat;; 5) echo fstat;; 7) echo poll;; 8) echo lseek;;
        9) echo mmap;; 10) echo mprotect;; 11) echo munmap;; 16) echo ioctl;;
        21) echo access;; 24) echo sched_yield;; 32) echo dup;; 35) echo nanosleep;;
        39) echo getpid;; 41) echo socket;; 48) echo shutdown;; 49) echo bind;;
        50) echo listen;; 51) echo getsockname;; 54) echo setsockopt;;
        61) echo wait4;; 72) echo fcntl;; 74) echo fsync;; 77) echo ftruncate;;
        79) echo getcwd;; 89) echo readlink;; 186) echo gettid;;
        202) echo futex;; 217) echo getdents64;; 228) echo clock_gettime;;
        257) echo openat;; 288) echo accept4;; 318) echo getrandom;;
        *) echo "?";;
    esac
}

bound=0; missing=0
printf '  %-4s %-14s %-10s %-10s %s\n' NR NAME LINUX WINDOWS STATE
while read -r nr lv; do
    wv=$(awk -v n="$nr" '$1==n {print $2; exit}' "$WORK/win.txt")
    [ -z "$wv" ] && wv="(none)"
    if [ "$wv" = "-38" ]; then
        st="MISSING (ENOSYS)"; missing=$((missing+1))
    elif [ "$lv" = "$wv" ]; then
        st="BOUND"; bound=$((bound+1))
    else
        st="BOUND, other value"; bound=$((bound+1))
    fi
    printf '  %-4s %-14s %-10s %-10s %s\n' "$nr" "$(name_of "$nr")" "$lv" "$wv" "$st"
done < "$WORK/lin.txt"

echo
echo "  BOUND    $bound"
echo "  MISSING  $missing   (they answer -38 = ENOSYS, visibly wrong instead of quietly wrong)"
exit 0

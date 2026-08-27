#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/repro/two_machines.sh -- ACCEPTANCE ITEM 5, the whole sentence:
# "package management builds reproducibly on two machines".
#
# WHAT THIS MEASURES, and what it does not
# ---------------------------------------------------------------------------
# `tools/repro/run.sh` (round 48) unpacks the same commit into two
# directories and compares the artifacts. That catches the coarsest kind of
# path dependence and nothing else: same environment, same clock, same
# process layout, one directory name apart.
#
# A second MACHINE differs in more than its directory name. This script
# builds the same package TWICE and makes the second run differ in every
# way that costs nothing to change and that a real second machine would
# differ in as well:
#
#   * another working directory, deeper and with a longer name
#   * another $HOME, $TMPDIR, $USER, $LOGNAME, $SHELL, $PWD, another $PATH
#   * another language ($LANG, $LC_ALL) and another time zone ($TZ)
#   * another umask (077 against 022)
#   * another wall clock -- the two runs lie seconds apart, and the file
#     time stamps of the sources are set years apart
#   * the sources CREATED IN THE OPPOSITE ORDER, so the directory order the
#     file system hands out is not the same one
#   * THE COMPILER AT ANOTHER PATH -- `firnc1` reads `/proc/self/exe` for
#     its library search, so the place the binary sits at is an input
#   * and, if `qemu-x86_64` is installed, machine B does not even run on
#     the same CPU implementation: the second run goes through the
#     emulator, with other CPU features and another address layout.
#
# What stays equal is the SOURCE STATE and the compiler binary's content.
# That is the claim: same sources -> same octets.
#
# BOTH compilers are measured, because a package system that were
# reproducible in only one of the two would be none: `firnc0` (Rust) and
# `firnc1` (Firn, self hosted).
#
# The lock file is measured with them: all four runs have to write the same
# `firn.lock`, otherwise "reproducible" would only mean the artifact and
# not the input.
#
# Usage:  bash tools/repro/two_machines.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC (cargo build --release --manifest-path compiler/Cargo.toml)"
    exit 1
fi
# The lesson of rounds 35/45/46: never measure a compiler that no longer
# exists. `.firnc1` is rebuilt as soon as `firnc0` or one of its sources is
# younger.
rebuild=0
[ -x "$FC1" ] || rebuild=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && rebuild=1
    while IFS= read -r f; do
        [ "$f" -nt "$FC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc1 cannot be built"; exit 1; }
fi

BASE=$(mktemp -d "${TMPDIR:-/tmp}/firn-two-machines.XXXXXXXX")
trap 'rm -rf "$BASE"' EXIT

A="$BASE/a"
B="$BASE/a-machine-with-a-considerably-longer-name/deeper/still/b"
mkdir -p "$A" "$B" "$BASE/home-a" "$BASE/a-much-longer-home-directory-of-machine-b" \
         "$BASE/tmp-a" "$BASE/tmp-b" "$B/tools"

# --- the sources, twice ---------------------------------------------------
# Machine A: plain copy. Machine B: every file written in the OPPOSITE
# order and with a time stamp years away, so that neither the directory
# order nor a time stamp can be what makes the two agree.
cp -r demos/packages "$A/packages"
mkdir -p "$B/packages"
while IFS= read -r rel; do
    mkdir -p "$B/packages/$(dirname "$rel")"
    cp "demos/packages/$rel" "$B/packages/$rel"
    touch -d '2001-09-11 08:46:00' "$B/packages/$rel"
done < <(cd demos/packages && find . -type f | sed 's|^\./||' | LC_ALL=C sort -r)
# A lock file out of the repository must not take part in the comparison:
# every run writes its own.
rm -f "$A/packages/app/firn.lock" "$B/packages/app/firn.lock"

sa=$(cd "$A/packages" && find . -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
sb=$(cd "$B/packages" && find . -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
echo "== ACCEPTANCE item 5: the same package on two machines =="
echo "   sources A: $sa"
echo "   sources B: $sb"
[ "$sa" = "$sb" ] || { echo "FAILED: the two source trees differ already."; exit 1; }

# --- the compilers, machine B gets them at another path -------------------
cp "$FIRNC" "$B/tools/firnc-somewhere-else"
cp "$FC1" "$B/tools/firnc1-somewhere-else"
cp -r lib "$B/lib"

QEMU=""
command -v qemu-x86_64 > /dev/null 2>&1 && QEMU=qemu-x86_64
# A language that really exists on this machine -- a $LC_ALL that the C
# library does not know is not a difference, it is a warning.
LOC=$(locale -a 2>/dev/null | grep -v -E '^(C|POSIX)$' | grep -i -m1 'utf' || true)
[ -z "$LOC" ] && LOC=C

# --- build ---------------------------------------------------------------
# Machine A: this environment, ASLR switched off where possible.
build_a() {
    ( cd "$A" || exit 1
      umask 022
      export HOME="$BASE/home-a" TMPDIR="$BASE/tmp-a" TZ=UTC LANG=C LC_ALL=C
      export USER=firn-a LOGNAME=firn-a SHELL=/bin/sh
      export FIRNLIB="$ROOT/lib"
      setarch "$(uname -m)" -R "$FIRNC" --package packages/app -o app_fc0 --lock --keep-asm \
          > "$BASE/a.fc0.log" 2>&1 || { echo "   firnc0 failed on A"; tail -3 "$BASE/a.fc0.log"; exit 1; }
      cp packages/app/firn.lock lock_fc0
      rm -f packages/app/firn.lock
      setarch "$(uname -m)" -R "$FC1" --package packages/app -o app_fc1 --lock \
          > "$BASE/a.fc1.log" 2>&1 || { echo "   firnc1 failed on A"; tail -3 "$BASE/a.fc1.log"; exit 1; }
      cp packages/app/firn.lock lock_fc1
    )
}
# Machine B: another directory, another environment, another clock, the
# compiler at another path -- and through the emulator if there is one.
build_b() {
    ( cd "$B" || exit 1
      umask 077
      export HOME="$BASE/a-much-longer-home-directory-of-machine-b"
      export TMPDIR="$BASE/tmp-b" TZ=Pacific/Kiritimati LANG="$LOC" LC_ALL="$LOC"
      export USER=someone-else LOGNAME=someone-else SHELL=/bin/bash
      export PATH="/usr/local/bin:$PATH"
      export FIRNLIB="$B/lib"
      $QEMU ./tools/firnc-somewhere-else --package packages/app -o app_fc0 --lock --keep-asm \
          > "$BASE/b.fc0.log" 2>&1 || { echo "   firnc0 failed on B"; tail -3 "$BASE/b.fc0.log"; exit 1; }
      cp packages/app/firn.lock lock_fc0
      rm -f packages/app/firn.lock
      $QEMU ./tools/firnc1-somewhere-else --package packages/app -o app_fc1 --lock \
          > "$BASE/b.fc1.log" 2>&1 || { echo "   firnc1 failed on B"; tail -3 "$BASE/b.fc1.log"; exit 1; }
      cp packages/app/firn.lock lock_fc1
    )
}

echo
echo "-- machine A: $A"
echo "   environment: HOME=$BASE/home-a TZ=UTC LANG=C umask 022, ASLR off"
build_a || exit 1
# Seconds between the two runs -- a build that puts a time stamp into its
# result fails right here.
sleep 2
echo "-- machine B: $B"
if [ -n "$QEMU" ]; then
    echo "   environment: another \$HOME/\$TZ/\$PATH, \$LC_ALL=$LOC, umask 077,"
    echo "                compiler at another path, time stamps of 2001, under qemu-x86_64"
else
    echo "   environment: another \$HOME/\$TZ/\$PATH, \$LC_ALL=$LOC, umask 077,"
    echo "                compiler at another path, time stamps of 2001 (no qemu-x86_64)"
fi
build_b || exit 1

# --- compare -------------------------------------------------------------
echo
echo "-- the octets --"
bad=0
compare() {   # $1 = file, $2 = label
    local ha hb
    ha=$(sha256sum "$A/$1" 2>/dev/null | cut -d' ' -f1)
    hb=$(sha256sum "$B/$1" 2>/dev/null | cut -d' ' -f1)
    if [ -z "$ha" ] || [ -z "$hb" ]; then
        printf '   %-22s MISSING\n' "$2"
        bad=$((bad + 1))
        return
    fi
    if [ "$ha" = "$hb" ]; then
        printf '   %-22s IDENTICAL  %s\n' "$2" "$ha"
    else
        printf '   %-22s DIFFERENT\n' "$2"
        printf '        A %s\n        B %s\n' "$ha" "$hb"
        if command -v cmp > /dev/null; then
            printf '        %s octet(s) of %s differ\n' \
                "$(cmp -l "$A/$1" "$B/$1" 2>/dev/null | wc -l)" \
                "$(stat -c%s "$A/$1")"
        fi
        bad=$((bad + 1))
    fi
}
compare app_fc0     "binary (firnc0)"
compare app_fc0.s   "assembly (firnc0)"
compare app_fc1     "binary (firnc1)"
compare lock_fc0    "firn.lock (firnc0)"
compare lock_fc1    "firn.lock (firnc1)"

# The two compilers do not have to produce the same BINARY -- `firnc0`
# writes `.file`/`.loc` directives, `firnc1` does not. They do have to
# produce the same LOCK FILE: that is a statement about the input, and the
# input is the same.
if ! cmp -s "$A/lock_fc0" "$A/lock_fc1"; then
    echo "   firn.lock              DIFFERENT between the two compilers"
    diff "$A/lock_fc0" "$A/lock_fc1" | head -6 | sed 's/^/        /'
    bad=$((bad + 1))
else
    echo "   firn.lock              the same out of BOTH compilers"
fi
# And the programs have to do the same thing.
oa=$("$A/app_fc0"); ra=$?
ob=$("$B/app_fc0"); rb=$?
oc=$("$A/app_fc1"); rc=$?
if [ "$oa" != "12 14 3" ] || [ "$ob" != "12 14 3" ] || [ "$oc" != "12 14 3" ] \
   || [ "$ra" -ne 0 ] || [ "$rb" -ne 0 ] || [ "$rc" -ne 0 ]; then
    echo "   the program does not print what it should: '$oa'/'$ob'/'$oc' ($ra/$rb/$rc)"
    bad=$((bad + 1))
else
    echo "   the program prints '12 14 3' on both machines, out of both compilers"
fi

echo
if [ "$bad" -ne 0 ]; then
    echo "FAILED: $bad difference(s) between the two machines."
    exit 1
fi
echo "PASS: the same source state gives bit identical artifacts on both machines,"
echo "      out of both compilers, and the same firn.lock in all four runs."
exit 0

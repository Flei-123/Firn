#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/repro/run.sh -- ACCEPTANCE ITEM 5, the part that CAN be shown today:
# from the same source state, the same artifact, octet for octet.
#
# The criterion of item 5 is "two different machines produce a bit-identical
# artifact from the same source state". This script measures the CORE of it,
# and the core alone (round 93 added the rest: the lock file with checksums
# in `--lock`/`--locked`, and the second machine in
# `tools/repro/two_machines.sh`):
#
#   the same commit, unpacked into TWO working directories with different
#   paths, built from scratch in both -- and the results compared with
#   sha256.
#
# That is not two machines. It is the half of the promise that can be checked
# here, and the half a build fails at first if it is not deterministic:
# absolute paths, time stamps, a hash iteration order, the source path in the
# debug information.
#
# Built in each directory:
#   1. the stage 0 compiler out of Rust (`cargo build --release`)
#   2. `bin/firnc1.fi` -> the compiler in Firn, with firnc0        (stage 1)
#   3. the same source again, with the compiler in Firn            (stage 2)
#   4. the demo PACKAGE `demos/packages/app` over the build driver
#      `--package`, because that is what item 5 is actually about
#   5. every `.s` next to it -- the assembly text is where a difference shows
#      up first and is readable
#
# Usage:  bash tools/repro/run.sh [commit]      (default: HEAD)
set -uo pipefail
cd "$(dirname "$0")/../.."
COMMIT=${1:-HEAD}

BASE=$(mktemp -d)
trap 'rm -rf "$BASE"' EXIT
# Two paths of DIFFERENT length -- an absolute path that leaked into the
# artifact would otherwise be able to hide.
A="$BASE/a"
B="$BASE/build-directory-with-a-much-longer-name/b"
mkdir -p "$A" "$B"

echo "== ACCEPTANCE item 5 (core): the same source, two working directories =="
echo "   commit:      $(git rev-parse --short "$COMMIT")"
echo "   directory A: $A"
echo "   directory B: $B"

echo
echo "-- 0. unpack --"
git archive "$COMMIT" | tar -x -C "$A" || exit 1
git archive "$COMMIT" | tar -x -C "$B" || exit 1
sa=$(cd "$A" && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
sb=$(cd "$B" && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
echo "   tree A: $sa"
echo "   tree B: $sb"
[ "$sa" = "$sb" ] || { echo "FAILED: the two working directories differ already."; exit 1; }

build() {
    local d=$1 tag=$2
    ( cd "$d" || exit 1
      export FIRNLIB="$d/lib"
      cargo build --release --manifest-path compiler/Cargo.toml > "$BASE/cargo.$tag.log" 2>&1 \
          || { echo "   cargo failed in $tag"; tail -5 "$BASE/cargo.$tag.log"; exit 1; }
      FC=compiler/target/release/firnc
      "$FC" bin/firnc1.fi -o ./stage1 > "$BASE/s1.$tag.log" 2>&1 \
          || { echo "   stage 1 failed in $tag"; tail -3 "$BASE/s1.$tag.log"; exit 1; }
      ./stage1 bin/firnc1.fi -o ./stage2 > "$BASE/s2.$tag.log" 2>&1 \
          || { echo "   stage 2 failed in $tag"; tail -3 "$BASE/s2.$tag.log"; exit 1; }
      "$FC" --package demos/packages/app -o ./package_bin > "$BASE/pk.$tag.log" 2>&1 \
          || { echo "   the package build failed in $tag"; tail -3 "$BASE/pk.$tag.log"; exit 1; }
    ) || return 1
}

echo
echo "-- 1. build in both directories (cargo + three Firn builds each) --"
t0=$(date +%s)
build "$A" a || exit 1
build "$B" b || exit 1
echo "   $(( $(date +%s) - t0 ))s wall clock for both"

echo
echo "-- 2. compare --"
same=0
different=0
missing=0
# `firnc0` deletes its `.s` after linking, the compiler in Firn keeps it --
# hence only stage2.s is in the list.
for f in stage1 stage2 stage2.s package_bin; do
    if [ ! -f "$A/$f" ] || [ ! -f "$B/$f" ]; then
        printf '   %-14s MISSING\n' "$f"
        missing=$((missing + 1))
        continue
    fi
    ha=$(sha256sum "$A/$f" | cut -d' ' -f1)
    hb=$(sha256sum "$B/$f" | cut -d' ' -f1)
    if [ "$ha" = "$hb" ]; then
        printf '   %-14s IDENTICAL  %s  (%s octets)\n' "$f" "${ha:0:16}" "$(stat -c%s "$A/$f")"
        same=$((same + 1))
    else
        printf '   %-14s DIFFERENT\n' "$f"
        printf '        A %s\n        B %s\n' "$ha" "$hb"
        if [ "${f##*.}" = "s" ]; then
            echo "        first difference:"
            diff "$A/$f" "$B/$f" | head -6 | sed 's/^/          /'
        fi
        different=$((different + 1))
    fi
done

# --- the diagnosis, so that "DIFFERENT" is not the end of the sentence ------
if [ "$different" -ne 0 ] && command -v readelf > /dev/null; then
    echo
    echo "-- 3. where the difference sits --"
    for f in stage1 package_bin; do
        [ -f "$A/$f" ] && [ -f "$B/$f" ] || continue
        ca=$(readelf --debug-dump=info "$A/$f" 2>/dev/null | grep -m1 'DW_AT_comp_dir' | sed 's/.*: *//')
        cb=$(readelf --debug-dump=info "$B/$f" 2>/dev/null | grep -m1 'DW_AT_comp_dir' | sed 's/.*: *//')
        if [ -n "$ca$cb" ] && [ "$ca" != "$cb" ]; then
            printf '   %-14s DW_AT_comp_dir  A: %s\n' "$f" "$ca"
            printf '   %-14s DW_AT_comp_dir  B: %s\n' "" "$cb"
        fi
        # How many octets really differ?
        n=$(cmp -l "$A/$f" "$B/$f" 2>/dev/null | wc -l)
        t=$(stat -c%s "$A/$f")
        printf '   %-14s %s of %s octets differ\n' "" "$n" "$t"
    done
    echo "   ROUND 93 found three ways for the working directory to get into"
    echo "   the artifact: DW_AT_comp_dir written by 'as' out of the .file/.loc"
    echo "   directives (fixed with --debug-prefix-map, main.rs::assemble), the"
    echo "   absolute module paths of the package search in .debug_line, and the"
    echo "   same paths in the panic message table in .rodata (both fixed with"
    echo "   package_world::build_path). If this line appears again, a FOURTH way"
    echo "   was found -- look at .debug_str and at the strings of the binary."
fi

# The compiler out of Rust is checked too -- but its result is NOT part of
# the claim: `rustc` writes the build path into the binary, and that is
# rustc's business, not Firn's.
ra=$(sha256sum "$A/compiler/target/release/firnc" | cut -d' ' -f1)
rb=$(sha256sum "$B/compiler/target/release/firnc" | cut -d' ' -f1)
echo
if [ "$ra" = "$rb" ]; then
    echo "   for information: firnc0 (out of Rust) is identical as well"
else
    echo "   for information: firnc0 (out of Rust) DIFFERS -- rustc writes the"
    echo "                    build path into the binary. Not a statement about Firn."
fi

echo
echo "   identical: $same   different: $different   missing: $missing"
if [ "$different" -ne 0 ] || [ "$missing" -ne 0 ]; then
    echo "FAILED: the same source does not produce the same artifact in two directories."
    exit 1
fi
echo "OK: $same artifacts identical octet for octet, out of two working directories."
echo "    That is the CORE of item 5. The whole of item 5 is measured by"
echo "    tools/repro/two_machines.sh (round 93); what is still missing there"
echo "    is a registry, and nothing else."
exit 0

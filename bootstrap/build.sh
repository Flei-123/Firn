#!/usr/bin/env sh
# bootstrap/build.sh -- FROM NOTHING TO A FIRN COMPILER, WITHOUT RUST.
#
# What this script needs, and nothing else:
#   * a POSIX shell
#   * gzip (or zcat)
#   * GNU as and GNU ld  (binutils; the same two programs the compiler
#     itself calls afterwards)
#   * sha256sum, if the checksums are to be verified
#
# What it does NOT need: `cargo`, `rustc`, a C compiler, `make`, python.
#
# Three steps:
#
#   1. SEED     bootstrap/firnc1-seed.s.gz is unpacked and assembled. The
#               result is a working Firn compiler -- `.seed/firnc-seed`.
#               The seed is the ASSEMBLY TEXT of `bin/firnc1.fi`, produced
#               by the fixpoint: it is what stage 2 and stage 3 agree on,
#               so it is what a Firn compiler produces from the Firn
#               compiler. It is readable text, and `as`/`ld` are the only
#               things that have to be trusted for it.
#
#   2. STAGE 1  the seed compiles bin/firnc1.fi FROM SOURCE -> .seed/firnc1
#
#   3. STAGE 2  .seed/firnc1 compiles bin/firnc1.fi again -> .seed/firnc2
#               and `.seed/firnc1` and `.seed/firnc2` have to be
#               BYTE IDENTICAL. If they are, the compiler built from the
#               seed is the same compiler as the one the seed contains, and
#               the seed can be thrown away without losing anything.
#
# Usage:  sh bootstrap/build.sh [TARGET]      (default: ./firnc1)
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
target=${1:-./firnc1}
work=.seed
mkdir -p "$work"

say() { printf '%s\n' "$*"; }

# ---------------------------------------------------------------- step 0
for prog in as ld; do
    command -v "$prog" >/dev/null 2>&1 || {
        say "missing: $prog (binutils)"; exit 1; }
done
if command -v gzip >/dev/null 2>&1; then
    UNZIP="gzip -dc"
elif command -v zcat >/dev/null 2>&1; then
    UNZIP="zcat"
else
    say "missing: gzip"; exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    say "-- checksums"
    ( cd bootstrap && sha256sum -c SHA256SUMS ) || {
        say "the seed does not match SHA256SUMS"; exit 1; }
fi

# ---------------------------------------------------------------- step 1
say "-- step 1: assembling the seed"
$UNZIP bootstrap/firnc1-seed.s.gz > "$work/seed.s"
as --64 -o "$work/seed.o" "$work/seed.s"
ld -o "$work/firnc-seed" "$work/seed.o"
rm -f "$work/seed.s" "$work/seed.o"
say "   $work/firnc-seed built"

# ---------------------------------------------------------------- step 2
FIRNLIB="$here/lib"
export FIRNLIB
say "-- step 2: the seed compiles bin/firnc1.fi"
"$work/firnc-seed" bin/firnc1.fi -o "$work/firnc1"

# ---------------------------------------------------------------- step 3
say "-- step 3: the result compiles itself"
"$work/firnc1" bin/firnc1.fi -o "$work/firnc2"

if cmp -s "$work/firnc1" "$work/firnc2"; then
    say "   FIXPOINT: the compiler out of the seed and the one it builds are identical"
else
    say "   NO FIXPOINT: $work/firnc1 and $work/firnc2 differ"
    exit 1
fi

cp "$work/firnc2" "$target"
rm -f "$work/firnc1.s" "$work/firnc1.o" "$work/firnc2.s" "$work/firnc2.o"
rm -f "$work/firnc1" "$work/firnc2" "$work/firnc-seed"
say
say "done: $target"
say "use it with   FIRNLIB=$here/lib $target <file>.fi -o <program>"

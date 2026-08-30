#!/usr/bin/env bash
# tools/freeze_bootstrap.sh -- writes bootstrap/firnc1-seed.s.gz.
#
# The seed of `bootstrap/` is the assembly text of `bin/firnc1.fi` AS A
# FIRN COMPILER PRODUCES IT. It may only be renewed from a run that really
# reached the fixpoint, otherwise the archive would carry a compiler that
# nobody has checked against itself.
#
# Therefore, in this order:
#   1. tools/fixpoint.sh has to end in FIXPOINT (it is run here)
#   2. .firnc2.s is the seed -- stage 2, and stage 3 is identical to it
#   3. checksums over the seed AND over the source it was made from
#
# Usage:  bash tools/freeze_bootstrap.sh [--skip-fixpoint]
set -uo pipefail
cd "$(dirname "$0")/.."

if [ "${1:-}" != "--skip-fixpoint" ]; then
    echo "-- tools/fixpoint.sh"
    if ! bash tools/fixpoint.sh; then
        echo "no fixpoint -- the seed is NOT renewed"
        exit 1
    fi
fi

[ -f .firnc2.s ] || { echo ".firnc2.s is missing -- run tools/fixpoint.sh"; exit 1; }
if [ -f .firnc3.s ] && ! cmp -s .firnc2.s .firnc3.s; then
    echo "stage 2 and stage 3 differ -- the seed is NOT renewed"
    exit 1
fi

mkdir -p bootstrap
gzip -9 -c .firnc2.s > bootstrap/firnc1-seed.s.gz

{
    echo "# The seed and everything it was made from (round BOOTSTRAP)."
    echo "# Checked by bootstrap/build.sh, renewed by tools/freeze_bootstrap.sh."
    ( cd bootstrap && sha256sum firnc1-seed.s.gz )
} > bootstrap/SHA256SUMS

{
    echo
    echo "# The source of the seed -- not checked by build.sh (the paths lie"
    echo "# outside bootstrap/), but recorded, so that it is possible to say"
    echo "# afterwards WHICH source text the seed belongs to."
    sha256sum bin/firnc1.fi lib/firnc1/*.fi | sed 's/^/# /'
} >> bootstrap/SHA256SUMS

echo
echo "seed:       bootstrap/firnc1-seed.s.gz  ($(wc -c < bootstrap/firnc1-seed.s.gz) octets, unpacked $(wc -c < .firnc2.s))"
echo "checksums:  bootstrap/SHA256SUMS"

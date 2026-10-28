#!/bin/bash
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
PYTHONPATH=/tmp/pa_x python3 "$HERE/cross_gen.py" "$D/fx" || exit 1
$FIRNC -o "$D/pqdump" "$HERE/pqdump.fi" || exit 1
fail=0; total=0
for f in "$D"/fx/*.parquet; do
    for col in text conversations; do
        total=$((total+1))
        if ! "$D/pqdump" "$f" $col "$D/out.bin" || ! cmp -s "$D/out.bin" "$D/fx/$col.exp"; then
            echo "FAIL $(basename $f) $col"; fail=$((fail+1))
        fi
    done
done
echo "parquet cross: $((total-fail))/$total ok"
exit $fail

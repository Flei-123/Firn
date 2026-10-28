#!/bin/sh
set -e
H="$(cd "$(dirname "$0")" && pwd)"
export FIRNLIB="$H/../../lib"
FIRNC="${FIRNC:-firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
"$FIRNC" "$H/live_range.fi" -o "$D/range"
"$FIRNC" "$H/live_list.fi" -o "$D/list"
GOT=$("$D/range" | tail -1 | cut -d' ' -f1)
curl -sL -r 123456789-124505364 'https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu/resolve/main/sample/10BT/000_00000.parquet' -o "$D/ref.bin"
WANT=$(python3 -I -c "
import sys
h=1469598103934665603
for b in open(sys.argv[1],'rb').read()[:1048576]: h=((h^b)*1099511628211)&(2**64-1)
print(h)" "$D/ref.bin")
[ "$GOT" = "$WANT" ] || { echo "range mismatch $GOT $WANT"; exit 1; }
"$D/list" > "$D/list.out"
echo "hf live ok"

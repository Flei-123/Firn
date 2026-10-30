#!/usr/bin/env bash
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
GOLD=$ROOT/tools/fui/golden/ds_tpl
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
. "$HERE/tpl_args.sh"
export FIRNLIB=$ROOT/lib
cd "$ROOT"
for app in $(awk "{print \$7}" "$HERE/cases.txt" | sort -u); do
    "$FIRNC" --opt-level=dev -o "$D/$app" "examples/fui_apps/$app/main.fi" > "$D/$app.log" 2>&1 || { cat "$D/$app.log"; echo "FAIL build $app"; exit 1; }
done
bad=0
while read -r name theme w h tag min app; do
    args=$(tpl_args "$name" "$theme" "$w" "$h")
    "$D/$app" $args --shot "$D/$name-$theme.png" > /dev/null 2>&1
    r=$(python3 -I "$HERE/ssim.py" "$GOLD/$name-$theme.png" "$D/$name-$theme.png" ${KEEP:+${KEEP}/diff-$name-$theme.png})
    s=$(echo "$r" | python3 -I -c 'import json,sys;print(json.load(sys.stdin)["ssim"])')
    ok=$(python3 -I -c "print(1 if $s >= $min else 0)")
    echo "ds_tpl $name $theme ssim=$s min=$min ok=$ok"
    [ -n "${KEEP:-}" ] && cp "$D/$name-$theme.png" "$KEEP/shot-$name-$theme.png"
    [ "$ok" = 1 ] || bad=$((bad + 1))
done < "$HERE/cases.txt"
[ "$bad" = 0 ] || { echo "FAIL ds_tpl $bad below gate"; exit 1; }
echo "ds_tpl ok"

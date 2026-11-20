#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
: >"$WORK/all"
if [ "$#" -eq 0 ]; then
    set -- $(cd "$ROOT/lib" && ls -d */ | tr -d /)
fi
for dir in "$@"; do
    for f in "$ROOT"/lib/$dir/*.fi; do
        base=$(basename "$f" .fi)
        case "$base" in *.*) continue;; esac
        printf 'import %s.%s\nfn main() -> i32 {\n    return 0\n}\n' "$dir" "$base" >"$WORK/m.fi"
        "$FIRNC" --unsafe=warn "$WORK/m.fi" -o "$WORK/m.out" 2>&1 | grep 'warning: ' | cut -d' ' -f1 >>"$WORK/all"
    done
done
sort -u "$WORK/all" >"$WORK/uniq"
[ -n "${AUDIT_SITES:-}" ] && cp "$WORK/uniq" "$AUDIT_SITES"
printf '%-40s %6s\n' file sites
sed -E 's/:[0-9]+:[0-9]+:$//' "$WORK/uniq" | sed "s#^$ROOT/##" | sort | uniq -c | sort -k1,1n | awk '{printf "%-40s %6d\n", $2, $1}'
printf '%-40s %6d\n' total "$(wc -l <"$WORK/uniq")"

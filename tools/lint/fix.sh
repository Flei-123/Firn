#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
LINT=${1:?lint binary}
shift
export FIRNLIB="${FIRNLIB:-$ROOT/lib}"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
module_of() {
    local m=${1#lib/}
    m=${m%.fi}
    echo "${m//\//.}"
}
compiles() {
    printf 'import %s\nfn main() -> i32 {\n    return 0\n}\n' "$1" > "$T/m.fi"
    timeout 120 "$FIRNC" -o "$T/m.bin" "$T/m.fi" > /dev/null 2>&1
}
fixed=0; skipped=0; files=0
for f in "$@"; do
    "$LINT" --rules var-never-mutated --lib lib "$f" 2>/dev/null | cut -d: -f2 | sort -un > "$T/lines.txt"
    [ -s "$T/lines.txt" ] || continue
    mod=$(module_of "$f")
    if ! compiles "$mod"; then
        skipped=$((skipped + 1))
        echo "skip (does not compile before fix): $f"
        continue
    fi
    cp "$f" "$T/orig.fi"
    expr=$(sed 's/$/s\/\\bvar\\b\/let\//' "$T/lines.txt" | tr '\n' ';')
    sed -i "$expr" "$f"
    if compiles "$mod"; then
        fixed=$((fixed + $(wc -l < "$T/lines.txt"))); files=$((files + 1))
        continue
    fi
    cp "$T/orig.fi" "$f"
    kept=0
    while read -r l; do
        cp "$f" "$T/prev.fi"
        sed -i "${l}s/\bvar\b/let/" "$f"
        if compiles "$mod"; then
            kept=$((kept + 1))
        else
            cp "$T/prev.fi" "$f"
        fi
    done < "$T/lines.txt"
    fixed=$((fixed + kept))
    [ "$kept" -gt 0 ] && files=$((files + 1))
done
echo "fixed $fixed lines in $files files, skipped $skipped files"

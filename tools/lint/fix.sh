#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
LINT=${1:?lint binary}
shift
RULE=${RULE:-var-never-mutated}
ROUNDS=${ROUNDS:-1}
export FIRNLIB="${FIRNLIB:-$ROOT/lib}"
T=$(mktemp -d)
CTX_T=$T
trap 'rm -rf "$T"' EXIT
source "$ROOT/tools/lint/ctx_compile.sh"
EDIT="$ROOT/tools/lint/edit.py"
fixed=0; skipped=0; files=0; left=0
fix_file() {
    local f=$1 n kept l
    "$LINT" --rules "$RULE" --lib lib "$f" 2>/dev/null | cut -d: -f2 | sort -un > "$T/lines.txt"
    [ -s "$T/lines.txt" ] || return 1
    if ! ctx_probe "$f"; then
        skipped=$((skipped + 1))
        echo "skip (does not compile before fix): $f"
        return 1
    fi
    cp "$f" "$T/orig.fi"
    python3 "$EDIT" "$RULE" "$f" $(cat "$T/lines.txt") > /dev/null
    if ! cmp -s "$f" "$T/orig.fi" && ctx_compiles "$f"; then
        fixed=$((fixed + $(wc -l < "$T/lines.txt")))
        files=$((files + 1))
        return 0
    fi
    cp "$T/orig.fi" "$f"
    kept=0
    for l in $(sort -rn "$T/lines.txt"); do
        cp "$f" "$T/prev.fi"
        python3 "$EDIT" "$RULE" "$f" "$l" > /dev/null
        if cmp -s "$f" "$T/prev.fi"; then
            left=$((left + 1))
        elif ctx_compiles "$f"; then
            kept=$((kept + 1))
        else
            cp "$T/prev.fi" "$f"
            echo "rejected: $f:$l"
            left=$((left + 1))
        fi
    done
    fixed=$((fixed + kept))
    ctx_compiles "$f" > /dev/null 2>&1
    [ "$kept" -gt 0 ] && files=$((files + 1))
    return 0
}
for f in "$@"; do
    r=0
    while [ "$r" -lt "$ROUNDS" ]; do
        fix_file "$f" || break
        r=$((r + 1))
    done
done
echo "fixed $fixed lines in $files files, skipped $skipped files, left $left"

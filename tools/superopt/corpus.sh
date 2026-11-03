#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
OUT="${1:?output dir missing}"
OPENPLAN="${OPENPLAN:-/root/projects/openplan}"
mkdir -p "$OUT"
export FIRNLIB="${FIRNLIB:-$ROOT/lib}"
build() {
    local tag="$1" level="$2"; shift 2
    if "$FIRNC" --no-cache --opt-level="$level" --keep-asm "$@" -o "$OUT/$tag" > "$OUT/$tag.log" 2>&1; then
        rm -f "$OUT/$tag"
    else
        echo "build failed: $tag" >&2
    fi
}
for level in release-fast release-safe; do
    for f in "$ROOT"/bench/firn/*.fi "$ROOT"/bench/firn_vs_c/firn/*.fi; do
        d=$(basename "$(dirname "$(dirname "$f")")")
        build "${level/-/_}_${d}_$(basename "$f" .fi)" "$level" "$f"
    done
done
build release_fast_firnc1 release-fast "$ROOT/bin/firnc1.fi"
if [ -d "$OPENPLAN" ]; then
    (cd "$OPENPLAN" && FIRN_USE_AS=1 build release_safe_openplan release-safe --package src/cli)
fi
ls "$OUT"/*.s | wc -l

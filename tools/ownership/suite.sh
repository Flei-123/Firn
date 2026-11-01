#!/bin/bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
OWN_MODE=${OWN_MODE:-check}
LEVELS=${LEVELS:-release-fast release-safe}
JOBS=${JOBS:-6}
ONLY=${ONLY:-}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
export FIRN_NO_CACHE=1
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
OUT=${OUT:-$D/result.tsv}

one() {
    local file=$1 level=$2 base bin hdr exp out rc st
    base=$(basename "$file" .fi)
    mkdir -p "$D/$level"
    bin="$D/$level/$base"
    hdr=$(head -1 "$file")
    case "$hdr" in
        *expect_out:*|*expect_exit:*) ;;
        *) echo -e "SKIP\t$base\t$level\tno-expectation"; return ;;
    esac
    if grep -q '^#\[test\]' "$file" && ! grep -q '^fn main' "$file"; then
        echo -e "SKIP\t$base\t$level\ttest-harness"; return
    fi
    if ! FIRN_OWN_STATS=1 FIRNC_OWNERSHIP="$OWN_MODE" timeout 300 "$FIRNC" --opt-level="$level" -o "$bin" "$file" > "$bin.cerr" 2>&1; then
        echo -e "CFAIL\t$base\t$level\t$(head -1 "$bin.cerr" | cut -c1-120)"; return
    fi
    st=$(grep -o 'tree_drops=[0-9]* fused_concats=[0-9]*' "$bin.cerr" | head -1)
    out=$(cd "$D/$level" && timeout 120 "$bin" 2> "$bin.err")
    rc=$?
    rm -f "$bin"
    if grep -q 'use after free' "$bin.err"; then
        echo -e "POISON\t$base\t$level\t$(head -1 "$bin.err" | cut -c1-120)"; return
    fi
    case "$hdr" in
        *expect_out:*)
            exp=${hdr#*expect_out: }
            if [ "$rc" = 0 ] && [ "$out" = "$exp" ]; then echo -e "OK\t$base\t$level\t$st"; else echo -e "FAIL\t$base\t$level\trc=$rc"; fi ;;
        *)
            exp=${hdr#*expect_exit: }
            if [ "$rc" = "$exp" ]; then echo -e "OK\t$base\t$level\t$st"; else echo -e "FAIL\t$base\t$level\trc=$rc want=$exp"; fi ;;
    esac
}
export -f one
export D FIRNC OWN_MODE

for f in "$ROOT"/tests/*.fi; do
    [ -n "$ONLY" ] && case "$(basename "$f")" in $ONLY) ;; *) continue ;; esac
    for l in $LEVELS; do echo "$f $l"; done
done | xargs -P "$JOBS" -n 2 bash -c 'one "$0" "$1"' > "$OUT"

for k in OK FAIL CFAIL POISON SKIP; do
    printf '%s=%s ' "$k" "$(grep -c "^$k	" "$OUT")"
done
echo
grep -E '^(FAIL|CFAIL|POISON)' "$OUT" | sort | head -60
! grep -q '^POISON' "$OUT"

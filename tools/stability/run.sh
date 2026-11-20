#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
MODES=${STABLE_MODES:-"dev-fast release-fast"}
STABLE=tests/stable
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "stability: compiler missing: $FIRNC"
    exit 1
fi

broken=0
checked=0
report() {
    echo "  BROKEN $1"
    [ -n "${2:-}" ] && printf '%s\n' "$2" | head -4 | sed 's/^/      /'
    broken=$((broken + 1))
}

echo "== frozen files =="
actual=$(cd "$STABLE" && find . -name '*.fi' | LC_ALL=C sort)
listed=$(awk '{print $2}' "$STABLE/MANIFEST.sha256" | LC_ALL=C sort)
if [ "$actual" != "$listed" ]; then
    report "file set of $STABLE differs from MANIFEST.sha256" "$(diff <(echo "$listed") <(echo "$actual") | head -4)"
fi
sumout=$(cd "$STABLE" && sha256sum -c --quiet MANIFEST.sha256 2>&1)
if [ -n "$sumout" ]; then
    report "frozen golden program changed (a golden may only change with a new edition)" "$sumout"
fi

echo "== golden programs =="
for f in "$STABLE"/*.fi; do
    base=$(basename "$f" .fi)
    hdr=$(head -1 "$f")
    for mode in $MODES; do
        checked=$((checked + 1))
        bin="$WORK/$base.$mode"
        if ! "$FIRNC" --edition=2026 --opt-level="$mode" -o "$bin" "$f" > "$WORK/cerr" 2>&1; then
            report "$base [$mode]: no longer compiles" "$(cat "$WORK/cerr")"
            continue
        fi
        case "$hdr" in
            *expect_out:*)
                want=${hdr#*expect_out: }
                { got=$(timeout 20 "$bin" 2>/dev/null); rc=$?; } 2>/dev/null
                if [ "$rc" -ne 0 ] || [ "$got" != "$want" ]; then
                    report "$base [$mode]: output '$got' (exit $rc), wanted '$want'"
                fi
                ;;
            *expect_exit:*)
                want=${hdr#*expect_exit: }
                { timeout 20 "$bin" > /dev/null 2>&1; rc=$?; } 2>/dev/null
                if [ "$rc" != "$want" ]; then
                    report "$base [$mode]: exit code $rc, wanted $want"
                fi
                ;;
        esac
        rm -f "$bin"
    done
done

echo "== level semantics =="
for f in "$STABLE"/levels/*.fi; do
    base=$(basename "$f" .fi)
    spec=$(head -1 "$f")
    spec=${spec#*expect_levels: }
    for pair in $spec; do
        mode=${pair%%=*}
        want=${pair#*=}
        flag="--opt-level=$mode"
        [ "$mode" = dev ] && flag="--no-opt"
        checked=$((checked + 1))
        bin="$WORK/$base.$mode"
        if ! "$FIRNC" --edition=2026 $flag -o "$bin" "$f" > "$WORK/cerr" 2>&1; then
            report "levels/$base [$mode]: no longer compiles" "$(cat "$WORK/cerr")"
            continue
        fi
        { timeout 20 "$bin" > /dev/null 2>&1; rc=$?; } 2>/dev/null
        if [ "$rc" != "$want" ]; then
            report "levels/$base [$mode]: exit code $rc, wanted $want"
        fi
        rm -f "$bin"
    done
done

echo "== rejected programs =="
for f in "$STABLE"/neg/*.fi; do
    base=$(basename "$f" .fi)
    checked=$((checked + 1))
    "$FIRNC" --edition=2026 --opt-level=dev-fast -o "$WORK/neg.out" "$f" > "$WORK/nerr" 2>&1
    rc=$?
    if [ "$rc" -eq 0 ]; then
        report "$base: is accepted now (the rule was loosened or removed)"
    elif [ "$rc" -ge 100 ] || grep -q panicked "$WORK/nerr"; then
        report "$base: compiler crashed instead of rejecting (exit $rc)" "$(head -3 "$WORK/nerr")"
    fi
done

echo "== std API =="
if ! bash tools/stability/api_check.sh; then
    broken=$((broken + 1))
fi

echo "stability: $checked language checks, $broken broken"
[ "$broken" -eq 0 ]

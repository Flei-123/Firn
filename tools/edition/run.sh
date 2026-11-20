#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FC1=${FIRNC1:-}
if [ -z "$FC1" ]; then
    FC1="$WORK/firnc1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1
fi

fail=0
say() { printf '  %-58s %s\n' "$1" "$2"; }
check() {
    if [ "$2" = "$3" ]; then
        say "$1" "OK"
    else
        say "$1" "DIFFERENT"
        echo "      wanted: $2"
        echo "      got   : $3"
        fail=$((fail + 1))
    fi
}

mkdir -p "$WORK/p"
printf 'fn main() -> i32 { return 7 }\n' > "$WORK/p/a.fi"
manifest() { printf 'package demo\nversion 0.1.0\n%sstart a.fi\n' "$1" > "$WORK/p/firn.package"; }
first_error() { grep -m1 '^error:' | sed "s#$WORK#W#g"; }

for pair in "firnc0:$FIRNC" "firnc1:$FC1"; do
    name=${pair%%:*}
    bin=${pair#*:}

    "$bin" "$WORK/p/a.fi" -o "$WORK/out" 2>/dev/null; "$WORK/out"
    check "$name default edition builds and runs" 7 $?

    "$bin" --edition=2026 "$WORK/p/a.fi" -o "$WORK/out" 2>/dev/null; "$WORK/out"
    check "$name --edition=2026 builds and runs" 7 $?

    got=$("$bin" --edition=1999 "$WORK/p/a.fi" -o "$WORK/out" 2>&1 | first_error)
    check "$name --edition=1999 refused" "error: unknown edition '1999' (supported: 2026)" "$got"
    "$bin" --edition=1999 "$WORK/p/a.fi" -o "$WORK/out" >/dev/null 2>&1
    check "$name --edition=1999 exit code" 2 $?

    got=$("$bin" --edition= "$WORK/p/a.fi" -o "$WORK/out" 2>&1 | first_error)
    check "$name empty edition refused" "error: unknown edition '' (supported: 2026)" "$got"

    manifest "edition 2026
"
    "$bin" --package "$WORK/p" 2>/dev/null; "$WORK/p/demo"
    check "$name manifest edition 2026 builds and runs" 7 $?
    got=$("$bin" --package-info "$WORK/p" | sed -n 3p)
    check "$name package-info reports the edition" "edition 2026" "$got"

    manifest ""
    got=$("$bin" --package-info "$WORK/p" | grep -c '^edition')
    check "$name package-info without edition has no edition line" 0 "$got"

    manifest "edition 2099
"
    got=$("$bin" --package "$WORK/p" 2>&1 | first_error)
    check "$name manifest edition 2099 refused" "error: W/p/firn.package:3: unknown edition '2099' (supported: 2026)" "$got"

    manifest "edition 2026
edition 2026
"
    got=$("$bin" --package "$WORK/p" 2>&1 | first_error)
    check "$name duplicate edition refused" "error: W/p/firn.package:4: 'edition' appears more than once in the manifest" "$got"

    manifest "edition
"
    got=$("$bin" --package "$WORK/p" 2>&1 | first_error)
    check "$name edition without value refused" "error: W/p/firn.package:3: 'edition' expects exactly one edition" "$got"

    manifest "editon 2026
"
    got=$("$bin" --package "$WORK/p" 2>&1 | first_error)
    check "$name unknown key lists edition" "error: W/p/firn.package:3: unknown key 'editon' (allowed: package, version, edition, start, source, public, needs)" "$got"
done

if [ "$fail" -ne 0 ]; then
    echo "edition: $fail checks failed"
    exit 1
fi
echo "edition: all checks passed"

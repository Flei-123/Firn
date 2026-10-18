#!/usr/bin/env bash
# tools/firn/add_check.sh -- `firn add / fetch / deps` on real projects: a local path dep and a
# git dep (a local bare repo), then the app BUILDS and RUNS with both.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRN=$ROOT/tools/firn/firn
export FIRN_ROOT=$ROOT FIRNLIB=$ROOT/lib
FIRNC=$ROOT/compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
fail=0
ck() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fail=1; fi; }
cd "$T"
"$FIRN" new app >/dev/null && "$FIRN" new --lib geo >/dev/null && "$FIRN" new --lib txt >/dev/null || { echo "FAIL new"; exit 1; }
# txt becomes a git repo (the "remote")
( cd txt && git init -q -b main && git add -A && git commit -q -m init && git tag v1 ) >/dev/null
cd app
ck "add path dep" '"$FIRN" add geo ../geo >/dev/null && grep -q "^needs    geo ../geo 0.1.0" firn.package'
ck "add git dep" '"$FIRN" add txt "file://$T/txt" v1 >/dev/null && [ -f .firn/deps/txt/firn.package ] && grep -q "^needs    txt .firn/deps/txt 0.1.0" firn.package'
ck "firn.deps written" 'grep -q "^txt file://.*/txt v1$" firn.deps && grep -qx "/.firn/" .gitignore'
ck "duplicate refused" '! "$FIRN" add geo ../geo >/dev/null 2>&1'
ck "non-package refused" '! "$FIRN" add nope /tmp >/dev/null 2>&1'
ck "deps lists both" '[ "$("$FIRN" deps | grep -c "geo\|txt")" -ge 3 ]'
rm -rf .firn/deps/txt
ck "fetch restores" '"$FIRN" fetch >/dev/null && [ -f .firn/deps/txt/firn.package ]'
printf 'import std.io\nimport geo\nimport txt\n\nfn main() -> i32 {\n    io.fmt_print_line(f"{geo.add(2, 3)} {txt.add(4, 5)}")\n    return 0\n}\n' > src/main.fi
ck "build + run with both deps" '"$FIRN" build . -o ./appbin >/dev/null 2>&1 && [ "$(./appbin)" = "5 9" ]'
[ $fail = 0 ] && echo "firn add check: ok"
exit $fail

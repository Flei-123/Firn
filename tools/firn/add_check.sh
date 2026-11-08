#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRN=$ROOT/tools/firn/firn
export FIRN_ROOT=$ROOT FIRNLIB=$ROOT/lib
FIRNC=$ROOT/compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
export FIRN_PKG_CACHE=$T/cache
unset FIRN_OFFLINE
fail=0
ck() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fail=1; fi; }
ensure_identity() { git config user.name > /dev/null || git config user.name firn-test; git config user.email > /dev/null || git config user.email firn-test@localhost; }
commit_repo() { ( cd "$1" && git init -q -b main && ensure_identity && git add -A && git commit -q -m "$2" && git tag "$3" ) > /dev/null; }

cd "$T"
"$FIRN" new app > /dev/null && "$FIRN" new --lib geo > /dev/null && "$FIRN" new --lib txt > /dev/null && "$FIRN" new --lib util > /dev/null || { echo "FAIL new"; exit 1; }
commit_repo txt init v1
( cd txt && printf 'fn add(a: i32, b: i32) -> i32 {\n    return a + b + 100\n}\n' > src/txt.fi && git commit -qam second && git tag v2 ) > /dev/null
( cd util && "$FIRN" add txt "file://$T/txt" v1 > /dev/null && printf 'import txt\n\nfn twice(a: i32) -> i32 {\n    return txt.add(a, a)\n}\n' > src/util.fi ) && commit_repo util init v1

cd app
ck "add path dep" '"$FIRN" add geo ../geo >/dev/null && grep -q "^needs    geo ../geo 0.1.0" firn.package'
ck "add git dep pins commit and tree hash" '"$FIRN" add util "file://$T/util" v1 >/dev/null && grep -Eq "^util file://.*/util v1 [0-9a-f]{40} [0-9a-f]{64}$" firn.deps'
ck "git dep needs line and gitignore" 'grep -q "^needs    util .firn/deps/util 0.1.0" firn.package && grep -qx "/.firn/" .gitignore'
ck "transitive git dep is fetched by add" '[ -f .firn/deps/util/.firn/deps/txt/firn.package ]'
ck "duplicate refused" '! "$FIRN" add geo ../geo >/dev/null 2>&1'
ck "non-package refused" '! "$FIRN" add nope /tmp >/dev/null 2>&1'
ck "unknown ref refused" '! "$FIRN" add other "file://$T/txt" nosuchref >/dev/null 2>&1 && ! grep -q "^other " firn.deps'
ck "deps lists path and git" 'out=$("$FIRN" deps) && grep -q "^geo " <<< "$out" && grep -q "^git  util" <<< "$out"'
ck "tree shows transitive deps" 'out=$("$FIRN" tree) && grep -q "^    txt 0.1.0 (git file://.*/txt@" <<< "$out"'

printf 'import std.io\nimport geo\nimport util\n\nfn main() -> i32 {\n    io.fmt_print_line(f"{geo.add(2, 3)} {util.twice(4)}")\n    return 0\n}\n' > src/main.fi
ck "build with path + git + transitive dep" '"$FIRN" build . -o ./appbin >/dev/null 2>&1 && [ "$(./appbin)" = "5 8" ]'
ck "build writes firn.lock for all packages" 'grep -q "^package txt " firn.lock && grep -q "^package util " firn.lock && grep -q "^total " firn.lock'
cp firn.lock first.lock
ck "locked rebuild is reproducible" '"$FIRN" build . -o ./appbin2 --locked >/dev/null 2>&1 && cmp -s first.lock firn.lock && cmp -s appbin appbin2'
ck "firn test sees the dependencies" 'printf "import std.testkit\nimport util\n\n#[test]\nfn t() {\n    testkit.assert_eq[i32](util.twice(2), 4)\n}\n" > tests/dep.fi && out=$("$FIRN" test tests/dep.fi) && grep -q "passed 1, failed 0" <<< "$out"'

ck "verify accepts the checkout" '"$FIRN" verify >/dev/null'
ck "verify catches a modified dependency" 'echo "// x" >> .firn/deps/util/src/util.fi && ! "$FIRN" verify >/dev/null 2>&1'
ck "fetch repairs a modified dependency" '"$FIRN" fetch >/dev/null && "$FIRN" verify >/dev/null'

cp firn.deps good.deps
ck "fetch fails on a hash mismatch" 'sed -i "s/ [0-9a-f]\{64\}\$/ $(printf "0%.0s" $(seq 64))/" firn.deps && rm -rf .firn && ! "$FIRN" fetch >/dev/null 2>"$T/err" && grep -q "checksum mismatch for util" "$T/err"'
cp good.deps firn.deps

ck "offline rebuild after the remotes are gone" 'rm -rf .firn && mv "$T/util" "$T/util.gone" && mv "$T/txt" "$T/txt.gone" && FIRN_OFFLINE=1 "$FIRN" fetch >/dev/null && "$FIRN" build . -o ./appbin3 --locked >/dev/null 2>&1 && cmp -s appbin appbin3'
ck "offline fetch fails cleanly without cache" 'rm -rf .firn "$T/cache" && ! "$FIRN" fetch --offline >/dev/null 2>"$T/err" && grep -q "offline" "$T/err"'
mv "$T/util.gone" "$T/util"; mv "$T/txt.gone" "$T/txt"
ck "fetch --locked restores from the lock" '"$FIRN" fetch --locked >/dev/null && "$FIRN" verify >/dev/null'

ck "fetch --locked refuses an unpinned row" 'printf "txt file://$T/txt v1\n" >> firn.deps && ! "$FIRN" fetch --locked >/dev/null 2>&1; cp good.deps firn.deps'
ck "update NAME keeps pins of the others" '( cd "$T/util" && git commit -q --allow-empty -m next && git tag -f v1 >/dev/null ) && before=$(grep "^util " firn.deps) && out=$("$FIRN" update util) && grep -q "updated   util" <<< "$out" && [ "$(grep "^util " firn.deps)" != "$before" ]'
ck "update of an unknown name fails" '! "$FIRN" update nope >/dev/null 2>&1'

ck "conflicting revisions are reported" '"$FIRN" add txt "file://$T/txt" v2 >/dev/null && ! "$FIRN" fetch >/dev/null 2>"$T/err" && grep -q "conflict\|checked out twice" "$T/err"'
ck "remove drops needs, row and checkout" '"$FIRN" remove txt >/dev/null && ! grep -q "^needs    txt" firn.package && ! grep -q "^txt " firn.deps && [ ! -e .firn/deps/txt ]'
ck "remove of an unknown name fails" '! "$FIRN" remove txt >/dev/null 2>&1'
ck "build works after remove" '"$FIRN" build . -o ./appbin4 >/dev/null 2>&1 && [ "$(./appbin4)" = "5 8" ]'

cd "$ROOT/examples/pkg_demo/app"
ck "example project builds and runs" 'd=$(mktemp -d) && "$FIRN" build . -o "$d/app" >/dev/null 2>&1 && [ "$("$d/app")" = "hello, world!" ]; rm -rf "$d" firn.lock'
ck "example project tests pass" 'out=$("$FIRN" test) && grep -q "passed 1, failed 0" <<< "$out"'

[ $fail = 0 ] && echo "firn add check: ok"
exit $fail

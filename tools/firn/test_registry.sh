#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FI=$ROOT/tools/firn/firn
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-registry-test.XXXXXX")
trap 'rm -rf "$W"' EXIT
export FIRN_CACHE=$W/cache FIRN_PKG_CACHE=$W/pkgcache FIRN_REGISTRY=$W/registry.index
FAILS=0
check() { local name=$1; shift; if "$@" > "$W/out" 2> "$W/err"; then printf '  %-56s ok\n' "$name"; else printf '  %-56s FAIL %s\n' "$name" "$(head -2 "$W/err" | tr '\n' ' ')"; FAILS=$((FAILS + 1)); fi; }

python3 -I tools/firn/pkg/test_semver.py > "$W/semver.out" 2>&1
check "semver unit checks" bash -c "grep -q ' 0 failure' $W/semver.out"

cd "$W"
git init -q --bare -b main geo.git
"$FI" new geo --lib > /dev/null
cd geo
git init -q -b main . && git remote add origin "file://$W/geo.git"
release() {
    sed -i "s/^version .*/version  $1/" firn.package
    printf 'fn add(a: i32, b: i32) -> i32 {\n    return a + b + %s\n}\n' "${2:-0}" > src/geo.fi
    git add -A && git commit -q -m "release $1" && "$FI" publish
}
for v in 1.0.0 1.2.0 1.3.0 2.0.0; do release $v > /dev/null; done
git push -q origin HEAD:main
check "republish refuses a duplicate" bash -c "! $FI publish 2> $W/e.txt && grep -q 'already published' $W/e.txt"
cd "$W"
check "registry has four releases" bash -c "[ \$(grep -c '^geo ' $W/registry.index) -eq 4 ]"
check "index sorted, one line per release" bash -c "awk 'NF == 5' $W/registry.index | wc -l | grep -qx 4"
check "search lists the package" bash -c "$FI search ge | grep -q '^geo .*2.0.0 (4 versions)'"
"$FI" new app > /dev/null
cd app
check "add geo@^1.2 picks 1.3.0" bash -c "$FI add geo@^1.2 | grep -q 'needs geo 1.3.0'"
check "needs line carries the version" bash -c "grep -q 'needs *geo .firn/deps/geo 1.3.0' firn.package"
check "firn.deps keeps the requirement" bash -c "awk '\$1 == \"geo\" && \$3 == \"@^1.2\"' firn.deps | grep -q ."
check "add twice is refused" bash -c "! $FI add geo@^1.2"
check "tree shows the dependency" bash -c "$FI tree | grep -q geo"
check "verify passes" "$FI" verify
"$FI" remove geo > /dev/null
check "add without requirement picks 2.0.0" bash -c "$FI add geo | grep -q 'needs geo 2.0.0'"
"$FI" remove geo > /dev/null
check "~1.0 picks 1.0.0" bash -c "$FI add 'geo@~1.0' | grep -q 'needs geo 1.0.0'"
"$FI" remove geo > /dev/null
check "unsatisfiable requirement lists versions" bash -c "! $FI add 'geo@^3' 2> e.txt && grep -q 'no version of .geo. satisfies .\^3. (available: 1.0.0 1.2.0 1.3.0 2.0.0' e.txt"
check "bad requirement is reported" bash -c "! $FI add 'geo@^^' 2> e.txt && grep -q 'bad version requirement' e.txt"
check "unknown package is reported" bash -c "! $FI add nothere 2> e.txt && grep -q 'not in the registry' e.txt"
cd "$W"
check "yank hides a version" bash -c "$FI yank geo 2.0.0 | grep -q yanked"
cd app
check "yanked version is skipped" bash -c "$FI add geo | grep -q 'needs geo 1.3.0'"
"$FI" remove geo > /dev/null
cp "$W/registry.index" "$W/registry.good"
sed -i 's/^\(geo 1.3.0 [^ ]* [^ ]*\) [0-9a-f]\{64\}/\1 0000000000000000000000000000000000000000000000000000000000000000/' "$W/registry.index"
check "tampered hash is refused" bash -c "! $FI add geo@1.3 2> e.txt && grep -q 'checksum mismatch for geo 1.3.0' e.txt && [ ! -d .firn/deps/geo ]"
cp "$W/registry.good" "$W/registry.index"
"$FI" add 'geo@=1.2.0' > /dev/null
cat > src/main.fi <<'EOT'
import geo

fn main() -> i32 {
    return geo.add(1, 2)
}
EOT
check "program using the registry package builds" bash -c "$FI build . -o app_bin"
check "built program runs against the pinned version" bash -c "./app_bin; [ \$? -eq 3 ]"
check "update stays inside the requirement" bash -c "$FI update geo | grep -q 'unchanged geo'"
check "offline fetch works from the cache" bash -c "FIRN_OFFLINE=1 $FI fetch"
echo "firn registry: $FAILS failure(s)"
[ "$FAILS" -eq 0 ]

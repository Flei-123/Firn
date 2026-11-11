#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd -P)
PREMERGE=$ROOT/tools/fmt/premerge.sh
INSTALL=$ROOT/tools/fmt/install_hook.sh

WORK=$(mktemp -d)
[ -n "${KEEP:-}" ] || trap 'rm -rf "$WORK"' EXIT

FAIL=0
PASS=0
check() {
    if [ "$2" -eq 0 ]; then PASS=$((PASS + 1)); else echo "  FAIL  $1"; FAIL=$((FAIL + 1)); fi
}

if [ -z "${FIRNFMT:-}" ]; then
    COMPILER=${FIRNC:-$ROOT/compiler/target/release/firnc}
    FIRNLIB="$ROOT/lib" "$COMPILER" -o "$WORK/firnfmt" "$ROOT/tools/fmt/firnfmt.fi" || { echo "cannot build firnfmt"; exit 1; }
    FIRNFMT=$WORK/firnfmt
fi
export FIRNFMT

UGLY='fn main( ) -> i32 {
  return    1
}'
printf '%s\n' "$UGLY" > "$WORK/ugly.fi"
"$FIRNFMT" "$WORK/ugly.fi" > "$WORK/pretty.fi"

REPO=$WORK/repo
mkdir -p "$REPO/lib" "$REPO/tests/lexneg" "$REPO/tools/fmt"
cp "$PREMERGE" "$REPO/tools/fmt/premerge.sh"
cd "$REPO" || exit 1
git init -q -b main . && git config core.hooksPath "$REPO/.git/hooks"
cp "$WORK/pretty.fi" lib/old.fi
printf 'text\n' > notes.txt
git add -A && git commit -q -m base
git checkout -q -b topic

run() { bash "$PREMERGE" main > "$WORK/out" 2>&1; echo $?; }

check "clean branch passes" "$(run)"

cp "$WORK/ugly.fi" lib/new_ugly.fi
rc=$(run)
check "untracked unformatted file is rejected" "$([ "$rc" = 1 ] && echo 0 || echo 1)"
check "rejected file is listed" "$(grep -q 'lib/new_ugly.fi' "$WORK/out" && echo 0 || echo 1)"

cp "$WORK/pretty.fi" lib/new_ugly.fi
check "formatted new file passes" "$(run)"

cp "$WORK/ugly.fi" lib/old.fi
rc=$(run)
check "modified file turned unformatted is rejected" "$([ "$rc" = 1 ] && echo 0 || echo 1)"

git add -A && git commit -q -m ugly
rc=$(run)
check "committed unformatted file is rejected" "$([ "$rc" = 1 ] && echo 0 || echo 1)"

bash "$PREMERGE" --fix main > /dev/null 2>&1
check "--fix repairs it" "$(run)"

git add -A && git commit -q -m fixed
cp "$WORK/ugly.fi" tests/lexneg/skipped.fi
cp "$WORK/ugly.fi" notes.txt
ln -s lib/old.fi linked.fi
check "lexneg, non-.fi and symlinks are skipped" "$(run)"

git checkout -q main
git branch -q -D topic
check "unknown base is an error" "$(bash "$PREMERGE" nosuchbase > /dev/null 2>&1; [ $? -eq 2 ] && echo 0 || echo 1)"

bash "$INSTALL" > /dev/null 2>&1
check "hook installed" "$([ -x .git/hooks/pre-push ] && grep -q firn-fmt-premerge-hook .git/hooks/pre-push && echo 0 || echo 1)"
bash "$INSTALL" > /dev/null 2>&1
check "hook reinstall is idempotent" "$?"
printf '#!/bin/sh\nexit 0\n' > .git/hooks/pre-push
bash "$INSTALL" > /dev/null 2>&1
check "foreign hook is not overwritten" "$([ $? -eq 1 ] && echo 0 || echo 1)"
bash "$INSTALL" --force > /dev/null 2>&1
check "--force replaces a foreign hook" "$(grep -q firn-fmt-premerge-hook .git/hooks/pre-push && echo 0 || echo 1)"

mkdir -p "$WORK/remote.git" && git init -q --bare "$WORK/remote.git"
git remote add origin "$WORK/remote.git"
git checkout -q -b feature
cp "$WORK/ugly.fi" lib/pushed.fi && git add -A && git commit -q -m ugly
git push -q origin main > /dev/null 2>&1
git fetch -q origin
git push -q origin feature:main > "$WORK/push.out" 2>&1
check "push of unformatted file to main is blocked" "$([ $? -ne 0 ] && echo 0 || echo 1)"
git push -q origin feature:other > /dev/null 2>&1
check "push to another branch is not checked" "$?"
FIRN_SKIP_FMT=1 git push -q origin feature:main > /dev/null 2>&1
check "FIRN_SKIP_FMT=1 bypasses the hook" "$?"

echo "premerge check: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

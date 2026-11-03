#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
. "$ROOT/tools/brand.sh"
OLD=$BRAND_CLI
NEW=${1:-fiero}
D=$(mktemp -d)
cleanup() { git -C "$ROOT" worktree remove --force "$D/wt" >/dev/null 2>&1; git -C "$ROOT" branch -D "rename-check-$$" >/dev/null 2>&1; rm -rf "$D"; }
trap cleanup EXIT
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL $1"; }
git worktree add -q -b "rename-check-$$" "$D/wt" HEAD || exit 1
cd "$D/wt"
R="python3 -I tools/rename/rename.py"
$R --from "$OLD" --to "$NEW" > "$D/dry.txt" && grep -q '^DRY RUN' "$D/dry.txt" && ok "dry run reports" || bad "dry run reports"
[ -z "$(git status --porcelain)" ] && ok "dry run changes nothing" || bad "dry run changes nothing"
$R --from "$OLD" --to "$NEW" --apply > "$D/apply.txt" && ok "apply" || { bad "apply"; tail -5 "$D/apply.txt"; }
grep -q "files still containing '$OLD': 0" "$D/apply.txt" && ok "no leftovers" || { bad "no leftovers"; tail -8 "$D/apply.txt"; }
[ -f compiler/src/main.rs ] && [ -z "$(git ls-files | grep -i "$OLD")" ] && ok "no old names in paths" || bad "no old names in paths"
grep -q "cli = \"$NEW\"" brand.toml && ok "brand.toml renamed" || bad "brand.toml renamed"
git add -A && git commit -q -m "rename" || bad "commit"
$R --from "$NEW" --to "$OLD" --apply > "$D/back.txt" && ok "apply back" || bad "apply back"
git add -A
[ -z "$(git diff --cached --stat HEAD~1)" ] && ok "round trip is exact" || { bad "round trip is exact"; git diff --cached --stat HEAD~1 | tail -5; }
echo "passed $PASS failed $FAIL"
[ "$FAIL" = 0 ]

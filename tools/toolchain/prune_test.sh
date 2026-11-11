#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRN=$ROOT
. "$ROOT/tools/testkit/testkit.sh"
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
tk_tmpdir prune-test
unset FIRNLIB FIRN_TOOLCHAIN
export FIRN_HOME=$D/home
FIRN_BIN=$D/firn
FIRNLIB=$ROOT/lib "$FIRNC" tools/toolchain/firn_main.fi -o "$FIRN_BIN" > "$D/build.log" 2>&1 || { cat "$D/build.log"; bad "build firn"; tk_summary prune; exit 1; }

mkdir -p "$D/cwd" "$D/repo_a" "$D/repo_b/sub" "$D/proj"
make_versions() {
    rm -rf "$FIRN_HOME"; mkdir -p "$FIRN_HOME/versions"
    for v in 0.1.1 0.1.2 0.1.3 0.1.4 0.1.5 0.1.6 0.1.7 0.1.8; do
        mkdir -p "$FIRN_HOME/versions/$v"; echo ready > "$FIRN_HOME/versions/$v/.ready"
    done
    mkdir -p "$FIRN_HOME/versions/0.1.9"
    ln -s versions/0.1.8 "$FIRN_HOME/current"
}
has_version() { [ -d "$FIRN_HOME/versions/$1" ]; }
versions() { ls "$FIRN_HOME/versions" | tr '\n' ' '; }
run() { (cd "$D/cwd" && "$FIRN_BIN" prune "$@" > "$D/out" 2>&1); }

echo "firn 0.1.2" > "$D/repo_a/firn.toolchain"
echo "firn >=0.1.3" > "$D/repo_b/sub/firn.toolchain"
printf '# name path dir\nrepo_a %s .\nrepo_b %s sub\nmissing %s/none .\n' "$D/repo_a" "$D/repo_b" "$D" > "$D/repos.txt"
echo "firn 0.1.1" > "$D/proj/firn.toolchain"

make_versions
run --dry-run --keep 1 --repos "$D/repos.txt"; code=$?
[ "$code" = 0 ] && ok "dry run exits 0" || bad "dry run exits 0 ($code)"
[ "$(versions)" = "0.1.1 0.1.2 0.1.3 0.1.4 0.1.5 0.1.6 0.1.7 0.1.8 0.1.9 " ] && ok "dry run removes nothing" || bad "dry run removes nothing ($(versions))"
grep -q "remove  0.1.4 (dry run)" "$D/out" && grep -q "keep    0.1.8 (current)" "$D/out" && ok "dry run lists plan" || bad "dry run lists plan"
grep -q "keep    0.1.2 (registry $D/repo_a/firn.toolchain)" "$D/out" && ok "registry pin reported" || bad "registry pin reported"

make_versions
run --keep 1 --repos "$D/repos.txt"; code=$?
[ "$code" = 0 ] && ok "prune exits 0" || bad "prune exits 0 ($code)"
[ "$(versions)" = "0.1.2 0.1.8 0.1.9 " ] && ok "keep 1 + registry pin" || bad "keep 1 + registry pin ($(versions))"
grep -q "prune: 6 removed, 2 kept" "$D/out" && ok "summary counts" || bad "summary counts ($(tail -1 "$D/out"))"
[ -L "$FIRN_HOME/current" ] && [ "$(readlink "$FIRN_HOME/current")" = versions/0.1.8 ] && ok "current link untouched" || bad "current link untouched"

make_versions
run --repos "$D/repos.txt"
[ "$(versions)" = "0.1.2 0.1.6 0.1.7 0.1.8 0.1.9 " ] && ok "default keeps 3 newest" || bad "default keeps 3 newest ($(versions))"

make_versions
run --keep 0 --repos "$D/none.txt"
[ "$(versions)" = "0.1.8 0.1.9 " ] && ok "keep 0 leaves only current (+ unfinished dir)" || bad "keep 0 leaves only current ($(versions))"

make_versions
echo "$D/proj/firn.toolchain" > "$FIRN_HOME/projects"
echo "$D/gone/firn.toolchain" >> "$FIRN_HOME/projects"
echo "pin=0.1.4" > "$FIRN_HOME/config"
run --keep 0 --repos "$D/none.txt"
[ "$(versions)" = "0.1.1 0.1.4 0.1.8 0.1.9 " ] && ok "projects file and config pin are kept" || bad "projects file and config pin are kept ($(versions))"

make_versions
echo "firn 0.1.6" > "$D/cwd/firn.toolchain"
run --keep 0 --repos "$D/none.txt"
rm -f "$D/cwd/firn.toolchain"
[ "$(versions)" = "0.1.6 0.1.8 0.1.9 " ] && ok "pin of the working directory is kept" || bad "pin of the working directory is kept ($(versions))"

make_versions
run --keep abc; code=$?
[ "$code" = 2 ] && ok "bad --keep is a usage error" || bad "bad --keep is a usage error ($code)"
[ "$(ls "$FIRN_HOME/versions" | wc -l)" = 9 ] && ok "bad --keep removes nothing" || bad "bad --keep removes nothing"

make_versions
rm "$FIRN_HOME/current"
run --keep 2 --repos "$D/none.txt"
[ "$(versions)" = "0.1.7 0.1.8 0.1.9 " ] && ok "without current only newest are kept" || bad "without current ($(versions))"

"$FIRN_BIN" help > "$D/out" 2>&1
grep -q "prune \[--keep N\] \[--dry-run\]" "$D/out" && ok "help documents prune" || bad "help documents prune"

tk_summary prune

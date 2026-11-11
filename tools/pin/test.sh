#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
VERSION=9.9.9
COMMIT=0123456789abcdef0123456789abcdef01234567
OLD_COMMIT=fedcba9876543210fedcba9876543210fedcba98

make_toolchain() {
    mkdir -p "$D/home/versions/$VERSION/bin"
    printf '#!/bin/sh\n' > "$D/home/versions/$VERSION/bin/firnc"
    chmod +x "$D/home/versions/$VERSION/bin/firnc"
    echo "$COMMIT" > "$D/home/versions/$VERSION/COMMIT"
}

make_repo() {
    local repo=$1 with_doc=$2
    mkdir -p "$repo/vendor/firn" "$repo/docs"
    echo "firn 0.0.1" > "$repo/firn.toolchain"
    echo "$OLD_COMMIT" > "$repo/vendor/firn/COMMIT"
    if [ "$with_doc" = 1 ]; then
        printf 'Intro\nFirn toolchain pin 0.0.1 (%s), shared by all.\nOutro\n' "$OLD_COMMIT" > "$repo/docs/firn-pin.md"
    fi
    git -C "$repo" init -q
    git -C "$repo" add -A
    git -C "$repo" commit -q -m init
}

run_bump() {
    local script=$1 repo=$2
    FIRN_HOME=$D/home \
        bash "$script" "$VERSION" --only osum --repo "osum=$repo" --no-smoke --commit > "$D/out.log" 2>&1
}

check_doc_follows() {
    local script=$1 repo=$2
    run_bump "$script" "$repo" || return 1
    grep -qx "Firn toolchain pin $VERSION ($COMMIT), shared by all." "$repo/docs/firn-pin.md" || return 1
    [ -z "$(git -C "$repo" status --porcelain)" ] || return 1
    git -C "$repo" show --stat --format= HEAD | grep -q 'docs/firn-pin.md'
}

make_toolchain
FAILS=0
fail() { echo "FAIL: $1"; FAILS=$((FAILS + 1)); }

make_repo "$D/with_doc" 1
check_doc_follows "$HERE/bump.sh" "$D/with_doc" || fail "doc line not updated and committed"

make_repo "$D/no_doc" 0
run_bump "$HERE/bump.sh" "$D/no_doc" || fail "bump errors without docs/firn-pin.md"
[ ! -e "$D/no_doc/docs/firn-pin.md" ] || fail "missing doc was created"

make_repo "$D/mutant_repo" 1
grep -v '^    update_pin_doc "\$repo"$' "$HERE/bump.sh" > "$D/mutant.sh"
if ! grep -q update_pin_doc "$D/mutant.sh" || [ "$(grep -c 'update_pin_doc "\$repo"' "$D/mutant.sh")" != 0 ]; then
    fail "mutant not built"
elif check_doc_follows "$D/mutant.sh" "$D/mutant_repo"; then
    fail "mutant (doc line stays old) was not detected"
fi

[ $FAILS = 0 ] && echo "pin test: ok" || exit 1

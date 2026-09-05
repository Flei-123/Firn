#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proves that NOTHING was changed in the test262 subset.
#   1. the sha256 of the archive itself,
#   2. the archive unpacks to exactly the expected number of files,
#   3. every single file has the sha256 sum from subset.sha256.
# Return: 0 = unchanged, 1 = a deviation was found.
set -euo pipefail
cd "$(dirname "$0")/../.."

COMMIT="3655e7464de3d52643ecddd4b5f9f4f3e7f62398"
ARCHIVE="testdata/test262/test262-subset.tar.gz"
SUMS="testdata/test262/subset.sha256"
ARCHIVE_SHA="6550c1f6aefcfe1dfcbc746c4255a5a9039eae124de2cdaea523ec64eb5db938"
EXPECTED_FILES=32893
DEST="${1:-}"

echo "== check the test data: $ARCHIVE =="
echo "   reference: tc39/test262 @ $COMMIT"

got=$(sha256sum "$ARCHIVE" | cut -d' ' -f1)
if [ "$got" != "$ARCHIVE_SHA" ]; then
    echo "   ERROR: the archive has the sum $got instead of $ARCHIVE_SHA"
    exit 1
fi
echo "   archive : sha256 matches"

n=$(awk 'NF==2' "$SUMS" | wc -l)
if [ "$n" -ne "$EXPECTED_FILES" ]; then
    echo "   ERROR: $n lines in $SUMS instead of $EXPECTED_FILES"
    exit 1
fi
echo "   manifest: $n file sums"

if [ -z "$DEST" ]; then
    DEST=$(mktemp -d)
    trap 'rm -rf "$DEST"' EXIT
    tar xzf "$ARCHIVE" -C "$DEST"
fi
count=$(find "$DEST/test" "$DEST/harness" -type f -name '*.js' | wc -l)
if [ "$count" -ne "$EXPECTED_FILES" ]; then
    echo "   ERROR: $count files unpacked instead of $EXPECTED_FILES"
    exit 1
fi
echo "   unpacked: $count files"

if (cd "$DEST" && sha256sum -c --status "$OLDPWD/$SUMS"); then
    echo "   sha256  : all $EXPECTED_FILES file sums match"
else
    echo "   ERROR: at least one file sum differs:"
    (cd "$DEST" && sha256sum -c "$OLDPWD/$SUMS" 2>&1 | grep -v ': OK$' | head -20 | sed 's/^/          /') || true
    exit 1
fi
echo
echo "OK: test262 subset unchanged ($EXPECTED_FILES files, sha256 as upstream $COMMIT)."

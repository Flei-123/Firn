#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
repo=${1:?usage: blobs.sh REPO VERSION}
version=${2:?usage: blobs.sh REPO VERSION}
file=$repo/vendor/net/BLOBS
net=${FIRN_HOME:-$HOME/.firn}/versions/$version/lib/net
[ -f "$file" ] || exit 0
[ -d "$net" ] || { echo "blobs: $net is missing" >&2; exit 1; }
tmp=$(mktemp "$file.XXXXXX")
grep '^#' "$file" > "$tmp"
while read -r _ name; do
    [ -n "$name" ] || continue
    [ -f "$net/${name#net/}" ] || { echo "blobs: no $name in toolchain $version" >&2; rm -f "$tmp"; exit 1; }
    echo "$(git hash-object "$net/${name#net/}") $name" >> "$tmp"
done < <(grep -v '^#' "$file")
mv "$tmp" "$file"

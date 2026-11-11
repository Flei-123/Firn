#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
FIRN=${FIRN:-$(cd "$HERE/../.." && pwd)}
. "$FIRN/tools/testkit/testkit.sh"
REGISTRY=${REGISTRY:-$HERE/repos.txt}
FIRN_HOME=${FIRN_HOME:-$HOME/.firn}
reference=

check_repo() {
    local name=$1 repo=$2 pin_dir=$3 pin=$2/$3/firn.toolchain word version
    if [ ! -d "$repo" ]; then bad "$name: repo exists"; return; fi
    if [ ! -f "$pin" ]; then bad "$name: firn.toolchain exists"; return; fi
    read -r word version < "$pin"
    if [ "$word" = firn ] && [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then ok "$name: firn.toolchain well-formed"; else bad "$name: firn.toolchain well-formed"; return; fi
    [ -n "$reference" ] || reference=$version
    if [ "$version" = "$reference" ]; then ok "$name: pin $version equals $reference"; else bad "$name: pin $version differs from $reference"; fi
    check "$name: $version installed" test -x "$FIRN_HOME/versions/$version/bin/firnc"
    if git -C "$repo" ls-files -s -- vendor/firn 2> /dev/null | grep -qv '^120000'; then bad "$name: no tracked vendor/firn"; else ok "$name: no tracked vendor/firn"; fi
}

while read -r name path pin_dir; do
    case "$name" in ''|'#'*) continue ;; esac
    check_repo "$name" "$path" "$pin_dir"
done < "$REGISTRY"
printf 'SUMMARY pass=%d fail=%d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]

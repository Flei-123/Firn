#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
FIRN=$(cd "$HERE/../.." && pwd)
. "$FIRN/tools/testkit/testkit.sh"
tk_tmpdir pin-check
export FIRN_HOME=$D/home
for v in 0.1.1 0.1.2; do mkdir -p "$FIRN_HOME/versions/$v/bin" "$FIRN_HOME/versions/$v/lib/net"; : > "$FIRN_HOME/versions/$v/bin/firnc"; chmod +x "$FIRN_HOME/versions/$v/bin/firnc"; done
printf 'a\n' > "$FIRN_HOME/versions/0.1.1/lib/net/wire.fi"
printf 'b\n' > "$FIRN_HOME/versions/0.1.2/lib/net/wire.fi"
for r in a b; do mkdir -p "$D/$r"; git -C "$D/$r" init -q; echo "firn 0.1.1" > "$D/$r/firn.toolchain"; done
printf 'x %s\ny %s\n' "$D/a" "$D/b" | awk '{print $1" "$2" ."}' > "$D/repos.txt"
export REGISTRY=$D/repos.txt
check "same pins pass" bash "$HERE/check_all.sh"
echo "firn 0.1.2" > "$D/b/firn.toolchain"
if bash "$HERE/check_all.sh" > /dev/null 2>&1; then bad "differing pins fail"; else ok "differing pins fail"; fi
echo "firn 0.1.9" > "$D/a/firn.toolchain"; echo "firn 0.1.9" > "$D/b/firn.toolchain"
if bash "$HERE/check_all.sh" > /dev/null 2>&1; then bad "missing install fails"; else ok "missing install fails"; fi
echo "firn 0.1.1" > "$D/a/firn.toolchain"; echo "firn 0.1.1" > "$D/b/firn.toolchain"
mkdir -p "$D/b/vendor/firn"; echo x > "$D/b/vendor/firn/COMMIT"; git -C "$D/b" add vendor/firn/COMMIT
if bash "$HERE/check_all.sh" > /dev/null 2>&1; then bad "tracked vendor/firn fails"; else ok "tracked vendor/firn fails"; fi
git -C "$D/b" rm -qf vendor/firn/COMMIT
mkdir -p "$D/a/vendor/net"; printf '# h\n0000 net/wire.fi\n' > "$D/a/vendor/net/BLOBS"
bash "$HERE/blobs.sh" "$D/a" 0.1.2
check_out "blobs refreshed" "$(git hash-object "$FIRN_HOME/versions/0.1.2/lib/net/wire.fi") net/wire.fi" tail -1 "$D/a/vendor/net/BLOBS"
check_out "blobs keeps comment" "# h" head -1 "$D/a/vendor/net/BLOBS"
tk_summary pin

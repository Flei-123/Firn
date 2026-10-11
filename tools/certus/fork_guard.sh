#!/usr/bin/env bash
# tools/certus/fork_guard.sh -- stop /root/firnc-gc (the Certus compiler fork, NOT a git tree) from drifting further.
#
# Rule (docs/CERTUS-COMPILER-MERGE.md section 6): a compiler or GC-library change is made on Firn main FIRST. The fork gets
# no new edits; Certus moves to a main build (gate in section 5). This script makes a violation visible:
#
#   bash tools/certus/fork_guard.sh --freeze    record the sha256 of every file the fork embeds (run ONCE, after review)
#   bash tools/certus/fork_guard.sh --check     exit 1 and list every file that changed, appeared or vanished since the freeze
#   bash tools/certus/fork_guard.sh --report    per file: identical to main / differs from main / only in the fork
#
# The manifest lives in the repo (tools/certus/firnc-gc.sha256), so the freeze point is reviewable and versioned.
set -uo pipefail
cd "$(dirname "$0")/../.."
FORK=${FIRNC_GC:-/root/firnc-gc}
MAN=tools/certus/firnc-gc.sha256
files() { (cd "$FORK" && ls compiler/src/*.rs compiler/Cargo.toml lib/gc/*.fi lib/test/*.fi 2>/dev/null | sort); }
case "${1:---check}" in
--freeze)
    (cd "$FORK" && sha256sum $(files)) > "$MAN"
    echo "froze $(wc -l < "$MAN") files into $MAN" ;;
--check)
    [ -f "$MAN" ] || { echo "no manifest: run --freeze first"; exit 2; }
    bad=0
    cur=$(cd "$FORK" && sha256sum $(files))
    if ! diff <(sort -k2 "$MAN") <(echo "$cur" | sort -k2) > /tmp/fork_guard.$$; then
        echo "FORK DRIFT: $FORK changed since the freeze:"; sed 's/^/  /' /tmp/fork_guard.$$ | head -30; bad=1
        echo "Make the change on Firn main instead (docs/CERTUS-COMPILER-MERGE.md section 6)."
    else echo "ok: $FORK is unchanged since the freeze ($(wc -l < "$MAN") files)"; fi
    rm -f /tmp/fork_guard.$$
    exit $bad ;;
--report)
    for f in $(files); do
        if [ ! -f "$f" ]; then echo "only-in-fork   $f"
        elif cmp -s "$FORK/$f" "$f"; then echo "identical      $f"
        else echo "differs        $f"; fi
    done ;;
*) echo "usage: $0 --freeze|--check|--report"; exit 2 ;;
esac

#!/usr/bin/env bash
# tools/mc/merge_main.sh <branch> "<message>" -- merge a finished, GREEN branch into
# main and push (standing rule of the owner: own projects may be merged and pushed
# without asking once the tests are green). Serialised with a lock because several
# workers finish at the same time. Run it from ANY directory.
#
# Refuses when: main has uncommitted changes in files the merge touches, or the merge
# conflicts (then it aborts the merge and tells you; rebase your branch on main first).
set -uo pipefail
BR=${1:?branch}; MSG=${2:-"Merge $BR"}
MAIN=/root/jarvis/projects/u_DiS4in7esMF1/firn
exec 9>/tmp/firn-main-merge.lock
flock -w 600 9 || { echo "merge_main: lock timeout" >&2; exit 2; }
cd "$MAIN" || exit 2
[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || { echo "merge_main: main worktree is not on main" >&2; exit 2; }
if ! git merge --no-ff -m "$MSG" "$BR"; then
    git merge --abort 2>/dev/null
    echo "merge_main: CONFLICT -- merge aborted; merge main into $BR, fix, test, retry" >&2
    exit 1
fi
git push origin main || { echo "merge_main: merged locally, push failed" >&2; exit 3; }
echo "merge_main: $BR merged and pushed ($(git rev-parse --short HEAD))"

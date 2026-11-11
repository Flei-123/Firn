#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail

MARKER="# firn-fmt-premerge-hook"
FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

hooks_dir=$(git rev-parse --path-format=absolute --git-path hooks 2>/dev/null)
[ -d "$hooks_dir" ] || { echo "install_hook: not inside a git repository" >&2; exit 2; }
target=$hooks_dir/pre-push

if [ -f "$target" ] && ! grep -q "$MARKER" "$target" && [ "$FORCE" -ne 1 ]; then
    echo "install_hook: $target exists and is not ours (use --force to replace)" >&2
    exit 1
fi

cat > "$target" <<HOOK
#!/usr/bin/env bash
$MARKER
[ "\${FIRN_SKIP_FMT:-}" = 1 ] && exit 0
pushing_main=0
while read -r local_ref local_sha remote_ref remote_sha; do
    [ "\$remote_ref" = refs/heads/main ] && pushing_main=1
done
[ "\$pushing_main" -eq 1 ] || exit 0
top=\$(git rev-parse --show-toplevel) || exit 1
[ -f "\$top/tools/fmt/premerge.sh" ] || exit 0
git rev-parse -q --verify origin/main > /dev/null || exit 0
exec bash "\$top/tools/fmt/premerge.sh"
HOOK
chmod +x "$target"
echo "installed $target (push to main runs tools/fmt/premerge.sh; FIRN_SKIP_FMT=1 skips)"

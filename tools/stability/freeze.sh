#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."

DEST=tests/stable
mkdir -p "$DEST/neg"

copy_new() {
    local mode=$1 src_dir=$2 dest_dir=$3 f
    for f in $(python3 -I tools/stability/select_corpus.py . "$mode"); do
        if [ -e "$dest_dir/$f" ]; then
            continue
        fi
        cp "$src_dir/$f" "$dest_dir/$f"
    done
}

copy_new positive tests "$DEST"
copy_new negative tests/neg "$DEST/neg"

(cd "$DEST" && find . -name '*.fi' | LC_ALL=C sort | xargs sha256sum > MANIFEST.sha256)
echo "stable corpus: $(ls "$DEST"/*.fi | wc -l) programs, $(ls "$DEST"/neg/*.fi | wc -l) rejected programs"

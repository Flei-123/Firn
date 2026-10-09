#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/dialog/filedlg_run.sh -- build and run the checks of the fUi file dialog (lib/fui/filedlg.fi).
#
#     tools/dialog/filedlg_run.sh [logic|timing|live|all] [pngdir]
#
#   logic   tools/fui/filedlg_main.fi built unoptimised, run on a temporary directory tree (navigation, sort,
#           filter, selection, keys, save, folder mode, new folder, 5000 entries, pixels, contrast, both themes)
#           expected last line: FILEDLG PASSED
#   timing  the same program built with --opt-level=release-fast; look at the FRAME_MS lines: every one <= 16
#   live    tools/dialog/filedlg_live.py on a private Xvfb with xdotool (skips when Xvfb/xdotool/xwd/PIL are missing)
#           expected last line: filedlg (live): all checks passed
#   all     all three (default)
#
# Everything lives in a mktemp -d directory that is removed at the end (the tree, the binaries, the screenshots);
# only the proof pictures go to [pngdir] (default: nowhere). The heavy part (the compiles) is small: no heavy wrapper.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
WHAT=${1:-all}
PNGDIR=${2:-}
D=$(mktemp -d)
trap 'chmod -R u+rwx "$D" 2>/dev/null; rm -rf "$D"' EXIT
rc=0

make_tree() {
  local T=$1
  mkdir -p "$T"/alpha/sub1 "$T"/beta "$T"/zeta "$T"/.hidden_dir "$T"/noaccess "$T"/big
  echo x > "$T"/alpha/inner.txt
  head -c 10 /dev/zero > "$T"/a.png
  head -c 5000 /dev/zero > "$T"/B.PNG
  head -c 100 /dev/zero > "$T"/c.txt
  head -c 1 /dev/zero > "$T"/d.jpg
  head -c 50 /dev/zero > "$T"/existing.txt
  head -c 7 /dev/zero > "$T"/Notes.TXT
  head -c 20000 /dev/zero > "$T"/x.bin
  : > "$T"/.hidden_file
  ln -s alpha "$T"/link_dir
  ln -s a.png "$T"/link_file
  ln -s /nonexistent-target "$T"/broken
  touch -d 2020-01-01T12:00:00 "$T"/a.png
  touch -d 2022-06-01T12:00:00 "$T"/B.PNG
  touch -d 2021-03-03T12:00:00 "$T"/c.txt
  touch -d 2019-05-05T12:00:00 "$T"/d.jpg
  touch -d 2023-01-01T12:00:00 "$T"/existing.txt
  touch -d 2024-02-02T12:00:00 "$T"/Notes.TXT
  touch -d 2018-08-08T12:00:00 "$T"/x.bin
  seq -f "$T/big/f%04g.dat" 0 4999 | xargs touch
}

build() { # build <opt> <out>
  "$FIRNC" --opt-level="$1" -o "$2" "$ROOT/tools/fui/filedlg_main.fi" || return 1
}

if [ "$WHAT" = logic ] || [ "$WHAT" = all ]; then
  echo "== logic (unoptimised)"
  make_tree "$D/tree"
  build dev "$D/filedlg_main" || { echo "build failed"; exit 1; }
  chmod 755 "$D" "$D/tree"
  "$D/filedlg_main" "$D/tree" ${PNGDIR:+"$PNGDIR"} | grep -v '^FRAME_MS\|^NAV_MS' || rc=1
  # the unreadable folder: only an unprivileged user cannot read a chmod 000 folder (root can)
  if [ "$(id -u)" = 0 ] && command -v setpriv >/dev/null; then
    chmod 000 "$D/tree/noaccess"
    chmod -R a+rX "$D/tree" "$D/filedlg_main"
    chmod 000 "$D/tree/noaccess"
    echo "== as nobody: the locked folder"
    setpriv --reuid=65534 --regid=65534 --clear-groups "$D/filedlg_main" "$D/tree" perm | grep -v '^FRAME_MS' || rc=1
    chmod 755 "$D/tree/noaccess"
  else
    echo "  NOTE  not root or no setpriv: the 'Permission denied' check was not run here"
  fi
fi

if [ "$WHAT" = timing ] || [ "$WHAT" = all ]; then
  echo "== timing (release-fast)"
  [ -d "$D/tree" ] || make_tree "$D/tree"
  build release-fast "$D/filedlg_fast" || { echo "build failed"; exit 1; }
  "$D/filedlg_fast" "$D/tree" | grep 'FRAME_MS\|NAV_MS\|FILEDLG' || rc=1
  "$D/filedlg_fast" "$D/tree" | awk '/^FRAME_MS/ { if ($3 > 16.0) { bad=1; print "  WRONG  frame over 16 ms: " $0 } } END { exit bad }' || rc=1
fi

if [ "$WHAT" = live ] || [ "$WHAT" = all ]; then
  echo "== live (Xvfb, xdotool)"
  "$FIRNC" --opt-level=release-fast -o "$D/filedlg_live_main" "$ROOT/tools/dialog/filedlg_live_main.fi" || { echo "build failed"; exit 1; }
  python3 "$ROOT/tools/dialog/filedlg_live.py" "$D/filedlg_live_main" ${PNGDIR:+"$PNGDIR"} || rc=1
fi
exit $rc

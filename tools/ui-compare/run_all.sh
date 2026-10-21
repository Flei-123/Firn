#!/bin/sh
# Measure all apps with apps/*/*/measure.json (web apps get a default one), compare shots, rebuild the gallery.
# Usage: run_all.sh [run_all.py options]   e.g. run_all.sh --only flei --runs 5 --skip-build
S="$(cd "$(dirname "$0")" && pwd)"
UIVGL_DIR="${UIVGL_DIR:-$S}"; export UIVGL_DIR
UIVGL_PORT="${UIVGL_PORT:-14291}"
UIVGL_HTTP="http://127.0.0.1:$UIVGL_PORT"; export UIVGL_HTTP
cd "$S" || exit 1
python3 -m http.server "$UIVGL_PORT" --directory "$UIVGL_DIR" >/dev/null 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null; rm -rf "$UIVGL_DIR"/.chrome-profile-*' EXIT
sleep 1
rc=0
python3 run_all.py "$@" || rc=$?
python3 compare.py --table || true
python3 gallery.py || true
exit $rc

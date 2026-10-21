#!/bin/sh
# Measure all apps with apps/*/*/measure.json, compare shots, rebuild the gallery.
# Usage: run_all.sh [run_all.py options]   e.g. run_all.sh --only flei --runs 5 --skip-build
S="$(cd "$(dirname "$0")" && pwd)"
UIVGL_DIR="${UIVGL_DIR:-$S}"; export UIVGL_DIR
cd "$S" || exit 1
rc=0
python3 run_all.py "$@" || rc=$?
python3 compare.py --table || true
python3 gallery.py || true
exit $rc

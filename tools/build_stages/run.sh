#!/usr/bin/env bash
# Misst die BAUSTUFEN gegeneinander (DESIGN_GOALS.md §5).
#
# Frage: Ist '--opt-level=dev-fast' (nur debugerhaltende Durchgaenge) nahe genug
# an 'release-fast'? Zielwert laut DESIGN_GOALS.md §5: 2-3x, nicht 30x wie bei
# Rusts Debug-Builds.
#
# Aufruf:  bash tools/build_stages/run.sh [LAEUFE]   (Standard 5, Median)
set -euo pipefail
cd "$(dirname "$0")/../.."
RUNS="${1:-5}"
python3 tools/build_stages/measure.py "$RUNS"

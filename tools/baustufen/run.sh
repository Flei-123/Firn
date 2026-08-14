#!/usr/bin/env bash
# Misst die BAUSTUFEN gegeneinander (DESIGNZIELE.md §5).
#
# Frage: Ist '--opt-level=dev-fast' (nur debugerhaltende Durchgaenge) nahe genug
# an 'release-fast'? Zielwert laut DESIGNZIELE.md §5: 2-3x, nicht 30x wie bei
# Rusts Debug-Builds.
#
# Aufruf:  bash tools/baustufen/run.sh [LAEUFE]   (Standard 5, Median)
set -euo pipefail
cd "$(dirname "$0")/../.."
RUNS="${1:-5}"
python3 tools/baustufen/messen.py "$RUNS"

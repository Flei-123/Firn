#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Measures the BUILD STAGES against each other (DESIGN_GOALS.md 5).
#
# Question: is '--opt-level=dev-fast' (only debug-preserving passes) close
# enough to 'release-fast'? Target value per DESIGN_GOALS.md 5: 2-3x, not 30x
# as with Rust's debug builds.
#
# Usage:  bash tools/build_stages/run.sh [RUNS]   (default 5, median)
set -euo pipefail
cd "$(dirname "$0")/../.."
RUNS="${1:-5}"
python3 tools/build_stages/measure.py "$RUNS"

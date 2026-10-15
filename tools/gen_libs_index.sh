#!/usr/bin/env bash
# Regenerate docs/LIBS.md (index of lib/*). Usage: bash tools/gen_libs_index.sh [--check|--out FILE]
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 -I tools/gen_libs_index.py "$@"

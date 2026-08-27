#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/bench82/ra_baseline.sh -- the register allocation over the three
# workloads of docs/BENCHMARKS.md §4, in one go. Round 87.
#
#   bash tools/bench82/ra_baseline.sh [outdir]
#
# Writes one <name>.txt per workload with the report of ra_report.py and
# prints the three summary lines. Takes about a minute.
set -uo pipefail
cd "$(dirname "$0")/../.."
OUT="${1:-/tmp/ra_baseline}"
mkdir -p "$OUT"
export FIRNLIB="$PWD/lib"
FIRNC=compiler/target/release/firnc

run() { # <name> <source>
    local name="$1" src="$2"
    FIRN_RA_STATS=1 "$FIRNC" --opt-level=release-fast -o /dev/null "$src" \
        2>"$OUT/$name.raw" >/dev/null
    python3 tools/bench82/ra_report.py --hot 10 < "$OUT/$name.raw" > "$OUT/$name.txt"
    echo "--- $name ($src)"
    cat "$OUT/$name.txt"
}

run deflate  tools/stdlib81/deflate_cli.fi
run js       lib/js/run_main.fi
run compiler bin/firnc1.fi

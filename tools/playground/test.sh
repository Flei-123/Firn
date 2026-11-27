#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
export PYTHONDONTWRITEBYTECODE=1
for suite in tests/*.test.mjs; do
    node "$suite" | grep -v '^PASS' || true
    node "$suite" > /dev/null
done
python3 test.py
exec python3 browser_test.py

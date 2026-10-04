#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/manifest-lint/run.sh -- the linter against its own tests and the fixtures (copies of the real manifests).
# Used by test.sh section 73; the projects call manifest_lint.py directly in their own CI.
set -euo pipefail
cd "$(dirname "$0")"
python3 test_manifest_lint.py 2>&1 | tail -4
python3 manifest_lint.py fixtures/good/*.ACTIONS 2>&1 | grep -c '^OK ' | sed 's/^/manifests accepted: /'
[ "$(python3 manifest_lint.py fixtures/good/*.ACTIONS 2>&1 | grep -c '^OK ')" -eq "$(ls fixtures/good | wc -l)" ]
echo "manifest-lint: PASS"

#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/uiextras/run.sh -- lib/i18n/human.fi held against ICU (PyICU) and zoneinfo.
#
#   relative times ("5 minutes ago", every unit and plural category), byte sizes, date and
#   time styles (full/long/medium/short x medium/short), percent, lists, and the wall clock of
#   eight zones read from the system's TZif files -- in en de fr it es pl cs ru, thousands of
#   random inputs each; the expectation is ICU's own answer (an "or" list: Babel's, PyICU
#   cannot ask ICU for one). See docs/UI_EXTRAS.md.
#
# Needs python3 with PyICU (and babel, zoneinfo). Without them: SKIP, exit 0.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
[ -n "$QR_PYDEPS" ] && export PYTHONPATH="$QR_PYDEPS${PYTHONPATH:+:$PYTHONPATH}"
"$FIRNC" --opt-level=release-fast -o "$W/humancli" tools/uiextras/humancli_main.fi
if ! python3 -c 'import icu, babel, zoneinfo' 2>/dev/null; then
    echo "  SKIP: python3 needs PyICU, babel and zoneinfo (QR_PYDEPS=DIR for pip --target packages)"
    exit 0
fi
python3 tools/uiextras/check_human.py "$W/humancli" 1 200
python3 tools/uiextras/check_human.py "$W/humancli" 2 200
echo "UIEXTRAS PASSED"

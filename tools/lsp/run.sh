#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/lsp/run.sh -- THE PROOF FOR THE LANGUAGE SERVER (round 64, point 4).
#
# `firnc --lsp` speaks the Language Server Protocol over standard
# input/output. tools/lsp/client.py is a REAL client: `Content-Length` head,
# JSON body, requests with a number, notifications without. It holds the
# answers against expectations.
#
# Checked:
#   * initialize announces the abilities
#   * didOpen/didChange -> publishDiagnostics with the texts of `firnc`
#     INCLUDING the suggestions of round 64
#   * definition, for a function and for a local variable
#   * hover, completion (with scope!), rename (with scope!)
#   * formatting -- over `firnfmt`, so that the editor and the tool cannot
#     produce two different shapes
#   * counter-checks: nothing in the void, an unknown request is answered,
#     a foreign local is not offered, a struct field is not renamed along
#
# Usage:  bash tools/lsp/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

# The formatter, so that `textDocument/formatting` has something to call.
FMT="$ROOT/.firnfmt"
if [ ! -x "$FMT" ] || [ -n "$(find tools/fmt -name '*.fi' -newer "$FMT" -print -quit 2>/dev/null)" ]; then
    "$FIRNC" -o "$FMT" tools/fmt/firnfmt.fi || exit 1
fi
export FIRNFMT="$FMT"

echo "== the sample program translates and runs =="
"$FIRNC" -o "$ROOT/.lsp-sample.bin" tools/lsp/sample.fi || exit 1
"$ROOT/.lsp-sample.bin"
rc=$?
rm -f "$ROOT/.lsp-sample.bin"
if [ "$rc" -ne 14 ]; then
    echo "  FAIL  tools/lsp/sample.fi yields $rc, expected 14"
    exit 1
fi
echo "  ok    tools/lsp/sample.fi -> $rc"

echo
echo "== the session with a real client =="
python3 tools/lsp/client.py "$FIRNC" tools/lsp/sample.fi
exit $?

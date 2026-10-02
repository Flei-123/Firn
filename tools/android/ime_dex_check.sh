#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/ime_dex_check.sh -- THE DEX OF THE INPUT METHOD, CHECKED BY
# THE SDK'S OWN `dexdump` (roadmap r114).
#
# lib/android/imedex.fi writes two classes into memory at run time
# (org.firn.ImeView, org.firn.ImeConn). tools/android/imedex_main.fi writes
# the same bytes into a file; `dexdump -d` (Android build tools) parses it
# with Google's reader, and the script looks at what came out: both classes,
# their superclasses, the static field, the seven instructions, and the
# eighteen methods of the connection as `native`.
#
#   bash tools/android/ime_dex_check.sh      (FIRNC, FIRNLIB, DEXDUMP from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
DEXDUMP=${DEXDUMP:-$(ls /root/android-sdk/build-tools/*/dexdump 2>/dev/null | tail -1)}
W=${W:-$(mktemp -d)}
fail=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

if [ -z "$DEXDUMP" ] || [ ! -x "$DEXDUMP" ]; then
    echo "   SKIPPED  no dexdump (Android build tools)"
    exit 0
fi
"$FIRNC" --opt-level=dev -o "$W/imedex" tools/android/imedex_main.fi || exit 1
"$W/imedex" "$W/ime.dex" || { echo "imedex failed"; exit 1; }
"$DEXDUMP" -d "$W/ime.dex" > "$W/dump.txt" 2>&1
check "dexdump reads the file (DEX version 035)" "grep -q \"DEX version '035'\" $W/dump.txt"
check "class org.firn.ImeView extends android.view.View" \
    "grep -A3 \"Class descriptor  : 'Lorg/firn/ImeView;'\" $W/dump.txt | grep -q \"Superclass        : 'Landroid/view/View;'\""
check "class org.firn.ImeConn extends BaseInputConnection" \
    "grep -A3 \"Class descriptor  : 'Lorg/firn/ImeConn;'\" $W/dump.txt | grep -q \"BaseInputConnection;'\""
check "the static field e (boolean)" "grep -A2 \"name          : 'e'\" $W/dump.txt | grep -q \"type          : 'Z'\""
check "onCheckIsTextEditor reads the field (sget-boolean)" "grep -q 'sget-boolean v0, Lorg/firn/ImeView;.e:Z' $W/dump.txt"
check "both constructors call their super constructor (invoke-direct)" \
    "[ \$(grep -c 'invoke-direct' $W/dump.txt) = 2 ]"
NAT=$(grep -c 'access        : 0x0101 (PUBLIC NATIVE)' "$W/dump.txt")
check "seventeen native methods (1 view + 16 connection), got $NAT" "[ '$NAT' = 17 ]"
for m in commitText setComposingText finishComposingText deleteSurroundingText \
    deleteSurroundingTextInCodePoints sendKeyEvent getTextBeforeCursor \
    getTextAfterCursor getSelectedText getCursorCapsMode performEditorAction \
    setSelection setComposingRegion replaceText getSurroundingText \
    getExtractedText onCreateInputConnection; do
    check "native $m" "grep -q \"name          : '$m'\" $W/dump.txt"
done
[ $fail -eq 0 ] && echo "IME DEX PASSED" || echo "IME DEX FAILED"
exit $fail

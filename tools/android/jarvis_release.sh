#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_release.sh -- stage a JARVIS APK as a SIGNED store entry in a LOCAL staging store.
#
#   bash tools/android/jarvis_release.sh [--version X.Y.Z] [--apk FILE] [--notes FILE] [--channel test]
#                                        [--staging DIR] [--show]
#
# What it does: takes the release notes of the version out of examples/jarvis_android/RELEASE_NOTES.md (section
# "## <version>"), runs the orientstore tool (`store add --kanal <channel> --aenderungen <notes> <apk>`) against the
# staging store, which signs entry.json / index.json with the staging store's OWN key, and verifies the result.
#
# Defaults: version = newest /root/jarvis-android-builds/jarvis-*.apk, channel = test, staging =
# /root/jarvis-android-builds/store-staging (created with `store init` on first use; its key schluessel.geheim is a
# dev key and stays there).
#
# What it NEVER does: upload anything. The live store (store.fleitec.com, /srv/store) is not touched and its key is not
# read: the script refuses a staging directory under /srv and an ORIENTSTORE_SCHLUESSEL outside the staging directory.
# Publishing to the live store stays a decision of the boss (copy the same `store add` call by hand against the live repo).
#
# The staging store's public key (trust anchor) is printed at the end: an app built for it needs FIRN_APP_STORE_KEY
# (tools/android/jarvis_update_check.sh does that for its throw-away store).
#   ORIENTSTORE_TOOL=... picks the store tool (default: /root/orientstore/werkzeug/store)
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BUILDS=/root/jarvis-android-builds
VERSION=""; APK=""; NOTES=$ROOT/examples/jarvis_android/RELEASE_NOTES.md; CHANNEL=test; STAGING=$BUILDS/store-staging; SHOW=0
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION=$2; shift 2 ;;
        --apk) APK=$2; shift 2 ;;
        --notes) NOTES=$2; shift 2 ;;
        --channel) CHANNEL=$2; shift 2 ;;
        --staging) STAGING=$2; shift 2 ;;
        --show) SHOW=1; shift ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
TOOL=${ORIENTSTORE_TOOL:-/root/orientstore/werkzeug/store}
[ -f "$TOOL" ] || { echo "no store tool: $TOOL" >&2; exit 2; }
if [ -z "$APK" ]; then
    if [ -n "$VERSION" ]; then APK=$BUILDS/jarvis-$VERSION.apk
    else APK=$(ls -t "$BUILDS"/jarvis-*.apk 2>/dev/null | head -1); fi
fi
[ -f "$APK" ] || { echo "no apk: $APK" >&2; exit 2; }
if [ -z "$VERSION" ]; then VERSION=$(basename "$APK" .apk); VERSION=${VERSION#jarvis-}; fi
[ -f "$NOTES" ] || { echo "no notes file: $NOTES" >&2; exit 2; }

# --- safety: staging only
mkdir -p "$STAGING"
STAGING=$(cd "$STAGING" && pwd -P)
case "$STAGING" in /srv|/srv/*) echo "refused: $STAGING is the live store area" >&2; exit 3 ;; esac
if [ -n "${ORIENTSTORE_SCHLUESSEL:-}" ]; then
    case "$(readlink -f "$ORIENTSTORE_SCHLUESSEL")" in "$STAGING"/*) ;; *) echo "refused: ORIENTSTORE_SCHLUESSEL is outside the staging store" >&2; exit 3 ;; esac
fi
export ORIENTSTORE_REPO=$STAGING
export ORIENTSTORE_SCHLUESSEL=$STAGING/schluessel.geheim

# --- the notes of this version: lines between "## <version>" and the next "## ", blank edges trimmed, at most 1536 bytes
NOTES_TEXT=$(python3 -I - "$NOTES" "$VERSION" <<'PY'
import sys
path, ver = sys.argv[1], sys.argv[2]
lines, grab, out = open(path, encoding="utf-8").read().splitlines(), False, []
for ln in lines:
    if ln.startswith("## "):
        if grab:
            break
        grab = ln[3:].strip() == ver
        continue
    if grab:
        out.append(ln.rstrip())
text = "\n".join(out).strip()
if not text:
    sys.exit("no release notes for version %s in %s" % (ver, path))
b = text.encode("utf-8")
if len(b) > 1536:
    sys.stderr.write("warning: notes cut from %d to 1536 bytes\n" % len(b))
    text = b[:1536].decode("utf-8", "ignore")
print(text)
PY
)

STORE="python3 $TOOL"
if [ ! -f "$STAGING/index.json" ]; then
    $STORE init --name "JARVIS staging store" --adresse "http://10.0.2.2/" >/dev/null
    echo "staging store created in $STAGING"
fi
echo "== add $(basename "$APK") to channel '$CHANNEL' of $STAGING"
$STORE add --kanal "$CHANNEL" --aenderungen "$NOTES_TEXT" "$APK" | sed -n 's/^\(Architektur\|SHA-256\|Katalog\|Ziele\)/  \1/p'
echo "== verify"
VER_OUT=$($STORE verify 2>&1 || true)
echo "$VER_OUT" | sed 's/^/  /'
# A channel other than 'stabil' alone makes `store verify` complain "no channel stabil": expected for a test-only package.
if echo "$VER_OUT" | grep '^FEHLER' | grep -v "kein Kanal 'stabil'" | grep -q .; then
    echo "verify found real errors" >&2; exit 1
fi
echo "== builds and channels"
$STORE builds org.fleitec.jarvis 2>&1 | sed 's/^/  /' | head -20
[ "$SHOW" = 1 ] && printf '== notes of %s\n%s\n' "$VERSION" "$NOTES_TEXT"
echo "trust anchor (public key of the staging store): $($STORE key)"
echo "NOT uploaded anywhere. Staging: $STAGING"

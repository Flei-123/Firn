#!/usr/bin/env bash
# release.sh -- build @NAME@ and publish it to the store.
#
#   bash release.sh [options]
#
#   --platforms linux,windows,android   which builds (default: linux)
#   --channel stabil|beta|...           where the new build goes (default: stabil)
#   --notes "text"  |  --notes-file F   the release notes (shown in the update dialog)
#   --min-version V                     lowest version clients may still accept (mindestFassung)
#   --store-repo DIR                    the store's repository directory (or $ORIENTSTORE_REPO)
#                                       WITHOUT it: a dry run into a throw-away repository
#   --publish-to DEST                   after adding: store publish DEST (rsync/cp, see `store publish --help`)
#   --yes                               do not ask before changing a real store
#
# The signing key of the STORE (schluessel.geheim) never lives in this
# project: it is read from $ORIENTSTORE_SCHLUESSEL or <store-repo>/schluessel.geheim
# by the store tool itself. This script only passes paths.
#
# THE STORE TOOL is orientstore's `store` (add-app). Set ORIENTSTORE_TOOL if it
# is not in the usual place. A dry run needs nothing else: it creates a
# temporary store, publishes into it, verifies it and deletes it.
set -euo pipefail
cd "$(dirname "$0")"
VERSION=$(tr -d ' \n' <VERSION)
PLATFORMS=linux
CHANNEL=stabil
NOTES=""
MINVER=""
REPO=${ORIENTSTORE_REPO:-}
PUBLISH_TO=""
YES=0
while [ $# -gt 0 ]; do
    case "$1" in
        --platforms) PLATFORMS=$2; shift 2 ;;
        --channel) CHANNEL=$2; shift 2 ;;
        --notes) NOTES=$2; shift 2 ;;
        --notes-file) NOTES=$(cat "$2"); shift 2 ;;
        --min-version) MINVER=$2; shift 2 ;;
        --store-repo) REPO=$2; shift 2 ;;
        --publish-to) PUBLISH_TO=$2; shift 2 ;;
        --yes) YES=1; shift ;;
        -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
TOOL=${ORIENTSTORE_TOOL:-}
if [ -z "$TOOL" ]; then
    for c in /root/orientstore/werkzeug/store "$HOME/orientstore/werkzeug/store"; do
        [ -f "$c" ] && grep -q add-app "$c" && TOOL=$c && break
    done
fi
[ -n "$TOOL" ] || { echo "the store tool (orientstore werkzeug/store with add-app) was not found: set ORIENTSTORE_TOOL" >&2; exit 2; }

TMP=""
cleanup() { [ -n "$TMP" ] && rm -rf "$TMP"; }
trap cleanup EXIT
DRY=0
if [ -z "$REPO" ]; then
    DRY=1
    TMP=$(mktemp -d)
    REPO=$TMP/repo
    echo "== DRY RUN: publishing into a throw-away store at $REPO"
    ORIENTSTORE_REPO=$REPO python3 "$TOOL" init --name "dry run" --adresse "http://127.0.0.1/" >/dev/null
elif [ "$YES" -ne 1 ]; then
    echo "About to publish @NAME@ $VERSION ($PLATFORMS, channel $CHANNEL) into the store at:"
    echo "    $REPO"
    printf "Type YES to continue: "
    read -r ans
    [ "$ans" = YES ] || { echo "cancelled"; exit 1; }
fi
export ORIENTSTORE_REPO=$REPO
STORE="python3 $TOOL"

add() {   # add <art> <platform> <file>
    local art=$1 plat=$2 file=$3
    local args=(add-app --art "$art" --id "@ID@" --fassung "$VERSION" --ziel "$plat" --name "@NAME@" --kanal "$CHANNEL")
    [ -n "$NOTES" ] && args+=(--aenderungen "$NOTES")
    [ -n "$MINVER" ] && args+=(--mindest "$MINVER")
    $STORE "${args[@]}" "$file"
}

add_apk() {   # an APK is read by `store add` itself (package name, version code, signing certificate)
    local file=$1
    local args=(add --name "@NAME@" --kanal "$CHANNEL")
    [ -n "$NOTES" ] && args+=(--aenderungen "$NOTES")
    $STORE "${args[@]}" "$file"
}

IFS=, read -ra LIST <<<"$PLATFORMS"
for p in "${LIST[@]}"; do
    case "$p" in
        linux)   bash build.sh;          add bin linux-x86_64 build/@ID@ ;;
        windows) bash build-windows.sh;  add exe windows-x86_64 build/@ID@.exe ;;
        android) bash build-android.sh;  add_apk build/@ID@.apk ;;
        *) echo "unknown platform $p (linux, windows, android)" >&2; exit 2 ;;
    esac
done
$STORE verify
if [ -n "$PUBLISH_TO" ] && [ "$DRY" -eq 0 ]; then
    $STORE publish "$PUBLISH_TO"
fi
echo "released @NAME@ $VERSION to channel $CHANNEL"

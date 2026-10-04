#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/android.sh -- a signed APK of a Firn program, with its launcher icon.
#
#   bash tools/pack/android.sh APP-DIR [--icons DIR] [--out FILE.apk]
#
# APP-DIR is a project made by tools/newapp.sh (src/main.fi, VERSION, pack.ini). The APK
# pipeline itself is tools/android/build.sh (NDK link, aapt2, zipalign, apksigner); this
# script only fills it in from pack.ini and the icons of tools/pack/icons.fi:
#   name/package/version come from pack.ini + VERSION, versionCode = major*10000 + minor*100 + patch,
#   the appkit receiver and permissions are the template's (the same as templates/app/build-android.sh).
# The signing key is ~/.firn/android.keystore (KEYSTORE=...): KEEP IT -- an update only installs
# over an APK signed with the same key.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
APP=${1:?usage: android.sh APP-DIR [--icons DIR] [--out FILE]}; shift
ICONS=""; OUT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --icons) ICONS=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
APP=$(cd "$APP" && pwd)
ini() { sed -n "s/^$1=//p" "$APP/pack.ini" 2>/dev/null | head -1; }
ID=$(ini id); NAME=$(ini name); PKG=$(ini android_id)
[ -n "$ID" ] || { echo "pack.ini has no id" >&2; exit 2; }
[ -n "$NAME" ] || NAME=$ID
[ -n "$PKG" ] || PKG=org.firn.$(printf '%s' "$ID" | tr -d '-')
VERSION=$(tr -d ' \n' <"$APP/VERSION")
IFS=. read -r MA MI PA _ <<<"${VERSION%%-*}"
VCODE=$(( ${MA:-0} * 10000 + ${MI:-0} * 100 + ${PA:-0} ))
[ -n "$OUT" ] || OUT=$APP/build/$ID.apk
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib FIRNC
export FIRN_APP_VERSION=$VERSION FIRN_APP_CHANNEL=${CHANNEL:-stabil}
[ -n "${STORE:-}" ] && export FIRN_APP_STORE=$STORE
mkdir -p "$APP/build"
EXTRA=()
if [ -f "$APP/build-android.sh" ] && grep -q installdex "$APP/build-android.sh"; then
    # an appkit program: the receiver class the updater needs
    "$FIRNC" -o "$APP/build/installdex" "$ROOT/tools/appkit/installdex_main.fi"
    "$APP/build/installdex" "$APP/build/classes.dex" firnapp
    printf '<receiver android:name="org.firn.FirnInstall" android:exported="false"/>\n' >"$APP/build/receiver.xml"
    EXTRA=(--permission android.permission.INTERNET --permission android.permission.REQUEST_INSTALL_PACKAGES
           --permission android.permission.UPDATE_PACKAGES_WITHOUT_USER_ACTION
           --dex "$APP/build/classes.dex" --manifest-extra "$APP/build/receiver.xml")
fi
ICON_ARGS=()
if [ -n "$ICONS" ] && [ -d "$ICONS/android/res" ]; then ICON_ARGS=(--icon-res "$ICONS/android/res"); fi
bash "$ROOT/tools/android/build.sh" "$APP/src/main.fi" --name "$NAME" --package "$PKG" \
    --version-code "$VCODE" --version-name "$VERSION" ${EXTRA[@]+"${EXTRA[@]}"} ${ICON_ARGS[@]+"${ICON_ARGS[@]}"} \
    --out "$OUT"
rm -f "$OUT.idsig"        # apksigner's v4 side file: not needed, not shipped
echo "$OUT"

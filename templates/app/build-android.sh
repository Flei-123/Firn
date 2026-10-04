#!/usr/bin/env bash
# build-android.sh -- build @NAME@ as an Android app (APK, arm64 + x86_64).
#
#   bash build-android.sh    -> build/@ID@.apk
#
# Uses tools/android/build.sh of the Firn checkout (NDK, SDK and the signing
# key as described there; the key is kept in ~/.firn/android.keystore -- an
# update only installs over the same key). The program updates itself through
# Android's PackageInstaller: lib/@android/appkit/platform.fi, and the small
# receiver class that handles the system's answer is written by Firn
# (tools/appkit/installdex_main.fi), no Java.
set -euo pipefail
cd "$(dirname "$0")"
FIRN_ROOT=${FIRN_ROOT:-@FIRN_ROOT@}
FIRNC=${FIRNC:-$FIRN_ROOT/compiler/target/release/firnc}
export FIRNLIB=$FIRN_ROOT/lib
export FIRN_APP_VERSION=$(tr -d ' \n' <VERSION)
export FIRN_APP_CHANNEL=${CHANNEL:-stabil}
[ -n "${STORE:-}" ] && export FIRN_APP_STORE=$STORE
VERSION=$FIRN_APP_VERSION
# the version code: major*10000 + minor*100 + patch (0.5.5 -> 505)
IFS=. read -r MA MI PA _ <<<"${VERSION%%-*}"
VCODE=$(( ${MA:-0} * 10000 + ${MI:-0} * 100 + ${PA:-0} ))
mkdir -p build
"$FIRNC" -o build/installdex "$FIRN_ROOT/tools/appkit/installdex_main.fi"
build/installdex build/classes.dex firnapp
cat >build/receiver.xml <<XML
<receiver android:name="org.firn.FirnInstall" android:exported="false"/>
XML
bash "$FIRN_ROOT/tools/android/build.sh" src/main.fi --name "@NAME@" --package "@ANDROID_ID@" \
    --version-code "$VCODE" --version-name "$VERSION" \
    --permission android.permission.INTERNET \
    --permission android.permission.REQUEST_INSTALL_PACKAGES \
    --dex build/classes.dex --manifest-extra build/receiver.xml \
    --out "$PWD/build/@ID@.apk"
echo "built build/@ID@.apk $VERSION (versionCode $VCODE)"

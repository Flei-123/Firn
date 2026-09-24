#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/build.sh -- ANY Firn program as an Android app (APK).
#
#   bash tools/android/build.sh <entry.fi> [options]
#
# The program is the SAME one that runs under X11: its `main` opens a
# window through lib/window/window.fi. What makes it an Android app:
#
#   1. a shadow of the program (its whole package if it has a firn.package)
#      in which window/backend.fi points to lib/window/android.fi and a
#      vendored platform window layer is left out (tools/android/shadow.py;
#      the backend is chosen by the directory of the root file, see the
#      header of lib/window/window.fi)
#   2. firnc --target=<arch> --pic -c            ->  ELF object
#      (--pic: no TEXTREL, which Android's loader refuses since API 23)
#   3. ld -shared -Bsymbolic, only the entry point exported -> lib<lib>.so
#      (EGL + GLES 3 as needed: lib/window/android.fi draws fUi on the GPU)
#   4. a manifest for android.app.NativeActivity that names
#      `firn_activity_create` (lib/android/activity.fi) as the entry, then
#      aapt2 + zipalign + apksigner                        ->  <name>.apk
#
# No Java, no Kotlin, no Gradle: NativeActivity is part of Android itself.
# Adapted from Certus' tools/android/bau.sh (same author, MPL-2.0 in Firn).
#
# Options:
#   --name <Label>        app label (default: directory name of the entry)
#   --package <id>        application id (default: org.firn.<name>)
#   --lib <name>          name of the .so (default: firnapp)
#   --abi arm64|x86_64|both   (default: both)
#   --version-code <n>    (default: 1)       --version-name <s> (default: 0.1)
#   --opt <level>         (default: release-safe)
#   --assets <dir>        packed uncompressed into assets/
#   --permission <name>   uses-permission, repeatable (android.permission.X)
#   --manifest-extra <f>  XML inserted into <application> (services, ...)
#   --dex <file>          classes.dex to ship (then hasCode="true")
#   --args-file <file>    default arguments (one per line) the app starts
#                         with, shipped as assets/args.txt; a file args.txt
#                         in the app's data directory still wins
#   --push                the program uses lib/plat/android/push.fi: Firn
#                         writes classes.dex with org.firn.FirnService
#                         (tools/android/servicedex_main.fi), the manifest
#                         gets the foreground service (remoteMessaging) and
#                         INTERNET, FOREGROUND_SERVICE(_REMOTE_MESSAGING),
#                         POST_NOTIFICATIONS
#   --out <file.apk>      (default: build/android/<name>/<name>.apk)
#
# Environment: FIRNC, FIRNLIB (default: this tree), NDK, SDK, API (29),
# KEYSTORE (default ~/.firn/android.keystore, created once and KEPT: a new
# key per build would make every APK a different app to Android).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)

usage() { sed -n '3,40p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[ $# -ge 1 ] || usage
ENTRY=$1; shift
[ -f "$ENTRY" ] || { echo "no such entry file: $ENTRY" >&2; exit 2; }
ENTRY_ABS=$(cd "$(dirname "$ENTRY")" && pwd)/$(basename "$ENTRY")
SRCDIR=$(dirname "$ENTRY_ABS")

NAME=$(basename "$SRCDIR"); PKG=""; LIB=firnapp; ABI=both
VCODE=1; VNAME=0.1; OPT=release-safe; ASSETS=""; PERMS=(); EXTRA=""; DEX=""
PUSH=0; ARGSFILE=""
OUT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --name) NAME=$2; shift 2 ;;
        --package) PKG=$2; shift 2 ;;
        --lib) LIB=$2; shift 2 ;;
        --abi) ABI=$2; shift 2 ;;
        --version-code) VCODE=$2; shift 2 ;;
        --version-name) VNAME=$2; shift 2 ;;
        --opt) OPT=$2; shift 2 ;;
        --assets) ASSETS=$2; shift 2 ;;
        --permission) PERMS+=("$2"); shift 2 ;;
        --manifest-extra) EXTRA=$2; shift 2 ;;
        --dex) DEX=$2; shift 2 ;;
        --push) PUSH=1; shift ;;
        --args-file) ARGSFILE=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; usage ;;
    esac
done
SAFE=$(echo "$NAME" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9\n' '_')
[ -n "$PKG" ] || PKG=org.firn.$SAFE

FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
FIRNLIB=${FIRNLIB:-$ROOT/lib}
NDK=${NDK:-/root/android-ndk-min/android-ndk-r27c/toolchains/llvm/prebuilt/linux-x86_64}
SDK=${SDK:-/root/android-sdk}
BT=${BUILDTOOLS:-$SDK/build-tools/35.0.0}
API=${API:-29}
KEYSTORE=${KEYSTORE:-$HOME/.firn/android.keystore}
KSPASS=${KSPASS:-firnstore}
export FIRNLIB

BUILD=$ROOT/build/android/$SAFE
[ -n "$OUT" ] || OUT=$BUILD/$SAFE.apk
rm -rf "$BUILD"; mkdir -p "$BUILD"

# ---- 1. the shadow tree ----------------------------------------------------
# tools/android/shadow.py mirrors the program (its whole package, if it has a
# firn.package) and leaves out every vendored platform window layer.
ROOTFILE=$(python3 "$ROOT/tools/android/shadow.py" "$ENTRY_ABS" "$BUILD/src" \
    "$FIRNLIB/window/android.fi")

# ---- 2./3. compile and link ----------------------------------------------
build_abi() { # $1 abi dir, $2 firn target, $3 linker, $4 NDK triple
    local abi=$1 target=$2 linker=$3 triple=$4
    local out=$BUILD/jni/$abi
    mkdir -p "$out"
    local t0=$(date +%s%N)
    "$FIRNC" --target="$target" --pic --opt-level="$OPT" -c \
        -o "$out/app.o" "$ROOTFILE"
    local t1=$(date +%s%N)
    # The entry point, and the native methods of org.firn.FirnService
    # (resolved by name by ART).
    printf '{ global: firn_activity_create; Java_org_firn_*; local: *; };\n' > "$out/exports.ver"
    local syslib=$NDK/sysroot/usr/lib/$triple/$API
    "$linker" -shared -Bsymbolic --version-script="$out/exports.ver" \
        -z noexecstack -z max-page-size=16384 \
        -soname "lib$LIB.so" -o "$out/lib$LIB.so" "$out/app.o" \
        -L"$syslib" -landroid -llog -lc -lm -ldl \
        --as-needed -lEGL -lGLESv3 --no-as-needed
    if readelf -d "$out/lib$LIB.so" | grep -q TEXTREL; then
        echo "ERROR: $abi: TEXTREL -- Android would refuse the library" >&2
        exit 1
    fi
    if ! readelf --dyn-syms -W "$out/lib$LIB.so" | grep -q ' firn_activity_create$'; then
        echo "ERROR: $abi: firn_activity_create not exported (does the program use lib/window?)" >&2
        exit 1
    fi
    echo "  $abi: compiled in $(( (t1-t0)/1000000 )) ms, lib$LIB.so $(stat -c%s "$out/lib$LIB.so") octets, no TEXTREL"
}
case "$ABI" in arm64|both) build_abi arm64-v8a aarch64-android aarch64-linux-gnu-ld aarch64-linux-android ;; esac
case "$ABI" in x86_64|both) build_abi x86_64 x86_64-android ld x86_64-linux-android ;; esac

# ---- 3b. the push service: a class written by Firn ------------------------
if [ $PUSH -eq 1 ]; then
    SDX=$BUILD/servicedex
    "$FIRNC" -o "$SDX" "$ROOT/tools/android/servicedex_main.fi"
    "$SDX" "$BUILD/classes.dex" "$LIB"
    DEX=$BUILD/classes.dex
    PERMS+=(android.permission.INTERNET android.permission.FOREGROUND_SERVICE
        android.permission.FOREGROUND_SERVICE_REMOTE_MESSAGING
        android.permission.POST_NOTIFICATIONS)
    SVCXML=$BUILD/service.xml
    {
        echo '        <service android:name="org.firn.FirnService" android:exported="false"'
        echo '            android:foregroundServiceType="remoteMessaging" />'
        [ -n "$EXTRA" ] && cat "$EXTRA"
    } > "$SVCXML"
    EXTRA=$SVCXML
    echo "  classes.dex: $(stat -c%s "$DEX") octets (org.firn.FirnService, written by Firn)"
fi

# ---- 4. manifest, package, sign ------------------------------------------
HASCODE=false
[ -n "$DEX" ] && HASCODE=true
MAN=$BUILD/AndroidManifest.xml
{
    cat <<EOF
<?xml version="1.0" encoding="utf-8"?>
<!-- generated by tools/android/build.sh -->
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$PKG" android:versionCode="$VCODE" android:versionName="$VNAME">
    <uses-sdk android:minSdkVersion="$API" android:targetSdkVersion="34" />
EOF
    for p in ${PERMS[@]+"${PERMS[@]}"}; do
        echo "    <uses-permission android:name=\"$p\" />"
    done
    cat <<EOF
    <application android:label="$NAME" android:hasCode="$HASCODE"
        android:extractNativeLibs="true">
        <activity android:name="android.app.NativeActivity"
            android:label="$NAME" android:exported="true"
            android:theme="@android:style/Theme.DeviceDefault.NoActionBar"
            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|screenLayout|uiMode|density">
            <meta-data android:name="android.app.lib_name" android:value="$LIB" />
            <meta-data android:name="android.app.func_name" android:value="firn_activity_create" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
EOF
    [ -n "$EXTRA" ] && cat "$EXTRA"
    echo "    </application>"
    echo "</manifest>"
} > "$MAN"

"$BT/aapt2" link -I "$SDK/platforms/android-35/android.jar" \
    --manifest "$MAN" --min-sdk-version "$API" --target-sdk-version 34 \
    -o "$BUILD/raw.apk"

PACK=$BUILD/pack
mkdir -p "$PACK/lib"
for abi in arm64-v8a x86_64; do
    if [ -f "$BUILD/jni/$abi/lib$LIB.so" ]; then
        mkdir -p "$PACK/lib/$abi"
        cp "$BUILD/jni/$abi/lib$LIB.so" "$PACK/lib/$abi/"
    fi
done
if [ -n "$ASSETS" ]; then
    mkdir -p "$PACK/assets"; cp -r "$ASSETS"/. "$PACK/assets/"
fi
[ -n "$DEX" ] && cp "$DEX" "$PACK/classes.dex"
if [ -n "$ARGSFILE" ]; then
    mkdir -p "$PACK/assets"; cp "$ARGSFILE" "$PACK/assets/args.txt"
fi
# `zip` is not always on the build host, Python is. Assets stay STORED:
# AAssetManager hands out only uncompressed assets in one piece.
python3 - "$BUILD/raw.apk" "$PACK" <<'PY'
import os, sys, zipfile
apk, pack = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(apk, "a", zipfile.ZIP_DEFLATED) as z:
    for top, _, files in os.walk(pack):
        for f in files:
            full = os.path.join(top, f)
            rel = os.path.relpath(full, pack)
            kind = zipfile.ZIP_STORED if rel.startswith("assets/") else zipfile.ZIP_DEFLATED
            z.write(full, rel, compress_type=kind)
PY
"$BT/zipalign" -f -p 4 "$BUILD/raw.apk" "$BUILD/aligned.apk"
mkdir -p "$(dirname "$KEYSTORE")"
if [ ! -f "$KEYSTORE" ]; then
    keytool -genkeypair -keystore "$KEYSTORE" -alias firn \
        -storepass "$KSPASS" -keypass "$KSPASS" -keyalg RSA -keysize 4096 \
        -validity 10950 -dname "CN=Firn, O=fleitec" >/dev/null 2>&1
    echo "  NEW signing key: $KEYSTORE (keep it: updates need the same key)"
fi
mkdir -p "$(dirname "$OUT")"
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass "pass:$KSPASS" \
    --key-pass "pass:$KSPASS" --out "$OUT" "$BUILD/aligned.apk"
rm -f "$BUILD/raw.apk" "$BUILD/aligned.apk"
echo "done: $OUT ($(stat -c%s "$OUT") octets, package $PKG)"

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
#                         (with --push/--pick/--install the launcher activity is org.firn.FirnMain, launchMode singleTop:
#                         onNewIntent keeps the intent of a notification tap while the app is open; an
#                         activity-alias keeps the component name android.app.NativeActivity working)
#   --plain-activity      the launcher is the plain android.app.NativeActivity (no org.firn.FirnMain, no alias, no singleTop) even with
#                         our own dex (bisect builds; a notification tap while the app is open then does not switch the chat)
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
#   --pick                the program uses lib/plat/android/pick.fi (a photo
#                         from the gallery or the camera): the manifest gets
#                         the invisible org.firn.FirnPick activity (same
#                         classes.dex as --push)
#   --install             the program updates itself through appkit (PackageInstaller): the same
#                         classes.dex also holds org.firn.FirnInstall, the manifest names it as a receiver
#                         (push, pick and install can be combined freely)
#   --speech              the program uses lib/plat/android/speech.fi (AAudio microphone): links libaaudio,
#                         the manifest gets RECORD_AUDIO
#   --tts                 the program uses lib/plat/android/tts.fi: the manifest gets <queries> for the TTS
#                         service (Android 11+ hides the engines without it)
#   --camera              the program uses lib/plat/android/camera.fi (NDK Camera2): links
#                         libcamera2ndk + libmediandk, the manifest gets CAMERA
#   --icon-res <dir>      launcher icon resources (tools/pack/icons.fi makes them:
#                         <outdir>/android/res with mipmap-*dpi, mipmap-anydpi-v26
#                         and values/ic_launcher_background.xml): compiled with
#                         aapt2 into the APK, the manifest gets android:icon
#   --out <file.apk>      (default: build/android/<name>/<name>.apk)
#   --release             never create a signing key (same as FIRN_RELEASE=1)
#
# Environment: FIRNC, FIRNLIB (default: this tree), NDK, SDK, API (29),
# KEYSTORE (default ~/.firn/android.keystore, created once and KEPT: a new
# key per build would make every APK a different app to Android).
# Signing (every APK is signed and VERIFIED, see docs/ANDROID.md "Signing"):
#   KSALIAS          key alias in the keystore (default firn)
#   KSPASS           store password; KSPASS_FILE=<file> reads it from the first
#                    line of a file instead (nothing secret goes into the repo)
#   KEYPASS          key password (default: the store password);
#                    KEYPASS_FILE=<file> likewise
#   FIRN_RELEASE=1   (or --release) a MISSING keystore is an error, never a
#                    fresh key: a release APK signed with a new key cannot
#                    update the installed app
#   EXPECT_CERT_SHA256  hex SHA-256 of the signer certificate the APK must
#                    carry (pin); without it the check is against the keystore
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
OWNDEX=0; PUSH=0; PICK=0; INSTALL=0; CAMERA=0; SPEECH=0; TTS=0; ARGSFILE=""; RELEASE=${FIRN_RELEASE:-0}
OUT=""; ICONRES=""
while [ $# -gt 0 ]; do
    case "$1" in
        --name) NAME=$2; shift 2 ;;
        --package) PKG=$2; shift 2 ;;
        --lib) LIB=$2; shift 2 ;;
        --abi) ABI=$2; shift 2 ;;
        --plain-activity) PLAINACT=1; shift ;;
        --version-code) VCODE=$2; shift 2 ;;
        --version-name) VNAME=$2; shift 2 ;;
        --opt) OPT=$2; shift 2 ;;
        --assets) ASSETS=$2; shift 2 ;;
        --permission) PERMS+=("$2"); shift 2 ;;
        --manifest-extra) EXTRA=$2; shift 2 ;;
        --dex) DEX=$2; shift 2 ;;
        --push) PUSH=1; shift ;;
        --pick) PICK=1; shift ;;
        --install) INSTALL=1; shift ;;
        --camera) CAMERA=1; shift ;;
        --speech) SPEECH=1; shift ;;
        --tts) TTS=1; shift ;;
        --args-file) ARGSFILE=$2; shift 2 ;;
        --icon-res) ICONRES=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        --release) RELEASE=1; shift ;;
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
CAMLIBS=""
if [ $CAMERA -eq 1 ]; then CAMLIBS="-lcamera2ndk -lmediandk"; PERMS+=(android.permission.CAMERA); fi
if [ $SPEECH -eq 1 ]; then CAMLIBS="$CAMLIBS -laaudio"; PERMS+=(android.permission.RECORD_AUDIO); fi
KEYSTORE=${KEYSTORE:-$HOME/.firn/android.keystore}
KSALIAS=${KSALIAS:-firn}
if [ -n "${KSPASS_FILE:-}" ]; then
    [ -r "$KSPASS_FILE" ] || { echo "ERROR: KSPASS_FILE not readable: $KSPASS_FILE" >&2; exit 2; }
    KSPASS=$(head -n1 "$KSPASS_FILE")
fi
KSPASS=${KSPASS:-firnstore}
if [ -n "${KEYPASS_FILE:-}" ]; then
    [ -r "$KEYPASS_FILE" ] || { echo "ERROR: KEYPASS_FILE not readable: $KEYPASS_FILE" >&2; exit 2; }
    KEYPASS=$(head -n1 "$KEYPASS_FILE")
fi
KEYPASS=${KEYPASS:-$KSPASS}
# passwords reach keytool/apksigner through the environment (env:NAME), not
# through argv: argv is visible in `ps`
export KSPASS KEYPASS
# fail BEFORE the long compile, not after it
if [ "$RELEASE" = 1 ] && [ ! -f "$KEYSTORE" ]; then
    echo "ERROR: release build but no signing key at $KEYSTORE (restore it from the backup, docs/ANDROID.md)" >&2
    exit 1
fi
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
    # `-z defs`: every symbol must resolve at LINK time. Without it a program that
    # uses lib/plat/android/{speech,camera}.fi but was built without --speech /
    # --camera links "fine" and only dies in dlopen on the phone ("cannot locate
    # symbol AAudioStreamBuilder_setChannelCount").
    if ! "$linker" -shared -Bsymbolic -z defs --version-script="$out/exports.ver" \
        -z noexecstack -z max-page-size=16384 \
        -soname "lib$LIB.so" -o "$out/lib$LIB.so" "$out/app.o" \
        -L"$syslib" -landroid -llog -lc -lm -ldl \
        --as-needed -lEGL -lGLESv3 --no-as-needed $CAMLIBS 2>"$out/link.err"; then
        head -n 12 "$out/link.err" >&2
        echo "ERROR: $abi: link failed (undefined symbols above). A program that uses lib/plat/android/speech.fi needs --speech (libaaudio), camera.fi needs --camera (libcamera2ndk + libmediandk)." >&2
        exit 1
    fi
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
if [ $PUSH -eq 1 ] || [ $PICK -eq 1 ] || [ $INSTALL -eq 1 ]; then
    SDX=$BUILD/servicedex
    "$FIRNC" -o "$SDX" "$ROOT/tools/android/servicedex_main.fi"
    "$SDX" "$BUILD/classes.dex" "$LIB"
    python3 "$ROOT/tools/android/dex_align_check.py" "$BUILD/classes.dex"
    DEX=$BUILD/classes.dex
    OWNDEX=1
    SVCXML=$BUILD/service.xml
    {
        if [ $PUSH -eq 1 ]; then
            echo '        <service android:name="org.firn.FirnService" android:exported="false"'
            echo '            android:foregroundServiceType="remoteMessaging" />'
        fi
        if [ $PICK -eq 1 ]; then
            echo '        <activity android:name="org.firn.FirnPick" android:exported="false"'
            echo '            android:theme="@android:style/Theme.Translucent.NoTitleBar"'
            echo '            android:excludeFromRecents="true"'
            echo '            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|screenLayout|uiMode|density|fontScale" />'
        fi
        if [ $INSTALL -eq 1 ]; then
            echo '        <receiver android:name="org.firn.FirnInstall" android:exported="false" />'
        fi
        [ -n "$EXTRA" ] && cat "$EXTRA"
    } > "$SVCXML"
    EXTRA=$SVCXML
    if [ $PUSH -eq 1 ]; then
        PERMS+=(android.permission.INTERNET android.permission.FOREGROUND_SERVICE
            android.permission.FOREGROUND_SERVICE_REMOTE_MESSAGING
            android.permission.POST_NOTIFICATIONS)
    fi
    echo "  classes.dex: $(stat -c%s "$DEX") octets (org.firn.FirnService, FirnMain, FirnPick, FirnInstall, written by Firn)"
fi

# ---- 4. manifest, package, sign ------------------------------------------
# With our own dex the launcher activity is org.firn.FirnMain (singleTop + onNewIntent: a tap on a notification while
# the app is open reaches the app); without it the plain NativeActivity.
MAINACT=android.app.NativeActivity; LAUNCHMODE=""
if [ $OWNDEX -eq 1 ] && [ "${PLAINACT:-0}" -eq 0 ]; then MAINACT=org.firn.FirnMain; LAUNCHMODE='android:launchMode="singleTop"'; fi
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
    if [ $TTS -eq 1 ]; then
        echo '    <queries><intent><action android:name="android.intent.action.TTS_SERVICE" /></intent></queries>'
    fi
    ICONATTR=""
    [ -n "$ICONRES" ] && ICONATTR='android:icon="@mipmap/ic_launcher" android:roundIcon="@mipmap/ic_launcher_round"'
    cat <<EOF
    <application android:label="$NAME" android:hasCode="$HASCODE" $ICONATTR
        android:extractNativeLibs="true">
        <activity android:name="$MAINACT"
            android:label="$NAME" android:exported="true" $LAUNCHMODE
            android:theme="@android:style/Theme.DeviceDefault.NoActionBar"
            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|screenLayout|uiMode|density|fontScale">
            <meta-data android:name="android.app.lib_name" android:value="$LIB" />
            <meta-data android:name="android.app.func_name" android:value="firn_activity_create" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
EOF
    if [ $OWNDEX -eq 1 ] && [ "${PLAINACT:-0}" -eq 0 ]; then
        # the old component name keeps working (am start -n pkg/android.app.NativeActivity)
        # (NativeActivity reads lib_name/func_name from the component of the START intent: the alias, so it carries them too)
        echo '        <activity-alias android:name="android.app.NativeActivity" android:targetActivity="org.firn.FirnMain" android:exported="true">'
        echo "            <meta-data android:name=\"android.app.lib_name\" android:value=\"$LIB\" />"
        echo '            <meta-data android:name="android.app.func_name" android:value="firn_activity_create" />'
        echo '        </activity-alias>'
    fi
    [ -n "$EXTRA" ] && cat "$EXTRA"
    echo "    </application>"
    echo "</manifest>"
} > "$MAN"

RESARGS=()
if [ -n "$ICONRES" ]; then
    "$BT/aapt2" compile --dir "$ICONRES" -o "$BUILD/res.zip"
    RESARGS=("$BUILD/res.zip")
fi
"$BT/aapt2" link -I "$SDK/platforms/android-35/android.jar" \
    --manifest "$MAN" --min-sdk-version "$API" --target-sdk-version 34 \
    ${RESARGS[@]+"${RESARGS[@]}"} -o "$BUILD/raw.apk"

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
if [ ! -f "$KEYSTORE" ]; then
    if [ "$RELEASE" = 1 ]; then
        echo "ERROR: release build but no signing key at $KEYSTORE (restore it from the backup, docs/ANDROID.md)" >&2
        exit 1
    fi
    mkdir -p "$(dirname "$KEYSTORE")"
    keytool -genkeypair -keystore "$KEYSTORE" -alias "$KSALIAS" \
        -storepass:env KSPASS -keypass:env KEYPASS -keyalg RSA -keysize 4096 \
        -validity 10950 -dname "CN=Firn, O=fleitec" >/dev/null 2>&1
    chmod 600 "$KEYSTORE"
    echo "  NEW signing key: $KEYSTORE (keep it: updates need the same key)"
fi
mkdir -p "$(dirname "$OUT")"
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-key-alias "$KSALIAS" \
    --ks-pass env:KSPASS --key-pass env:KEYPASS --out "$OUT" "$BUILD/aligned.apk"
rm -f "$BUILD/raw.apk" "$BUILD/aligned.apk"

# ---- 5. verify the signature -------------------------------------------------
# The APK must verify (v2 or v3 scheme) and carry the certificate of the
# keystore (or EXPECT_CERT_SHA256): a build that silently used another key
# would be an app that cannot update the installed one.
VERIFY=$("$BT/apksigner" verify --verbose --print-certs "$OUT" 2>&1) || {
    echo "$VERIFY" >&2; echo "ERROR: apksigner verify failed for $OUT" >&2; exit 1; }
echo "$VERIFY" | grep -Eq 'scheme \(APK Signature Scheme v[23]\): true' || {
    echo "$VERIFY" >&2; echo "ERROR: $OUT has neither a v2 nor a v3 signature" >&2; exit 1; }
APK_CERT=$(echo "$VERIFY" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | head -n1 | tr 'A-F' 'a-f')
WANT_CERT=${EXPECT_CERT_SHA256:-}
if [ -z "$WANT_CERT" ]; then
    WANT_CERT=$(keytool -list -keystore "$KEYSTORE" -alias "$KSALIAS" -storepass:env KSPASS 2>/dev/null \
        | sed -n 's/.*SHA-256): //p' | head -n1)
fi
WANT_CERT=$(echo "$WANT_CERT" | tr -d ':' | tr 'A-F' 'a-f')
if [ -z "$APK_CERT" ] || [ -z "$WANT_CERT" ] || [ "$APK_CERT" != "$WANT_CERT" ]; then
    echo "ERROR: signer certificate mismatch (apk '$APK_CERT', expected '$WANT_CERT')" >&2
    exit 1
fi
echo "  signed + verified: cert SHA-256 $APK_CERT"
echo "done: $OUT ($(stat -c%s "$OUT") octets, package $PKG)"

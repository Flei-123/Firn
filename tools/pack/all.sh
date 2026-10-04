#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/all.sh -- every package of one release of a Firn program.
#
#   bash tools/pack/all.sh APP-DIR VERSION [options]
#
# APP-DIR is a project made by tools/newapp.sh (VERSION, pack.ini, assets/icon.svg,
# build.sh, build-windows.sh, build-android.sh). Output: APP-DIR/dist/VERSION/
#
#   linux     ID-VERSION-linux-x86_64.deb        .deb (dpkg -i)
#             ID-VERSION-linux-x86_64.tar.gz     tarball with install.sh / uninstall.sh
#             ID-VERSION-x86_64.AppImage         a real AppImage (runtime + squashfs), when a runtime
#                                                is there (downloaded once, or --runtime FILE) ...
#             ID-VERSION-x86_64.run              ... and always the self-extracting program
#   windows   ID-VERSION-windows-x86_64.exe      the program with its icon (this is what the updater serves)
#             ID-VERSION-setup.exe               the installer (Start Menu, Desktop, "Apps & features", uninstaller)
#             ID-VERSION-windows-portable.zip    the portable zip
#             nsis/ID.nsi                        the same install as an NSIS script (built if makensis is there)
#   mac       ID-VERSION-macos.zip, .dmg, sign-and-notarize.sh     only with --mac-binary FILE (a Mach-O program;
#                                                Firn has no macOS target yet). Nothing is run on a Mac.
#   android   ID-VERSION-android.apk             signed APK with launcher icons (needs the Android SDK/NDK)
#   osum      ID-VERSION-osum.opk                OrientOS store package, only with --osum-start FILE
#   icons/    every icon size, .ico, .icns, Android mipmaps
#   manifest.json   one fragment per artifact: art, platform, version, size, SHA-256, file and an Ed25519
#                   signature (key file: --sign-key FILE or $PACK_SIGN_KEY / $ORIENTSTORE_SCHLUESSEL)
#   store-add.sh    the `store add-app` commands that publish the store artifacts (NOT run; the live store
#                   is only touched by hand)
#
# Options:
#   --platforms LIST   linux,windows,mac,android,osum or `all` (default: linux,windows)
#   --out DIR          output directory (default APP-DIR/dist/VERSION)
#   --no-build         use what is in APP-DIR/build (the programs must be there)
#   --mac-binary F  --mac-arch arm64|x86_64   --osum-start F   --runtime F   --sign-key F
#   --notes TEXT       release notes for the store-add.sh commands     --channel NAME (default stabil)
#
# Each platform runs on its own: one that fails or cannot run here (no mingw, no SDK) is reported in the
# summary and the others still finish; the exit code is 1 if any listed platform failed.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
[ $# -ge 2 ] || { sed -n '3,40p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
APP=$(cd "$1" && pwd); VERSION=$2; shift 2
PLATFORMS=linux,windows; OUT=""; NOBUILD=0; MACBIN=""; MACARCH=arm64; OSUMSTART=""; RUNTIME=""
SIGNKEY=""; NOTES=""; CHANNEL=stabil
while [ $# -gt 0 ]; do
    case "$1" in
        --platforms) PLATFORMS=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        --no-build) NOBUILD=1; shift ;;
        --mac-binary) MACBIN=$2; shift 2 ;;
        --mac-arch) MACARCH=$2; shift 2 ;;
        --osum-start) OSUMSTART=$2; shift 2 ;;
        --runtime) RUNTIME=$2; shift 2 ;;
        --sign-key) SIGNKEY=$2; shift 2 ;;
        --notes) NOTES=$2; shift 2 ;;
        --channel) CHANNEL=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
[ "$PLATFORMS" = all ] && PLATFORMS=linux,windows,mac,android,osum
[ -f "$APP/pack.ini" ] || { echo "$APP/pack.ini is missing (a project from tools/newapp.sh has one)" >&2; exit 2; }
ini() { sed -n "s/^$1=//p" "$APP/pack.ini" | head -1; }
ID=$(ini id); NAME=$(ini name)
[ -n "$ID" ] && [ -n "$NAME" ] || { echo "pack.ini needs id= and name=" >&2; exit 2; }
if [ "$(tr -d ' \n' <"$APP/VERSION" 2>/dev/null)" != "$VERSION" ]; then
    printf '%s\n' "$VERSION" >"$APP/VERSION"
    echo "note: $APP/VERSION is now $VERSION"
fi
[ -n "$OUT" ] || OUT=$APP/dist/$VERSION
rm -rf "$OUT"; mkdir -p "$OUT"
PACK="bash $HERE/pack.sh"
APPARGS=(--ini "$APP/pack.ini" --version "$VERSION")
ARTS=(); SUMMARY=(); FAILS=0
has() { case ",$PLATFORMS," in *",$1,"*) return 0 ;; esac; return 1; }
say() { SUMMARY+=("$1"); }
art() { ARTS+=(--artifact "$1"); }      # art:platform:file[:store]

# ---- the Firn parts (icons tool, stubs)
$PACK stubs >/dev/null || { echo "the stubs did not build" >&2; exit 1; }
BUILD=${PACK_BUILD:-$ROOT/build/pack}

# ---- icons
ICONSRC=$(ini icon); [ -n "$ICONSRC" ] || ICONSRC=assets/icon.svg
ICONBG=$(ini icon_bg); [ -n "$ICONBG" ] || ICONBG=FFFFFF
if [ -f "$APP/$ICONSRC" ]; then
    "$BUILD/icons" "$APP/$ICONSRC" "$OUT/icons" --name "$ID" --bg "$ICONBG" >/dev/null && say "icons       $OUT/icons" \
        || { say "icons       FAILED"; FAILS=$((FAILS+1)); }
else
    say "icons       none ($APP/$ICONSRC missing): packages get no icon"
fi
ICONS=$OUT/icons; [ -d "$ICONS" ] || ICONS=""
ICOFILE=""; [ -f "$ICONS/$ID.ico" ] && ICOFILE=$ICONS/$ID.ico
ICNSFILE=""; [ -f "$ICONS/$ID.icns" ] && ICNSFILE=$ICONS/$ID.icns
ICONARG=(); [ -n "$ICONS" ] && ICONARG=(--icons "$ICONS")

# ---- linux
if has linux; then
    ( set -e
      if [ $NOBUILD -eq 0 ]; then bash "$APP/build.sh" >/dev/null; fi
      EXE=$APP/build/$ID
      [ -f "$EXE" ] || { echo "no $EXE" >&2; exit 1; }
      ARCH=x86_64
      $PACK deb "${APPARGS[@]}" --exe "$EXE" --out "$OUT/$ID-$VERSION-linux-$ARCH.deb" "${ICONARG[@]}" \
          --depends "$(ini depends)" >/dev/null
      $PACK tar "${APPARGS[@]}" --exe "$EXE" --out "$OUT/$ID-$VERSION-linux-$ARCH.tar.gz" "${ICONARG[@]}" >/dev/null
      $PACK selfextract "${APPARGS[@]}" --stub "$BUILD/selfx" --exe "$EXE" --out "$OUT/$ID-$VERSION-$ARCH.run" \
          "${ICONARG[@]}" >/dev/null
      cp "$EXE" "$OUT/$ID-$VERSION-linux-$ARCH"
    ) && {
        EXE=$APP/build/$ID
        art "bin:linux-x86_64:$OUT/$ID-$VERSION-linux-x86_64:store"
        art "deb:linux-x86_64:$OUT/$ID-$VERSION-linux-x86_64.deb"
        art "tar:linux-x86_64:$OUT/$ID-$VERSION-linux-x86_64.tar.gz"
        RT=(); [ -n "$RUNTIME" ] && RT=(--runtime "$RUNTIME")
        if $PACK appimage "${APPARGS[@]}" --exe "$EXE" --out "$OUT/$ID-$VERSION-x86_64.AppImage" "${ICONARG[@]}" \
                ${RT[@]+"${RT[@]}"} >/dev/null 2>"$OUT/appimage.log"; then
            art "appimage:linux-x86_64:$OUT/$ID-$VERSION-x86_64.AppImage:store"
            art "selfextract:linux-x86_64:$OUT/$ID-$VERSION-x86_64.run"
            say "linux       deb, tar.gz, AppImage (real), .run (self-extracting), raw program"
        else
            art "appimage:linux-x86_64:$OUT/$ID-$VERSION-x86_64.run:store"
            say "linux       deb, tar.gz, .run (self-extracting; served to the updater as the 'appimage' art), raw program -- no real AppImage: $(tail -1 "$OUT/appimage.log")"
        fi
        rm -f "$OUT/appimage.log"
    } || { say "linux       FAILED"; FAILS=$((FAILS+1)); }
fi

# ---- windows
if has windows; then
    ( set -e
      if [ $NOBUILD -eq 0 ]; then bash "$APP/build-windows.sh" >/dev/null; fi
      EXE=$APP/build/$ID.exe
      [ -f "$EXE" ] || { echo "no $EXE" >&2; exit 1; }
      STUB=$BUILD/setup-stub.exe
      [ -f "$STUB" ] || { echo "no installer stub (mingw binutils missing?)" >&2; exit 1; }
      PATCHED=$OUT/$ID-$VERSION-windows-x86_64.exe
      if [ -n "$ICOFILE" ]; then
          $PACK pe-icon --exe "$EXE" --ico "$ICOFILE" --out "$PATCHED" >/dev/null
          # the installer carries its own icon too (the stub is patched before the payload goes on)
          $PACK pe-icon --exe "$STUB" --ico "$ICOFILE" --out "$OUT/.stub-icon.exe" >/dev/null
          STUB=$OUT/.stub-icon.exe
      else
          cp "$EXE" "$PATCHED"
      fi
      ICOARG=(); [ -n "$ICOFILE" ] && ICOARG=(--ico "$ICOFILE")
      $PACK win-installer "${APPARGS[@]}" --exe-name "$ID.exe" --stub "$STUB" --exe "$PATCHED" \
          --out "$OUT/$ID-$VERSION-setup.exe" ${ICOARG[@]+"${ICOARG[@]}"} >/dev/null
      $PACK win-zip "${APPARGS[@]}" --exe-name "$ID.exe" --exe "$PATCHED" --out "$OUT/$ID-$VERSION-windows-portable.zip" \
          ${ICOARG[@]+"${ICOARG[@]}"} >/dev/null
      $PACK win-nsis "${APPARGS[@]}" --exe-name "$ID.exe" --exe "$PATCHED" --outdir "$OUT/nsis" --build \
          ${ICOARG[@]+"${ICOARG[@]}"} >/dev/null
      mv "$OUT/nsis/$(basename "$PATCHED")" "$OUT/nsis/$ID.exe" 2>/dev/null || true
      rm -f "$OUT/.stub-icon.exe"
    ) && {
        art "exe:windows-x86_64:$OUT/$ID-$VERSION-windows-x86_64.exe:store"
        art "installer:windows-x86_64:$OUT/$ID-$VERSION-setup.exe"
        art "zip:windows-x86_64:$OUT/$ID-$VERSION-windows-portable.zip"
        say "windows     program with icon, setup.exe, portable zip, NSIS script"
    } || { say "windows     FAILED"; FAILS=$((FAILS+1)); }
fi

# ---- macOS
if has mac; then
    if [ -z "$MACBIN" ]; then
        say "mac         skipped: no --mac-binary (Firn has no macOS target; give a Mach-O program)"
    else
        ( set -e
          ICNSARG=(); [ -n "$ICNSFILE" ] && ICNSARG=(--icns "$ICNSFILE")
          mkdir -p "$OUT/macos"
          $PACK mac-app "${APPARGS[@]}" --exe "$MACBIN" --outdir "$OUT/macos" ${ICNSARG[@]+"${ICNSARG[@]}"} >/dev/null
          mv "$OUT/macos/$ID-$VERSION-macos.zip" "$OUT/$ID-$VERSION-macos-$MACARCH.zip"
          $PACK mac-dmg "${APPARGS[@]}" --exe "$MACBIN" --out "$OUT/$ID-$VERSION-macos-$MACARCH.dmg" \
              ${ICNSARG[@]+"${ICNSARG[@]}"} >/dev/null || echo "no .dmg (xorriso?)" >&2
          $PACK mac-sign-script "${APPARGS[@]}" --out "$OUT/sign-and-notarize.sh" >/dev/null
        ) && {
            art "macos-app:macos-$MACARCH:$OUT/$ID-$VERSION-macos-$MACARCH.zip:store"
            [ -f "$OUT/$ID-$VERSION-macos-$MACARCH.dmg" ] && art "dmg:macos-$MACARCH:$OUT/$ID-$VERSION-macos-$MACARCH.dmg"
            say "mac         .app (macos/), zip, dmg, sign-and-notarize.sh -- UNSIGNED, UNTESTED on a Mac"
        } || { say "mac         FAILED"; FAILS=$((FAILS+1)); }
    fi
fi

# ---- android
if has android; then
    ( set -e
      IC=(); [ -n "$ICONS" ] && IC=(--icons "$ICONS")
      bash "$HERE/android.sh" "$APP" ${IC[@]+"${IC[@]}"} --out "$OUT/$ID-$VERSION-android.apk" >/dev/null
    ) && { art "apk:android:$OUT/$ID-$VERSION-android.apk:store"; say "android     signed APK (arm64-v8a, x86_64)"; } \
      || { say "android     FAILED (or no SDK/NDK here)"; FAILS=$((FAILS+1)); }
fi

# ---- OrientOS
if has osum; then
    if [ -z "$OSUMSTART" ]; then
        say "osum        skipped: no --osum-start (an ELF program built for OrientOS)"
    else
        ( set -e
          PNG=""; [ -f "$ICONS/png/$ID-64.png" ] && PNG=$ICONS/png/$ID-64.png
          PA=(); [ -n "$PNG" ] && PA=(--icon-png "$PNG")
          $PACK opk "${APPARGS[@]}" --exe "$OSUMSTART" --out "$OUT/$ID-$VERSION-osum.opk" ${PA[@]+"${PA[@]}"} >/dev/null 2>&1
        ) && { art "opk:osum:$OUT/$ID-$VERSION-osum.opk:store"; say "osum        .opk (untested on OrientOS)"; } \
          || { say "osum        FAILED"; FAILS=$((FAILS+1)); }
    fi
fi

# ---- the manifest
KEYARG=(); [ -n "$SIGNKEY" ] && KEYARG=(--key "$SIGNKEY")
if [ ${#ARTS[@]} -gt 0 ]; then
    $PACK manifest "${APPARGS[@]}" --dir "$OUT" "${ARTS[@]}" ${KEYARG[@]+"${KEYARG[@]}"} \
        --notes "$NOTES" --channel "$CHANNEL" >/dev/null && say "manifest    $OUT/manifest.json, store-add.sh" \
        || { say "manifest    FAILED"; FAILS=$((FAILS+1)); }
fi
echo "== $NAME $VERSION -> $OUT"
for l in "${SUMMARY[@]}"; do echo "   $l"; done
ls -la "$OUT" | sed 's/^/   /' | tail -n +4
[ $FAILS -eq 0 ]

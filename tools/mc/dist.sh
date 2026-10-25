#!/usr/bin/env bash
# tools/mc/dist.sh -- Windows distribution of the Voxel client: cross-compile (no Windows needed), synthesise the sound
# bank, assemble one folder, zip it, and (if the mingw binutils exist) build setup.exe with Start-menu shortcut + uninstaller.
#
#   bash tools/mc/dist.sh [--no-installer]
#
# Env: FIRNC (default compiler/target/release/firnc), DIST (default /root/projects/minecraft-neu/dist, never committed),
#      VERSION (default 0.1.0), OPT (default release-fast).
# Result in $DIST:
#   mcplay-win/            mcplay.exe (GUI subsystem, log in %LOCALAPPDATA%\mcplay\mcplay.log)
#                          mcplay-console.exe (same program with a console), mcclient.exe (headless), sounds/ (bank), README.txt
#   mcplay-win.zip         portable package (pack.py win-zip)
#   mcplay-setup.exe       installer (Start menu, uninstall.exe), only when the Windows stub can be built
# The sound bank is found by mcplay at <exe dir>/sounds (MCPLAY_SOUNDS overrides).
# Needs the compiler, python3; heavy build -> start it via `/root/jarvis/bin/heavy bash tools/mc/dist.sh`.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
FIRNC=$(readlink -f "$FIRNC"); export FIRNC
export FIRNLIB="$ROOT/lib"
DIST=${DIST:-/root/projects/minecraft-neu/dist}
VERSION=${VERSION:-0.1.0}
OPT=${OPT:-release-fast}
INSTALLER=1; [ "${1:-}" = "--no-installer" ] && INSTALLER=0
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
PKG="$DIST/mcplay-win"
mkdir -p "$DIST"; rm -rf "$PKG"; mkdir -p "$PKG/sounds"
fail() { echo "FAIL: $1"; echo "RESULT mc-dist: FAIL"; exit 1; }

# 1) executables
"$FIRNC" --target=x86_64-windows --win-subsystem=windows --opt-level="$OPT" -o "$PKG/mcplay.exe" demos/mcplay/main.fi 2>"$W/e1" || { head -10 "$W/e1"; fail "mcplay.exe"; }
"$FIRNC" --target=x86_64-windows --opt-level="$OPT" -o "$PKG/mcplay-console.exe" demos/mcplay/main.fi 2>"$W/e2" || { head -10 "$W/e2"; fail "mcplay-console.exe"; }
"$FIRNC" --target=x86_64-windows --opt-level="$OPT" -o "$PKG/mcclient.exe" demos/mcclient/main.fi 2>"$W/e3" || { head -10 "$W/e3"; fail "mcclient.exe"; }
for f in mcplay mcplay-console mcclient; do head -c 2 "$PKG/$f.exe" | grep -q MZ || fail "$f.exe is not a PE file"; done
echo "  ok  exes: $(stat -c %s "$PKG/mcplay.exe") / $(stat -c %s "$PKG/mcplay-console.exe") / $(stat -c %s "$PKG/mcclient.exe") B"

# 2) sound bank (deterministic synthesis, tools/sounds/gen)
"$FIRNC" -o "$W/gen" tools/sounds/gen/main.fi 2>"$W/e4" || { head -10 "$W/e4"; fail "sound generator"; }
"$W/gen" tools/sounds/mc.json "$PKG/sounds" >/dev/null || fail "sound bank generation"
[ -s "$PKG/sounds/bank.json" ] || fail "no bank.json"
echo "  ok  sounds: $(ls "$PKG/sounds" | wc -l) files, $(du -sk "$PKG/sounds" | cut -f1) KiB"

# 3) readme
cat > "$PKG/README.txt" <<EOF
Voxel client $VERSION (Windows)
  mcplay.exe                               start screen: host, port, name (saved in %LOCALAPPDATA%\\mcplay\\mcplay.cfg), Return joins, Esc quits
  mcplay.exe <host> <port> <name>          play directly (no console window; log: %LOCALAPPDATA%\\mcplay\\mcplay.log)
  mcplay-console.exe ...                   the same with a console (log on screen)
  mcclient.exe <host> <port> <name> ...    headless join / chunk dump
Environment: MCPLAY_GL=1 (OpenGL), MCPLAY_VIEW=<chunks> (above 8: 8 chunks full detail + LOD up to the view, MCPLAY_LOD=0 turns LOD off), MCPLAY_LOG=<file|->, MCPLAY_SOUNDS=<dir>, FIRN_AUDIO=device|null
Sound bank: sounds\\ next to the exe is used automatically.
Textures and models: read at run time from your own Minecraft installation (versions\\<version>\\<version>.jar, or MC_VANILLA_JAR), cached in %LOCALAPPDATA%\\mcplay. No Mojang files are shipped. MC_VANILLA=0 turns it off.
EOF

if find "$PKG" \( -iname '*.jar' -o -name blockstates -o -name textures -o -name pack.mcmeta -o -iname '*.ogg' \) | grep -q .; then fail "package contains Mojang-style assets"; fi
echo "  ok  no Mojang assets in the package"

# 4) zip + installer
python3 -I tools/pack/pack.py win-zip --id mcplay --name "Voxel Client" --version "$VERSION" --vendor Fleitec --exe-name mcplay.exe \
    --exe "$PKG/mcplay.exe" --files "$PKG" --out "$DIST/mcplay-win.zip" >"$W/zip.out" 2>&1 || { tail -5 "$W/zip.out"; fail "zip"; }
echo "  ok  zip: $DIST/mcplay-win.zip ($(stat -c %s "$DIST/mcplay-win.zip") B)"
if [ $INSTALLER = 1 ]; then
    if command -v x86_64-w64-mingw32-as >/dev/null && PACK_BUILD="$W/pack" bash tools/pack/pack.sh stubs >"$W/stubs.out" 2>&1 && [ -f "$W/pack/setup-stub.exe" ]; then
        python3 -I tools/pack/pack.py win-installer --id mcplay --name "Voxel Client" --version "$VERSION" --vendor Fleitec --exe-name mcplay.exe \
            --stub "$W/pack/setup-stub.exe" --exe "$PKG/mcplay.exe" --files "$PKG" --out "$DIST/mcplay-setup.exe" >"$W/inst.out" 2>&1 \
            && echo "  ok  installer: $DIST/mcplay-setup.exe ($(stat -c %s "$DIST/mcplay-setup.exe") B)" \
            || { tail -5 "$W/inst.out"; fail "installer"; }
    else
        echo "  skip installer (stub not buildable: $(tail -1 "$W/stubs.out" 2>/dev/null))"
    fi
fi
echo "RESULT mc-dist: ok"

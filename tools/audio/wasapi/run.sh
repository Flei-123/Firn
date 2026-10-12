#!/usr/bin/env bash
# tools/audio/wasapi/run.sh -- audio.wasapi (WASAPI shared mode) under Wine.
#
# Wine's ALSA driver is pointed at ALSA's `file` plugin, so everything WASAPI hands to the sound system lands
# in a raw file (no sound card, no sound server). Checks: endpoint list, Initialize/GetBuffer/ReleaseBuffer path,
# the delivered PCM equals the generated pattern bit for bit (blocks of 1152 / 4410 / 77 frames), the same
# through audio.sink with FIRN_WIN_AUDIO=wasapi, waveOut unchanged, and "no device" fails cleanly.
# REAL WINDOWS: UNTESTED (docs/AUDIO.md, "WASAPI").
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
if ! command -v wine >/dev/null || ! command -v x86_64-w64-mingw32-ld >/dev/null; then
    echo "SKIP wine or the mingw linker is missing"
    exit 0
fi
W=$(mktemp -d)
export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-firn}" WINEDEBUG=-all
trap 'wineserver -k >/dev/null 2>&1 || true; rm -rf "$W"' EXIT
FIRNLIB="$(pwd)/lib" "$FIRNC" --target=x86_64-windows -o "$W/probe.exe" tools/audio/wasapi/main.fi
FIRNLIB="$(pwd)/lib" "$FIRNC" --target=x86_64-windows -o "$W/sink.exe" tools/desktop/sink_main.fi
export W
if [ -z "${DISPLAY:-}" ] && command -v xvfb-run >/dev/null; then
    exec xvfb-run -a python3 -I tools/audio/wasapi/check.py "$W"
fi
exec python3 -I tools/audio/wasapi/check.py "$W"

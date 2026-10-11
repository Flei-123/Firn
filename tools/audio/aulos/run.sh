#!/usr/bin/env bash
# tools/audio/aulos/run.sh -- audio.engine against the real Aulos library, same scenario, offline.
#   AULOS_DIR=/root/projects/aulos  FIRNC=compiler/target/release/firnc  bash tools/audio/aulos/run.sh
# Builds Aulos (g++, read only access to the Aulos repo) into a temp dir, renders every scenario
# in tools/audio/aulos/scenarios with Aulos and with Firn, compares peak/RMS/max deviation.
set -euo pipefail
cd "$(dirname "$0")/../../.."
AULOS_DIR=${AULOS_DIR:-/root/projects/aulos}
FIRNC=${FIRNC:-compiler/target/release/firnc}
if [ ! -f "$AULOS_DIR/src/aulos.cpp" ]; then echo "SKIP: no Aulos checkout at $AULOS_DIR"; exit 0; fi
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
D=tools/audio/aulos
# -ffp-contract=off: plain f32 mul/add like Firn (no fused multiply-add)
cc -O2 -ffp-contract=off -I"$AULOS_DIR/extern" -c "$AULOS_DIR/src/miniaudio_impl.c" -o "$W/ma.o"
c++ -std=c++17 -O2 -ffp-contract=off -DAULOS_NO_DEVICE -I"$AULOS_DIR/include" -I"$AULOS_DIR/extern" -I"$AULOS_DIR/src" \
    -c "$AULOS_DIR/src/aulos.cpp" -o "$W/aulos.o"
c++ -std=c++17 -O2 -I"$AULOS_DIR/include" "$D/aulos_offline.cpp" "$W/aulos.o" "$W/ma.o" -o "$W/aulos_offline" -lpthread -lm -ldl
# asset dir = Aulos assets + files generated here (MP3 via ffmpeg, 44.1 kHz WAV for the resampler)
mkdir -p "$W/assets"
cp "$AULOS_DIR"/assets/*.wav "$W/assets/"
if command -v ffmpeg >/dev/null; then
  ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=1.5" -ac 1 -b:a 128k "$W/assets/tone.mp3"
  ffmpeg -v error -f lavfi -i "sine=frequency=330:sample_rate=44100:duration=1.0" -ac 1 -c:a pcm_s16le "$W/assets/tone44.wav"
fi
fail=0
for sc in "$D"/scenarios/*.txt; do
  n=$(basename "$sc" .txt)
  need=$(grep -o '^# needs [a-z0-9.]*' "$sc" | awk '{print $3}' || true)
  if [ -n "$need" ] && [ ! -f "$W/assets/$need" ]; then echo "SKIP $n: no $need (ffmpeg missing)"; continue; fi
  tol=$(grep -o '^# tol [0-9.e-]*' "$sc" | awk '{print $3}' || true)
  "$W/aulos_offline" "$sc" "$W/assets" "$W/$n.aulos.f32" | sed "s/^/aulos $n: /"
  python3 "$D/gen_firn.py" "$sc" "$W/assets" "$W/$n.firn.f32" > "$W/$n.fi"
  FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$W/$n.bin" "$W/$n.fi"
  "$W/$n.bin" | sed "s/^/firn  $n: /"
  python3 "$D/compare.py" "$W/$n.aulos.f32" "$W/$n.firn.f32" "$n" ${tol:+--max-abs $tol} || fail=1
done
exit $fail

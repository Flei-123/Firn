#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-scene3d.XXXXXX")
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
command -v ffmpeg >/dev/null && command -v ffprobe >/dev/null || { echo "ffmpeg/ffprobe missing"; exit 1; }

"$FIRNC" -o "$W/mini" tools/scene3d/mini.fi > "$W/build.log" 2>&1 || { cat "$W/build.log" | head -20; report "mini build"; exit 1; }

"$W/mini" --out "$W/a.mp4" --fps 10 --duration 1 --width 160 --height 90 --ss 1 --threads 2 > "$W/run.log" 2>&1 || report "video run: $(head -3 "$W/run.log")"
probe() { ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=codec_name,width,height,nb_read_frames,pix_fmt -of default=nw=1 "$1" 2>&1; }
P=$(probe "$W/a.mp4")
echo "$P" | grep -q '^codec_name=h264$' || report "codec: $P"
echo "$P" | grep -q '^width=160$' || report "width: $P"
echo "$P" | grep -q '^height=90$' || report "height: $P"
echo "$P" | grep -q '^nb_read_frames=10$' || report "frames: $P"
echo "$P" | grep -q '^pix_fmt=yuv420p$' || report "pix_fmt: $P"

"$W/mini" --out "$W/b.mp4" --fps 10 --duration 1 --width 160 --height 90 --ss 1 --threads 2 --from 2 --to 5 > "$W/run2.log" 2>&1 || report "range run"
probe "$W/b.mp4" | grep -q '^nb_read_frames=3$' || report "range frames: $(probe "$W/b.mp4")"

"$W/mini" --out "$W/s.png" --still 0.5 --width 160 --height 90 --ss 1 --threads 2 > "$W/run3.log" 2>&1 || report "still run"
python3 -I - "$W/s.png" <<'PY' || report "still png content"
import sys, zlib, struct
d = open(sys.argv[1], 'rb').read()
assert d[:8] == b'\x89PNG\r\n\x1a\n'
w, h = struct.unpack('>II', d[16:24])
assert (w, h) == (160, 90), (w, h)
i = 8; idat = b''
while i < len(d):
    n, = struct.unpack('>I', d[i:i+4]); tag = d[i+4:i+8]
    if tag == b'IDAT': idat += d[i+8:i+8+n]
    i += 12 + n
raw = zlib.decompress(idat)
assert len(set(raw[1:])) > 8, 'flat image'
PY

"$W/mini" --out "$W/c.png" --still 0.5 --width 160 --height 90 --ss 1 --threads 2 > /dev/null 2>&1
"$W/mini" --out "$W/d.png" --still 0.5 --width 160 --height 90 --ss 1 --threads 1 > /dev/null 2>&1
cmp -s "$W/c.png" "$W/d.png" || report "still differs between 1 and 2 threads"
"$W/mini" --out "$W/e.png" --still 0.5 --width 160 --height 90 --ss 1 --threads 2 > /dev/null 2>&1
cmp -s "$W/c.png" "$W/e.png" || report "still not deterministic"
"$W/mini" --out "$W/f.png" --still 0.9 --width 160 --height 90 --ss 1 --threads 2 > /dev/null 2>&1
cmp -s "$W/c.png" "$W/f.png" && report "still ignores time"

if [ "$ERRORS" -eq 0 ]; then echo "scene3d: all checks passed"; exit 0; fi
echo "scene3d: $ERRORS failure(s)"
exit 1

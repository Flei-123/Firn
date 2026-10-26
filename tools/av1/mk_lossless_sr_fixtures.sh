#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
if [ -n "${AOM_INC:-}" ]; then
  INC="$AOM_INC"
else
  (cd "$W" && apt-get download libaom-dev >/dev/null 2>&1 && dpkg -x libaom-dev*.deb x)
  INC="$W/x/usr/include"
fi
gcc -O1 -I "$INC" -o "$W/aom_still" tools/av1/aom_still.c "${AOM_LIB:-/usr/lib/x86_64-linux-gnu/libaom.so.3}"
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-10}:allf=t:all_seed=$2"; }
enc() {
  name="$1"; size="$2"; seed="$3"; fmt="$4"; chroma="$5"; bd="$6"; sr="$7"; den="$8"; cdef="$9"; lr="${10}"; cq="${11}"
  w=${size%x*}; h=${size#*x}
  ffmpeg -v error -y -f lavfi -i "$(G $size $seed)" -frames:v 1 -pix_fmt "$fmt" -f rawvideo "$W/in.yuv"
  "$W/aom_still" "$W/in.yuv" "$w" "$h" "$chroma" "$bd" 0 "$sr" "$den" "$cdef" "$lr" "$cq" "$W/o.obu"
  ffmpeg -v error -y -f obu -i "$W/o.obu" -c copy -frames:v 1 "$O/$name.avif"
}
for d in 9 10 11 12 13 14 15 16; do
  enc lsr_d${d} 200x200 $((60 + d)) yuv420p 420 8 1 $d 1 1 0
done
enc lsr_d12_nocdef 200x200 81 yuv420p 420 8 1 12 0 1 0
enc lsr_d12_nolr 200x200 82 yuv420p 420 8 1 12 1 0 0
enc lsr_d9_small 136x72 83 yuv420p 420 8 1 9 1 1 0
enc lsr_d13_odd 131x77 84 yuv420p 420 8 1 13 1 1 0
enc lsr_d16_444 200x200 85 yuv444p 444 8 1 16 1 1 0
enc lsr_d11_422 200x200 86 yuv422p 422 8 1 11 1 1 0
enc lsr_d10_400 200x200 87 gray 400 8 1 10 1 1 0
enc lsr_d10_10b 200x200 88 yuv420p10le 420 10 1 10 1 1 0
enc lsr_d14_12b 200x200 89 yuv420p12le 420 12 1 14 1 1 0
enc lsr_d12_big 640x360 90 yuv420p 420 8 1 12 1 1 0
enc lsr_d9_444_10b 200x200 91 yuv444p10le 444 10 1 9 1 1 0

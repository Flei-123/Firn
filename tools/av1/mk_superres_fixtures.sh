#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
svt() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; params="$5"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libsvtav1 -crf "$crf" -preset 6 -svtav1-params "$params" "$O/$name.avif" 2>/dev/null
}
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-10}:allf=t:all_seed=$2"; }
for d in 9 10 12 14 16; do
  svt "sr_d${d}" "$(G 200x200 $d)" yuv420p 30 "superres-mode=1:superres-kf-denom=$d:enable-cdef=1:enable-restoration-filtering=1"
done
svt sr_d12_nolr "$(G 200x200 41)" yuv420p 30 "superres-mode=1:superres-kf-denom=12:enable-cdef=1:enable-restoration-filtering=0"
svt sr_d12_nocdef "$(G 200x200 42)" yuv420p 30 "superres-mode=1:superres-kf-denom=12:enable-cdef=0:enable-restoration-filtering=1"
svt sr_d13_small "$(G 136x72 43)" yuv420p 30 "superres-mode=1:superres-kf-denom=13:enable-cdef=1:enable-restoration-filtering=1"
svt sr_d9_444 "$(G 200x200 44)" yuv444p 30 "superres-mode=1:superres-kf-denom=9:enable-cdef=1:enable-restoration-filtering=1"
svt sr_d11_422 "$(G 200x200 45)" yuv422p 30 "superres-mode=1:superres-kf-denom=11:enable-cdef=1:enable-restoration-filtering=1"
svt sr_d10_10b "$(G 200x200 46)" yuv420p10le 30 "superres-mode=1:superres-kf-denom=10:enable-cdef=1:enable-restoration-filtering=1"
svt sr_d16_big "$(G 640x360 47)" yuv420p 35 "superres-mode=1:superres-kf-denom=16:enable-cdef=1:enable-restoration-filtering=1"

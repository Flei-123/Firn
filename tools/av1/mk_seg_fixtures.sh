#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-14}:allf=t:all_seed=$2"; }
rav() {
  name="$1"; src="$2"; fmt="$3"; qp="$4"; speed="$5"; extra="${6:-}"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v librav1e -qp "$qp" -speed "$speed" ${extra:+-rav1e-params "$extra"} "$O/$name.avif"
}
svt() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; preset="$5"; extra="${6:-}"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libsvtav1 -crf "$crf" -preset "$preset" ${extra:+-svtav1-params "$extra"} "$O/$name.avif" 2>/dev/null
}
aom() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; cpu="$5"; params="$6"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libaom-av1 -crf "$crf" -cpu-used "$cpu" -still-picture 1 -aom-params "$params" "$O/$name.avif"
}
rav seg_300 "$(G 300x300 7 10)" yuv420p 100 3
rav seg_300_hi "$(G 300x300 8 10)" yuv420p 60 6
rav seg_300_lo "$(G 300x300 9 10)" yuv420p 200 6
rav seg_129 "$(G 129x129 10 12)" yuv420p 120 5
rav seg_131x77 "$(G 131x77 11 20)" yuv420p 130 5
rav seg_444 "$(G 200x200 12)" yuv444p 110 5
rav seg_422 "$(G 200x200 13)" yuv422p 110 5
rav seg_400 "$(G 200x200 14)" gray 110 5
rav seg_10b "$(G 300x300 15)" yuv420p10le 110 5
rav seg_testsrc "testsrc2=s=320x240,format=yuv420p" yuv420p 100 4
rav seg_tiles "$(G 512x384 16 8)" yuv420p 120 6 "tile_cols=2:tile_rows=2"
svt dq_svt_300 "$(G 300x300 7 10)" yuv420p 35 6
svt dq_svt_300_aq1 "$(G 300x300 17 10)" yuv420p 40 8 "aq-mode=1"
svt dq_svt_444 "$(G 200x200 18)" yuv444p 35 6
svt dq_svt_10b "$(G 300x300 19)" yuv420p10le 35 6
B="enable-cdef=1:enable-restoration=0"
aom dlf_300 "$(G 300x300 21 10)" yuv420p 30 4 "$B:deltaq-mode=3:delta-lf-mode=1"
aom dlf_300_lr "$(G 300x300 22 10)" yuv420p 30 4 "enable-cdef=1:enable-restoration=1:deltaq-mode=3:delta-lf-mode=1"
aom dlf_129 "$(G 129x129 23 12)" yuv420p 25 3 "$B:deltaq-mode=3:delta-lf-mode=1"
aom dlf_444 "$(G 200x200 24)" yuv444p 30 4 "$B:deltaq-mode=3:delta-lf-mode=1"
aom dlf_400 "$(G 200x200 25)" gray 30 4 "$B:deltaq-mode=3:delta-lf-mode=1"
aom dlf_10b "$(G 300x300 26)" yuv420p10le 30 4 "$B:deltaq-mode=3:delta-lf-mode=1"
aom dlf_sb128 "$(G 300x300 27 10)" yuv420p 30 4 "$B:sb-size=128:deltaq-mode=3:delta-lf-mode=1"
aom dq_aom_300 "$(G 300x300 28 10)" yuv420p 30 4 "$B:deltaq-mode=1:delta-lf-mode=1"
aom dlf_tiles "$(G 512x384 29 8)" yuv420p 30 4 "$B:deltaq-mode=3:delta-lf-mode=1:tile-columns=1:tile-rows=1"

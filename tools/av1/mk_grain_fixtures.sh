#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
enc() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; params="$5"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libaom-av1 -crf "$crf" -cpu-used 4 -still-picture 1 -aom-params "$params" "$O/$name.avif"
}
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-10}:allf=t:all_seed=$2"; }
for t in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
  enc "fg_t${t}" "$(G 200x200 $t)" yuv420p 30 "film-grain-test=$t"
done
enc fg_t1_odd "$(G 131x77 21)" yuv420p 30 "film-grain-test=1"
enc fg_t5_odd "$(G 129x65 22)" yuv420p 30 "film-grain-test=5"
enc fg_t2_444 "$(G 200x200 23)" yuv444p 30 "film-grain-test=2"
enc fg_t3_422 "$(G 200x200 24)" yuv422p 30 "film-grain-test=3"
enc fg_t4_400 "$(G 200x200 25)" gray 30 "film-grain-test=4"
enc fg_t6_10 "$(G 200x200 26)" yuv420p10le 30 "film-grain-test=6"
enc fg_t7_12 "$(G 200x200 27)" yuv420p12le 30 "film-grain-test=7"
enc fg_t8_10_444 "$(G 200x200 28)" yuv444p10le 30 "film-grain-test=8"
enc fg_t9_big "$(G 400x300 29)" yuv420p 30 "film-grain-test=9"
enc fg_t10_65 "$(G 65x200 30)" yuv420p 30 "film-grain-test=10"
enc fg_t11_10 "$(G 250x130 31)" yuv420p10le 30 "film-grain-test=11"

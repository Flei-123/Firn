set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
OUT=${1:?output directory}
mkdir -p "$OUT"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
for s in 600x300 2000x1500; do
    W=${s%x*}
    H=${s#*x}
    python3 -I tools/jxl/gen_bench_image.py "$D/b$s.png" $W $H
    $J/cjxl "$D/b$s.png" "$OUT/b${s}_d1.jxl" -d 1 -e 7 --epf=3 --gaborish=1
    $J/djxl "$OUT/b${s}_d1.jxl" "$OUT/b${s}_d1.ppm"
done

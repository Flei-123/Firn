set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/mepf
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D" >/dev/null
for e in 1 2 3; do
    $J/cjxl "$D/s300x200.png" "$V/s300x200_e$e.jxl" -m 1 -d 1 -e 7 --epf=$e >/dev/null 2>&1
    $J/djxl "$V/s300x200_e$e.jxl" "$V/s300x200_e$e.ppm" >/dev/null 2>&1
done
$J/cjxl "$D/t37x29.png" "$V/t37x29_e2.jxl" -m 1 -d 1 -e 7 --epf=2 >/dev/null 2>&1
$J/djxl "$V/t37x29_e2.jxl" "$V/t37x29_e2.ppm" >/dev/null 2>&1

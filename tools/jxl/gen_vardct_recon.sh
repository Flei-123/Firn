set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/vd
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D"
for f in t64 t37x29 s300x200; do
    $J/cjxl "$D/$f.png" "$V/${f}_gab.jxl" -d 1 -e 3 --gaborish=1 --epf=0
    $J/djxl "$V/${f}_gab.jxl" "$V/${f}_gab.ppm"
    for n in 1 2 3; do
        $J/cjxl "$D/$f.png" "$V/${f}_epf$n.jxl" -d 1 -e 3 --gaborish=0 --epf=$n
        $J/djxl "$V/${f}_epf$n.jxl" "$V/${f}_epf$n.ppm"
    done
done
for f in t64 t37x29 s300x200 s600x300; do
    $J/djxl "$V/${f}_e7.jxl" "$V/${f}_e7.ppm"
done
python3 -I tools/jxl/gen_vardct_acs_corpus.py "$D"
for f in acsmix acsedges acsnoise; do
    $J/cjxl "$D/$f.png" "$V/${f}_e8.jxl" -d 1.5 -e 8 --epf=1
    $J/djxl "$V/${f}_e8.jxl" "$V/${f}_e8.ppm"
done

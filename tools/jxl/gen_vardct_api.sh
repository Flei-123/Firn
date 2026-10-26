set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/vd
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_api_corpus.py "$D"
for f in rgba64 rgba300; do
    $J/cjxl "$D/$f.png" "$V/${f}_a.jxl" -d 1 -e 3
    $J/djxl "$V/${f}_a.jxl" "$V/${f}_a.pam"
done
for f in d16_64 d16_300; do
    $J/cjxl "$D/$f.ppm" "$V/${f}.jxl" -d 1 -e 3
    $J/djxl "$V/${f}.jxl" "$V/${f}.ppm" --bits_per_sample=16
done
for f in prog300 prog600; do
    $J/cjxl "$D/$f.png" "$V/${f}_ac.jxl" -d 1 -e 3 --progressive_ac
    $J/cjxl "$D/$f.png" "$V/${f}_qac.jxl" -d 1 -e 3 --qprogressive_ac
    for k in ac qac; do
        $J/djxl "$V/${f}_$k.jxl" "$V/${f}_$k.ppm"
    done
done

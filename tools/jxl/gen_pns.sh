set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/pns
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D"
python3 -I tools/jxl/gen_pns_corpus.py "$D"
for spec in s64:3200 t37x29:3200 s300x200:1600 s600x300:6400; do
    f=${spec%%:*}
    iso=${spec##*:}
    $J/cjxl "$D/$f.png" "$V/${f}_noise.jxl" -d 1 -e 7 --photon_noise_iso=$iso
    $J/djxl "$V/${f}_noise.jxl" "$V/${f}_noise.ppm"
done
$J/cjxl "$D/text.png" "$V/text_patches.jxl" -d 1 -e 7 --patches=1
$J/djxl "$V/text_patches.jxl" "$V/text_patches.ppm"

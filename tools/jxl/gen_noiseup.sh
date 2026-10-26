set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/nup
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D" >/dev/null
python3 -I tools/jxl/gen_pns_corpus.py "$D" >/dev/null
enc() {
    name=$1
    src=$2
    shift 2
    $J/cjxl "$D/$src.png" "$V/$name.jxl" "$@" >/dev/null 2>&1
    $J/djxl "$V/$name.jxl" "$V/$name.ppm" >/dev/null 2>&1
}
for r in 2 4; do
    enc s300x200_vd_r$r s300x200 -d 1 -e 7 --resampling=$r --photon_noise_iso=1600
    enc s300x200_md_r$r s300x200 -m 1 -d 1 -e 7 --resampling=$r --photon_noise_iso=1600
done
enc t37x29_vd_r2 t37x29 -d 1 -e 7 --resampling=2 --photon_noise_iso=3200
enc s600x300_vd_r2 s600x300 -d 1 -e 7 --resampling=2 --photon_noise_iso=6400

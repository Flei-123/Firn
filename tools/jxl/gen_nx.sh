set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/nx
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
enc text_noise800 text -m 1 -d 0 -e 7 --photon_noise_iso=800
enc text_noise6400 text -m 1 -d 0 -e 7 --photon_noise_iso=6400
enc s300x200_noise1600 s300x200 -m 1 -d 0 -e 7 --photon_noise_iso=1600
enc s600x300_noise3200 s600x300 -m 1 -d 0 -e 7 --photon_noise_iso=3200
enc t37x29_noise3200 t37x29 -m 1 -d 0 -e 7 --photon_noise_iso=3200
enc s64_noise100 s64 -m 1 -d 0 -e 3 --photon_noise_iso=100
enc text_patches text -m 1 -d 0 -e 7
enc text_patches_e9 text -m 1 -d 0 -e 9

set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/mx
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
for f in s64 t37x29 s300x200; do
    enc ${f}_mx $f -m 1 -d 1 -e 7
done
enc s300x200_mxnoise s300x200 -m 1 -d 1 -e 7 --photon_noise_iso=1600
enc text_mxpatches text -m 1 -d 1 -e 7 --patches=1

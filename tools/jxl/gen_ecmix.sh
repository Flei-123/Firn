set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/ecmix
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_api_corpus.py "$D" >/dev/null
enc() {
    name=$1
    src=$2
    shift 2
    $J/cjxl "$D/$src.png" "$V/$name.jxl" "$@" >/dev/null 2>&1
    $J/djxl "$V/$name.jxl" "$V/$name.pam" >/dev/null 2>&1
}
for pair in 1:2 1:4 1:8 2:4 2:8 4:8; do
    r=${pair%%:*}
    e=${pair##*:}
    enc rgba300_vd_c${r}_e${e} rgba300 -d 1 -e 7 --resampling=$r --ec_resampling=$e
    enc rgba300_md_c${r}_e${e} rgba300 -m 1 -d 1 -e 7 --resampling=$r --ec_resampling=$e
done
enc rgba64_vd_c1_e2 rgba64 -d 1 -e 7 --resampling=1 --ec_resampling=2
enc rgba64_md_c2_e4 rgba64 -m 1 -d 1 -e 7 --resampling=2 --ec_resampling=4

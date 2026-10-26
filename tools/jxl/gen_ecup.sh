set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/ecup
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
for r in 2 4 8; do
    enc rgba300_vd_r$r rgba300 -d 1 -e 7 --resampling=$r
    enc rgba300_md_r$r rgba300 -m 1 -d 1 -e 7 --resampling=$r
done
enc rgba64_vd_r2 rgba64 -d 1 -e 7 --resampling=2

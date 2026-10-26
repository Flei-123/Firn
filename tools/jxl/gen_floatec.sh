set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
H=${JXL_HEADERS:-/root/jarvis/data/staff_wt/firn/_jxl_ref/src/libjxl-0.8.2/lib/include}
L=${JXL_LIB:-/usr/lib/x86_64-linux-gnu/libjxl.so.0.7}
V=tests/data/jxl/fec
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
gcc -O1 -Itools/jxl/floatec/include -I"$H" -o "$D/enc" tools/jxl/floatec/enc_float_ec.c "$L" -lm
mk() {
    name=$1
    shift
    "$D/enc" "$@" "$V/$name.jxl" >/dev/null 2>&1
    $J/djxl "$V/$name.jxl" "$V/$name.pam" >/dev/null 2>&1
}
mkf() {
    name=$1
    w=$2
    h=$3
    dist=$4
    rs=$5
    ers=$6
    mode=$7
    pat=$8
    bits=${9:-32}
    exp=${10:-8}
    "$D/enc" $w $h $dist $rs $ers $mode "$V/$name.jxl" $pat $bits $exp >/dev/null 2>&1
    $J/djxl "$V/$name.jxl" "$V/$name.pam" >/dev/null 2>&1
}
mkf f32_md_1_1 72 56 0 1 1 1 0
mkf f32_md_1_2 72 56 0 1 2 1 0
mkf f32_md_1_4 72 56 0 1 4 1 1
mkf f32_mx_1_1 72 56 1.0 1 1 2 0
mkf f32_mx_1_2 72 56 1.0 1 2 2 0
mkf f32_mx_2_4 72 56 1.0 2 4 2 1
mkf f32_mx_1_8 72 56 1.0 1 8 2 1
mkf f32_vd_1_2 72 56 1.0 1 2 0 0
mkf f32_vd_1_8 72 56 1.0 1 8 0 2
mkf f32_vd_2_2 72 56 1.0 2 2 0 2
mkf f32_vd_2_4 72 56 1.0 2 4 0 1
mkf h16_mx_1_2 72 56 1.0 1 2 2 1 16 5
mkf h16_vd_2_4 72 56 1.0 2 4 0 1 16 5
mkf f24_mx_1_4 72 56 1.0 1 4 2 1 24 7
python3 -I tools/jxl/gen_vardct_api_corpus.py "$D" >/dev/null
for e in 2 4 8; do
    $J/cjxl "$D/rgba64.png" "$V/i8_md_1_$e.jxl" -d 0 -m 1 --ec_resampling=$e >/dev/null 2>&1
    $J/djxl "$V/i8_md_1_$e.jxl" "$V/i8_md_1_$e.pam" >/dev/null 2>&1
done

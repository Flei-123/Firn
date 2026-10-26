set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/lf
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D" >/dev/null
enc() {
    name=$1
    src=$2
    shift 2
    $J/cjxl "$D/$src.png" "$V/$name.jxl" "$@" >/dev/null 2>&1
    $J/djxl "$V/$name.jxl" "$V/$name.ppm" >/dev/null 2>&1
}
enc s300x200_dc1 s300x200 -d 1 -e 7 --progressive_dc=1
enc s300x200_dc2 s300x200 -d 1 -e 7 --progressive_dc=2
enc s300x200_p s300x200 -d 1 -e 7 -p
enc t37x29_p t37x29 -d 1 -e 7 -p
enc t37x29_dc2 t37x29 -d 1 -e 7 --progressive_dc=2
enc s600x300_p s600x300 -d 2 -e 6 -p

set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/vd
O=tests/data/jxl/spl
R=$PWD
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
mkdir -p $O
FIRNLIB=$R/lib /root/firn/compiler/target/release/firnc tools/jxl/toc_probe.fi -o $D/probe
for src in "$@"; do
    cp $V/$src.jxl $D/p.jxl
    P=$(cd $D && ./probe | tr '\n' ' ')
    for set in a b; do
        python3 -I tools/jxl/gen_splines.py $V/$src.jxl $D/t.jxl $set $P
        $J/djxl $D/t.jxl $D/t.ppm >/dev/null
        cp $D/t.jxl $O/${src}_$set.jxl
        cp $D/t.ppm $O/${src}_$set.ppm
        echo "$src $set ok"
    done
done

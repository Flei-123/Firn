set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/vd
O=tests/data/jxl/dq
R=$PWD
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$R/lib /root/firn/compiler/target/release/firnc tools/jxl/toc_probe.fi -o $D/probe
for src in "$@"; do
    cp $V/$src.jxl $D/p.jxl
    P=$(cd $D && ./probe | tr '\n' ' ')
    for variant in ${VARIANTS:-lib dct small all}; do
        for strip in 0 1; do
            python3 -I tools/jxl/gen_dequant.py $R $V/$src.jxl $D/t.jxl $variant $strip $P
            if $J/djxl $D/t.jxl $D/t.ppm >/dev/null 2>&1; then
                cp $D/t.jxl $O/${src}_$variant.jxl
                cp $D/t.ppm $O/${src}_$variant.ppm
                echo "$src $variant strip=$strip ok"
                break
            fi
        done
    done
done

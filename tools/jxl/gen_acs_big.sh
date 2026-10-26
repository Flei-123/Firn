set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
O=tests/data/jxl/acs
R=$PWD
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
mkdir -p $O
FIRNLIB=$R/lib /root/firn/compiler/target/release/firnc tools/jxl/toc_probe.fi -o $D/probe
python3 -I tools/jxl/gen_acs_base.py $D/base.png
$J/cjxl $D/base.png $D/p.jxl -d 1 -e 7 >/dev/null 2>&1
P=$(cd $D && ./probe | tr '\n' ' ')
QF=200
seed=0
for spec in a24:rawg a24:lib b21:rawg b21:dct c22:rawz d23:rawl e25:all f26:rawg g2122:rawl h2523:rawz i2622:dct; do
    layout=${spec%%:*}
    variant=${spec##*:}
    seed=$((seed + 1))
    python3 -I tools/jxl/gen_acs_big.py $R $D/p.jxl $D/t.jxl $layout $QF $variant $seed $P
    $J/djxl $D/t.jxl $D/t.ppm >/dev/null 2>&1
    cp $D/t.jxl $O/${layout}_$variant.jxl
    cp $D/t.ppm $O/${layout}_$variant.ppm
    echo "$layout $variant ok"
done

set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
O=tests/data/jxl/lf
R=$PWD
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
mkdir $D/work $D/out
FIRNLIB=$R/lib /root/firn/compiler/target/release/firnc tools/jxl/frames_probe.fi -o $D/probe
python3 -I tools/jxl/gen_dclevel.py $J/cjxl $D/probe $D/work $D/out
for n in dc3 dc4; do
    $J/djxl $D/out/$n.jxl $D/out/$n.ppm >/dev/null
    cp $D/out/$n.jxl $O/dclevel${n#dc}.jxl
    cp $D/out/$n.ppm $O/dclevel${n#dc}.ppm
    echo "$n ok"
done

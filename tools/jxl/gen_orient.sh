set -e
J=/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin
V=tests/data/jxl/or
mkdir -p "$V"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I - "$D" <<'PY'
import sys
from PIL import Image
d = sys.argv[1]
for o in range(2, 9):
    im = Image.new('RGB', (40, 24))
    px = im.load()
    for y in range(24):
        for x in range(40):
            px[x, y] = (x * 6 % 256, y * 10 % 256, (x + y) * 4 % 256)
    ex = Image.Exif()
    ex[0x0112] = o
    im.save('%s/o%d.png' % (d, o), exif=ex)
PY
for o in 2 3 4 5 6 7 8; do
    $J/cjxl "$D/o$o.png" "$V/o${o}_vd.jxl" -d 1 -e 7 >/dev/null 2>&1
    $J/djxl "$V/o${o}_vd.jxl" "$V/o${o}_vd.ppm" >/dev/null 2>&1
    $J/cjxl "$D/o$o.png" "$V/o${o}_md.jxl" -m 1 -d 1 -e 7 >/dev/null 2>&1
    $J/djxl "$V/o${o}_md.jxl" "$V/o${o}_md.ppm" >/dev/null 2>&1
done

#!/usr/bin/env bash
# tools/testkit/selftest.sh -- the test kit tests itself (shell and Python halves).
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/testkit.sh"
tk_tmpdir tkself
out=$( ( . "$here/testkit.sh"; ok "a"; bad "b"; ok "c"; tk_summary demo; echo "rc=$?" ) 2>&1 )
check_out "shell kit prints the old lines and counts" $'  OK    a\n  FAIL  b\n  OK    c\ndemo: 2 passed, 1 failed\nrc=1' printf '%s' "$out"
check "check passes on exit 0" true
printf 'x' > "$D/f"
check_out "check_out compares trimmed output" "x" cat "$D/f"
if python3 - "$here" "$D" <<'PY'
import sys, os
sys.path.insert(0, sys.argv[1])
from testkit import *
d = sys.argv[2]
k = Kit("py")
img = ppm_solid(4, 3, (10, 20, 30))
write_ppm(d + "/a.ppm", *img)
back = read_ppm(d + "/a.ppm")
k.check("ppm roundtrip", back == img)
open(d + "/c.ppm", "wb").write(b"P6\n# a comment\n2 1\n255\n\x01\x02\x03\x04\x05\x06")
k.check("ppm with comment", read_ppm(d + "/c.ppm") == (2, 1, b"\x01\x02\x03\x04\x05\x06"))
other = (4, 3, bytes((10, 20, 30) * 11 + (10, 20, 40)))
k.check("ppm_diff counts one pixel, delta 10", ppm_diff(img, other) == (1, 10))
k.check("ppm_diff with tolerance", ppm_diff(img, other, 10)[0] == 0)
try:
    import numpy as np
    arr = parse_ppm_np(open(d + "/c.ppm", "rb").read())
    k.check("parse_ppm_np shape and values", arr.shape == (1, 2, 3) and arr[0, 1, 2] == 6)
except ImportError:
    k.ok("parse_ppm_np skipped (no numpy)")
k.check("crop", ppm_crop(other, 3, 2, 1, 1) == (1, 1, bytes((10, 20, 40))))
ppm_to_png(img, d + "/a.png")
k.check("png header", open(d + "/a.png", "rb").read(8) == b"\x89PNG\r\n\x1a\n")
sys.exit(k.summary())
PY
then ok "python half"; else bad "python half"; fi
tk_summary "testkit selftest"

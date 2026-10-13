#!/usr/bin/env bash
# tools/fontfb/run.sh -- gate 135: font fallback chain, COLR v0, CBDT/CBLC bitmap emoji, grapheme clusters.
#   1. probe.fi      runs + COLR v0 layers vs fontTools (as before)
#   2. chainprobe.fi + chain_check.py: bitmap glyphs vs Pillow/FreeType, PNG, numpy; runs/width vs
#      fontTools + regex \X (needs python3 with PIL, numpy, fontTools; `regex` is pip-installed
#      into a temp dir if missing, else part E is SKIPPED loudly)
#   3. tools/grapheme/run.sh (UAX #29 table + GraphemeBreakTest + regex cross-check)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
RC=0
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
"$FIRNC" --opt-level=dev-fast -o "$D/probe" tools/fontfb/probe.fi || exit 1
python3 - "$D/probe" <<'PY'
import subprocess, sys
from fontTools.ttLib import TTFont
probe = sys.argv[1]
JP = "tools/fontfb/data/NotoSansJP.subset.ttf"
DJ = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
CO = "tools/fontfb/data/COLR-palettes-test-font.ttf"
fonts = [TTFont(p) for p in (JP, DJ, CO)]
cms = [f.getBestCmap() for f in fonts]
fails = checks = 0
def expect_runs(text):
    runs = []
    off = 0
    for ch in text:
        l = len(ch.encode())
        fi = next((i for i, cm in enumerate(cms) if ord(ch) in cm), 0)
        if runs and runs[-1][2] == fi:
            runs[-1][1] += l
        else:
            runs.append([off, l, fi])
        off += l
    return runs
for text in ["test!AB", "abc xyz", "A!B?C", "tesét", "⚠ x 中", "e" * 70]:
    out = subprocess.run([probe, JP, DJ, CO, "--", text], capture_output=True, text=True).stdout.split("\n")
    got = [list(map(int, l.split()[1:])) for l in out if l.startswith("run ")]
    exp = expect_runs(text)
    checks += 1
    if got != exp[:len(got)] or (len(exp) <= 64 and got != exp):
        fails += 1; print("FAIL runs", repr(text), got, exp)
    # layers
    col = fonts[2]
    cp = col["COLR"].ColorLayers
    pal = col["CPAL"].palettes[0]
    for l in out:
        if l.startswith("layers "):
            parts = l.split()
            ch = chr(int(parts[1])); g = cms[2][ord(ch)]
            exp_l = []
            for lay in cp[g]:
                c = pal[lay.colorID]
                exp_l.append((col.getGlyphID(lay.name), (c.alpha << 24) | (c.blue << 16) | (c.green << 8) | c.red))
            got_l = [tuple(map(int, x.split(":"))) for x in parts[2:]]
            checks += 1
            if got_l != exp_l:
                fails += 1; print("FAIL layers", repr(ch), got_l, exp_l)
# all colour glyphs of the font
allch = "".join(chr(c) for c in cms[2] if cms[2][c] in cp)
out = subprocess.run([probe, JP, DJ, CO, "--", allch], capture_output=True, text=True).stdout.split("\n")
n_lay = sum(1 for l in out if l.startswith("layers "))
checks += 1
if n_lay != len(allch):
    fails += 1; print("FAIL all colour glyphs", n_lay, len(allch))
paint = [l for l in out if l.startswith("paint")]
print("  colour glyphs checked: %d, paint smoke: %s" % (n_lay, paint[0] if paint else "none"))
print("RESULT fontfb: %s (%d checks, %d failed)" % ("ok" if not fails else "FAIL", checks, fails))
sys.exit(1 if fails else 0)
PY
[ $? -eq 0 ] || RC=1
"$FIRNC" --opt-level=dev-fast -o "$D/chainprobe" tools/fontfb/chainprobe.fi || { echo "FAIL compile chainprobe"; exit 1; }
if ! python3 -I -c "import regex" 2>/dev/null; then
    timeout 90 python3 -m pip install -q --disable-pip-version-check --target "$D/pk" regex >/dev/null 2>&1
fi
PYTHONPATH="$D/pk" python3 -u tools/fontfb/chain_check.py "$D/chainprobe" || RC=1
bash tools/grapheme/run.sh || RC=1
exit $RC

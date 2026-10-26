set -e
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I tools/jxl/gen_vardct_corpus.py "$D"
for f in t64 t37x29 s300x200 s600x300; do
    cjxl "$D/$f.png" "tests/data/jxl/vd/${f}_e7.jxl" -d 1 -e 7 --gaborish=0 --epf=0
done

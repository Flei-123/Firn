#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/fui/gposkern_check.py -- GPOS PAIR KERNING (roadmap r13) against fontTools.

Builds lib/font/font_main.fi, asks it for the kerning of 3000 random pairs plus
seven famous ones (command K) in a font that has NO `kern` table, only GPOS
(Inter, subset in tests/data/fonts), and compares every answer with a walk
through the same GPOS tables done by fontTools. Needs at least 300 non-zero
pairs, so a reader that finds nothing cannot pass.

    python3 tools/fui/gposkern_check.py [font.ttf]
"""
import os, shutil, subprocess, tempfile
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
import struct, sys, random
from fontTools.ttLib import TTFont
path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "tests/data/fonts/InterSubset-Regular.ttf")
tmp = tempfile.mkdtemp(prefix="gposkern-")
binary = os.path.join(tmp, "font_main")
firnc = os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc"))
if not os.path.exists(firnc):
    firnc = "/root/firn/compiler/target/release/firnc"
subprocess.check_call([firnc, "--opt-level=dev", "-o", binary, os.path.join(ROOT, "lib/font/font_main.fi")],
                      env=dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib")))
fb=open(path,'rb').read()
t=TTFont(path); cmap=t.getBestCmap(); order=t.getGlyphOrder()
# reference via hb-like: use fontTools GPOS walk
gpos=t['GPOS'].table
klook=set()
for fr in gpos.FeatureList.FeatureRecord:
    if fr.FeatureTag=='kern': klook.update(fr.Feature.LookupListIndex)
def ref(a,b):
    total=0
    for li in sorted(klook):
        lk=gpos.LookupList.Lookup[li]
        for st in lk.SubTable:
            if lk.LookupType==9: st=st.ExtSubTable
            if st.LookupType!=2 if hasattr(st,'LookupType') else False: continue
            cov=st.Coverage.glyphs
            if a not in cov: continue
            if st.Format==1:
                ps=st.PairSet[cov.index(a)]
                hit=[r for r in ps.PairValueRecord if r.SecondGlyph==b]
                if not hit: continue
                v=hit[0].Value1
                total+= (getattr(v,'XAdvance',0) or 0) if v else 0
                break
            else:
                c1=st.ClassDef1.classDefs.get(a,0); c2=st.ClassDef2.classDefs.get(b,0)
                v=st.Class1Record[c1].Class2Record[c2].Value1
                total+= (getattr(v,'XAdvance',0) or 0) if v else 0
                break
    return total
cps=[c for c in range(32,0x250) if c in cmap]
random.seed(1)
pairs=[(a,b) for a in cps[:200] for b in cps[:200]]
random.shuffle(pairs); pairs=pairs[:3000]
pairs+= [(ord('A'),ord('V')),(ord('T'),ord('o')),(ord('A'),ord('T')),(ord('W'),ord('a')),(ord('f'),ord('f')),(ord('r'),ord('.')),(ord('L'),ord('y'))]
script=''.join('K %d %d\n'%p for p in pairs).encode()
out=subprocess.run([binary],input=struct.pack('<I',len(fb))+fb+script,capture_output=True).stdout.decode().split('\n')
out=[l for l in out if l.startswith('K ')]
bad=0;nz=0
for p,l in zip(pairs,out):
    got=int(l.split()[1]); want=ref(cmap[p[0]],cmap[p[1]])
    if want: nz+=1
    if got!=want:
        bad+=1
        if bad<8: print('MISMATCH',chr(p[0]),chr(p[1]),got,want)
print(path.split('/')[-1],'pairs',len(pairs),'nonzero',nz,'mismatch',bad)
shutil.rmtree(tmp, ignore_errors=True)
if bad or nz < 300:
    print('GPOSKERN FAILED')
    sys.exit(1)
print('GPOSKERN PASSED.')

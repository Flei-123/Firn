# SPDX-License-Identifier: GPL-2.0-only
import os,re,struct,subprocess,collections,sys
DATA=os.environ.get('T262','.js-work/t262')
SUB=sys.argv[2] if len(sys.argv)>2 else 'test/language'
FRONT=re.compile(r"/\*---(.*?)---\*/",re.S)
def meta(src):
    m=FRONT.search(src)
    if not m: return {}
    b=m.group(1); out={}
    neg=re.search(r"^negative:\s*$(.*?)(?=^\S|\Z)",b,re.S|re.M)
    if neg:
        ph=re.search(r"phase:\s*(\S+)",neg.group(1))
        out['negative']={'phase':ph.group(1) if ph else ''}
    fl=re.search(r"^flags:\s*\[(.*?)\]",b,re.M)
    out['flags']=[x.strip() for x in fl.group(1).split(',')] if fl else []
    return out
jobs=[];idx=[]
for base,_,files in os.walk(os.path.join(DATA,SUB)):
    if '_FIXTURE' in base: continue
    for f in sorted(files):
        if not f.endswith('.js') or f.endswith('_FIXTURE.js'): continue
        path=os.path.join(base,f)
        src=open(path,encoding='utf-8').read(); m=meta(src)
        neg=m.get('negative') or {}
        wf=neg.get('phase') in ('parse','early')
        fl=m.get('flags',[])
        if 'module' in fl: vs=[(1,src)]
        elif 'raw' in fl: vs=[(0,src)]
        else:
            vs=[]
            if 'onlyStrict' not in fl: vs.append((0,src))
            if 'noStrict' not in fl: vs.append((0,'"use strict";\n'+src))
        for mode,text in vs:
            d=text.encode('utf-8'); jobs.append(struct.pack('<II',mode,len(d))+d); idx.append((path,wf))
res=subprocess.run([sys.argv[1]],input=b''.join(jobs),stdout=subprocess.PIPE)
lines=res.stdout.decode('utf-8','replace').splitlines()
bad=collections.Counter(); samples=collections.defaultdict(list); ok=0
for (path,wf),ln in zip(idx,lines):
    gf=ln.startswith('ERR')
    if gf==wf: ok+=1; continue
    code=ln.split()[1] if gf else 'OK'
    key=('want-parse:err'+code) if not wf else ('want-fail:'+code)
    bad[key]+=1
    if len(samples[key])<40: samples[key].append(os.path.relpath(path,DATA))
print("runs %d passed %d quota %.2f%%"%(len(idx),ok,100.0*ok/max(1,len(idx))))
for k,v in bad.most_common(20):
    print("  %-18s %6d   %s"%(k,v,samples[k][:2]))
if len(sys.argv)>3:
    for s in samples[sys.argv[3]][:30]: print("   ",s)

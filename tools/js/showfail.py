# SPDX-License-Identifier: GPL-2.0-only
import os,re,struct,subprocess,collections,sys,random
DATA=os.environ.get('T262','.js-work/t262')
SUB=sys.argv[2]; WANT=sys.argv[3] if len(sys.argv)>3 else '1'
FRONT=re.compile(r"/\*---(.*?)---\*/",re.S)
def meta(src):
    m=FRONT.search(src)
    if not m: return {}
    b=m.group(1); out={}
    neg=re.search(r"^negative:\s*$(.*?)(?=^\S|\Z)",b,re.S|re.M)
    if neg:
        ph=re.search(r"phase:\s*(\S+)",neg.group(1)); out['negative']={'phase':ph.group(1) if ph else ''}
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
            d=text.encode('utf-8'); jobs.append(struct.pack('<II',mode,len(d))+d); idx.append((path,wf,text))
res=subprocess.run([sys.argv[1]],input=b''.join(jobs),stdout=subprocess.PIPE)
lines=res.stdout.decode('utf-8','replace').splitlines()
hits=[]
for (path,wf,text),ln in zip(idx,lines):
    gf=ln.startswith('ERR')
    if gf==wf: continue
    if wf:
        if WANT=='OK': hits.append((path,text,'accepted but must fail',0))
        continue
    if not gf: continue
    parts=ln.split()
    if parts[1]!=WANT: continue
    n=int(parts[2]); srcl=text.split('\n')
    bad=srcl[n-1].strip() if 0<n<=len(srcl) else '?'
    hits.append((path,text,bad,n))
print("total %d"%len(hits))
cnt=collections.Counter(h[2][:70] for h in hits)
for k,v in cnt.most_common(30): print("  %5d  %s"%(v,k))

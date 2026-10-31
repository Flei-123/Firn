import re,sys
src,dst=sys.argv[1],sys.argv[2]
cls={}
cur=None
entries=[]
for line in open(src):
    if line.startswith('#') or not line.strip(): continue
    if not line.startswith(' '):
        m=re.match(r'(\S+) -> (\S+):',line)
        cur=(m.group(1),m.group(2)); cls[cur[0]]=cur[1]; entries.append((cur,[]))
    else:
        entries[-1][1].append(line.strip())
prim={'int':'I','long':'J','float':'F','double':'D','boolean':'Z','byte':'B','short':'S','char':'C','void':'V'}
def desc(t,obf):
    dim=0
    while t.endswith('[]'): dim+=1;t=t[:-2]
    if t in prim: d=prim[t]
    else:
        n=cls.get(t,t) if obf else t
        d='L'+n.replace('.','/')+';'
    return '['*dim+d
out=[]
for (d,o),mem in entries:
    out.append('CL: %s %s'%(o.replace('.','/'),d.replace('.','/')))
    for m in mem:
        mm=re.match(r'(?:\d+:\d+:)?(\S+) (\S+)\((.*)\) -> (\S+)$',m)
        if mm:
            rt,name,args,on=mm.groups()
            ar=[a for a in args.split(',') if a]
            od='('+''.join(desc(a,True) for a in ar)+')'+desc(rt,True)
            dd='('+''.join(desc(a,False) for a in ar)+')'+desc(rt,False)
            if name in('<init>','<clinit>'): continue
            out.append('MD: %s/%s %s %s/%s %s'%(o.replace('.','/'),on,od,d.replace('.','/'),name,dd))
        else:
            mf=re.match(r'(\S+) (\S+) -> (\S+)$',m)
            if mf:
                out.append('FD: %s/%s %s/%s'%(o.replace('.','/'),mf.group(3),d.replace('.','/'),mf.group(2)))
open(dst,'w').write('\n'.join(out)+'\n')

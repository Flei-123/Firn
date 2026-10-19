import sys,json,statistics,math,collections
d=collections.defaultdict(lambda: collections.defaultdict(list)); sums=collections.defaultdict(dict)
for ln in open(sys.argv[1]):
    p=ln.split()
    if len(p)>=4 and p[2]!='ERR':
        d[p[0]][p[1]].append(float(p[2])); sums[p[0]].setdefault(p[1],set()).add(p[3] if len(p)>3 else '')
    elif len(p)>=3 and p[2]=='ERR': d[p[0]][p[1]+'_err']=[1]
out={}; rs=[]
for n in sorted(d):
    f=d[n].get('firn',[]); c=d[n].get('c',[])
    e={'firn_median':statistics.median(f) if f else None,'c_median':statistics.median(c) if c else None,'firn_runs':f,'c_runs':c,
       'sum_match': (sums[n].get('firn')==sums[n].get('c') and len(sums[n].get('firn',[]))==1)}
    if f and c and e['c_median']>0: e['ratio']=e['firn_median']/e['c_median']; 
    if e.get('ratio') and e['sum_match']: rs.append(e['ratio'])
    out[n]=e
g=math.exp(sum(map(math.log,rs))/len(rs)) if rs else None
json.dump({'device':'Xiaomi 14T Pro','geomean_firn_over_gcc_o2':g,'n':len(rs),'benchmarks':out},open(sys.argv[2],'w'),indent=1)
print('geomean',g,len(rs))
for n,e in out.items(): print(n,e['firn_median'],e['c_median'],e.get('ratio'),e['sum_match'])

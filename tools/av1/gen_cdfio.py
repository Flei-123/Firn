import re,sys
t=open('lib/av1/avif_cdf.fi').read()
consts=dict((m.group(1),int(m.group(2))) for m in re.finditer(r'const (\w+): usize = (\d+)',t))
def regions(prefix):
    out=[]
    for k,v in consts.items():
        if k.startswith(prefix) and not k.endswith('_N') and not k.endswith('_LEAVES') and k+'_N' in consts:
            out.append((v,consts[k+'_N'],consts[k+'_LEAVES']))
    return sorted(out)
def table(name,prefix):
    r=regions(prefix)
    rows=[]
    for i in range(0,len(r),4):
        rows.append('    '+', '.join('%d, %d, %d'%x for x in r[i:i+4])+',')
    return 'static %s: [u16; %d] = [\n%s\n]\n'%(name,len(r)*3,'\n'.join(rows)), len(r)
mt,mn=table('MODE_REGIONS','M_')
kt,kn=table('COEF_REGIONS','K_')
head=open('tools/av1/cdfio_head.fi').read()
head=head.replace('@MODE_N@',str(mn)).replace('@COEF_N@',str(kn))
open('lib/av1/av1cdfio.fi','w').write(head.replace('@TABLES@',mt+'\n'+kt))

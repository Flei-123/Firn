import re,sys
path='lib/av1/avif_cdf.fi'
t=open(path).read()
if 'M_MV_JOINT' in t:
    sys.exit(0)
def lv(n,vals):
    return vals+[32768,0]
regions=[]
vals=[]
def add(name,n,leaves):
    regions.append((name,len(vals)+4401,n,leaves))
add('M_MV_JOINT',4,2)
for _ in range(2):
    vals+=[4096,11264,19328,32768,0]
add('M_MV_CLASS',11,4)
for _ in range(4):
    vals+=[28672,30976,31858,32320,32551,32656,32740,32757,32762,32767,32768,0]
add('M_MV_CLASS0_FR',4,8)
for _ in range(4):
    vals+=[16384,24576,26624,32768,0,12288,21248,24128,32768,0]
add('M_MV_FR',4,4)
for _ in range(4):
    vals+=[8192,17408,21248,32768,0]
for name,d in (('M_MV_SIGN',128),('M_MV_CLASS0_BIT',216),('M_MV_CLASS0_HP',160),('M_MV_HP',128)):
    add(name,2,4)
    for _ in range(4):
        vals+=[d*128,32768,0]
add('M_MV_BIT',2,40)
bits=[136,140,148,160,176,192,224,234,234,240]
for _ in range(4):
    for b in bits:
        vals+=[b*128,32768,0]
total=4401+len(vals)
decl=''
for name,off,n,leaves in regions:
    decl+='export { %s, %s_N, %s_LEAVES }\nconst %s: usize = %d\nconst %s_N: usize = %d\nconst %s_LEAVES: usize = %d\n'%(name,name,name,name,off,name,n,name,leaves)
t=t.replace('const M_TX_SIZE_LEAVES: usize = 12\n','const M_TX_SIZE_LEAVES: usize = 12\n'+decl,1)
t=t.replace('const CDF_MODE_LEN: usize = 4401','const CDF_MODE_LEN: usize = %d'%total)
i=t.index('static AV1_CDF_MODE')
j=t.index('\n]\n',i)
rows=''
for k in range(0,len(vals),16):
    rows+='\n    '+', '.join(str(v) for v in vals[k:k+16])+','
t=t[:i]+t[i:j].replace('[u16; 4401]','[u16; %d]'%total)+rows+t[j:]
open(path,'w').write(t)
h=open('tools/av1/cdfio_head.fi').read()
h=h.replace('const MODE_CELLS: i64 = 4401','const MODE_CELLS: i64 = %d'%total)
h=h.replace('const SLOT_CELLS: i64 = 8446','const SLOT_CELLS: i64 = %d'%(total+4045))
open('tools/av1/cdfio_head.fi','w').write(h)
print(total)

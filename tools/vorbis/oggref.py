import sys,struct
d=open(sys.argv[1],'rb').read()
pos=0;pk=0;by=0;lastg=-1;pages=0;cur=0
while pos+27<=len(d):
    assert d[pos:pos+4]==b'OggS'
    n=d[pos+26]; seg=d[pos+27:pos+27+n]; g=struct.unpack('<q',d[pos+6:pos+14])[0]
    body=sum(seg); pages+=1
    for i,l in enumerate(seg):
        cur+=l
        if l<255:
            pk+=1; by+=cur; cur=0
            if i==n-1: lastg=g
    pos+=27+n+body
print(pk,by,lastg,pages,0)

import subprocess, struct, random, math, sys
import numpy as np
exe = sys.argv[1]
W=[2,3,4,5,6,2,3,3,4,4,5,5,6,2,4,3,5,4,6]
H=[2,3,4,5,6,3,2,4,3,5,4,6,5,4,2,5,3,6,4]
RK=[0,0,1,1,0,1,1,1,1,2,2,0,2,1,2,1]
CK=[0,1,0,1,1,0,1,1,1,2,0,2,1,2,1,2]
def basis(kind,n):
    N=1<<n
    B=np.zeros((N,N))
    for y in range(N):
        for k in range(N):
            if kind==0: B[y,k]=(1/math.sqrt(2) if k==0 else 1)*math.cos(math.pi*(2*y+1)*k/(2*N))
            elif kind==1:
                if N==4: B[y,k]=math.sin(math.pi*(y+1)*(2*k+1)/9)
                else: B[y,k]=math.sin(math.pi*(2*y+1)*(2*k+1)/(4*N))
            else: B[y,k]=1.0 if y==k else 0.0
    return B
bad=0; tot=0
random.seed(1)
for sz in range(19):
    for tt in range(16):
        # identity only allowed up to 32 in practice; skip 64 identity
        w=1<<W[sz]; h=1<<H[sz]
        if (RK[tt]==2 and w>32) or (CK[tt]==2 and h>32): continue
        if (RK[tt]==1 and w>16) or (CK[tt]==1 and h>16): continue
        cw=min(w,32); ch=min(h,32)
        X=np.zeros((h,w))
        for i in range(ch):
            for j in range(cw):
                if random.random()<0.25: X[i,j]=random.randint(-400,400)
        inp=struct.pack('<ii',sz,tt)+b''.join(struct.pack('<i',int(X[i,j])) for i in range(ch) for j in range(cw))
        out=subprocess.run([exe],input=inp,capture_output=True).stdout
        res=np.array(struct.unpack('<%di'%(w*h),out),dtype=float).reshape(h,w)
        ref=basis(CK[tt],H[sz])@X@basis(RK[tt],W[sz]).T
        s=(ref*res).sum()/max((ref*ref).sum(),1e-9)
        err=np.abs(res-s*ref).max(); mx=max(np.abs(res).max(),1)
        tot+=1
        if err>0.02*mx+3:
            bad+=1; print('BAD sz',sz,'tt',tt,'err',err,'max',mx,'scale',s)
print('txfm_test: %d of %d ok'%(tot-bad,tot))
sys.exit(1 if bad else 0)

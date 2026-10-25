import subprocess, struct, random, math, sys
import numpy as np
exe = sys.argv[1]
W=[2,3,4,5,6,2,3,3,4,4,5,5,6,2,4,3,5,4,6]
H=[2,3,4,5,6,3,2,4,3,5,4,6,5,4,2,5,3,6,4]
RK=[0,0,1,1,0,1,1,1,1,2,2,0,2,1,2,1]
CK=[0,1,0,1,1,0,1,1,1,2,0,2,1,2,1,2]
FUD=[0,0,0,0,1,0,1,0,1,0,0,0,0,0,1,0]
FLR=[0,0,0,0,0,1,1,1,0,0,0,0,0,0,0,1]
DEN={3:2,9:2,10:2,17:2,18:2,4:4,11:4,12:4}
def basis(kind,n):
    N=1<<n
    B=np.zeros((N,N))
    for y in range(N):
        for k in range(N):
            if kind==0: B[y,k]=(1/math.sqrt(2) if k==0 else 1)*math.cos(math.pi*(2*y+1)*k/(2*N))
            elif kind==1:
                B[y,k]=math.sin(math.pi*(y+1)*(2*k+1)/9) if N==4 else math.sin(math.pi*(2*y+1)*(2*k+1)/(4*N))
            else: B[y,k]=1.0 if y==k else 0.0
    return B
bad=0; tot=0
random.seed(2)
for sz in range(19):
    for tt in (0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15):
        w=1<<W[sz]; h=1<<H[sz]
        if (RK[tt]==2 and w>32) or (CK[tt]==2 and h>32): continue
        if (RK[tt]==1 and w>16) or (CK[tt]==1 and h>16): continue
        tw=min(w,32); th=min(h,32)
        L=np.zeros((th,tw))
        L[0,0]=random.randint(-3,3)
        for i in range(th):
            for j in range(tw):
                if random.random()<0.1: L[i,j]=random.randint(-2,2)
        dcq=random.randint(4,60); acq=random.randint(4,60)
        pred=bytes([128])*(w*h)
        inp=struct.pack('<iiii',sz,tt,dcq,acq)+b''.join(struct.pack('<i',int(L[i,j])) for i in range(th) for j in range(tw))+pred
        out=subprocess.run([exe],input=inp,capture_output=True).stdout
        res=np.array(list(out),dtype=float).reshape(h,w)-128
        D=L*acq; D[0,0]=L[0,0]*dcq
        D=D/DEN.get(sz,1)
        X=np.zeros((h,w)); X[:th,:tw]=D
        ref=basis(CK[tt],H[sz])@X@basis(RK[tt],W[sz]).T
        if FUD[tt]: ref=ref[::-1,:]
        if FLR[tt]: ref=ref[:,::-1]
        if np.abs(res).max()>=120: continue  # clipped, skip
        s=(ref*res).sum()/max((ref*ref).sum(),1e-9)
        err=np.abs(res-s*ref).max(); mx=max(np.abs(res).max(),1)
        tot+=1
        if err>0.05*mx+3:
            bad+=1; print('BAD sz',sz,'tt',tt,'err',err,'max',mx,'scale',s)
print('recon_test: %d of %d ok'%(tot-bad,tot))
sys.exit(1 if bad else 0)

import os,random,subprocess,sys,tempfile
root,vdec,runs=sys.argv[1],sys.argv[2],int(sys.argv[3])
files=[os.path.join(root,f) for f in sorted(os.listdir(root)) if f.endswith('.webm')]
rnd=random.Random(int(os.environ.get("FUZZ_SEED","7")))
bad=0
stats={'ok':0,'err':0}
d=tempfile.mkdtemp()
try:
    for i in range(runs):
        f=rnd.choice(files)
        b=bytearray(open(f,'rb').read())
        lo=min(len(b)-1,rnd.choice([0,200,400]))
        mode=rnd.randrange(4)
        if mode==0:
            b=b[:rnd.randrange(lo+1,len(b))]
        elif mode==1:
            for _ in range(rnd.randrange(1,20)):
                b[rnd.randrange(lo,len(b))]=rnd.randrange(256)
        elif mode==2:
            p=rnd.randrange(lo,len(b));n=rnd.randrange(1,64)
            b[p:p+n]=bytes(rnd.randrange(256) for _ in range(n))
        else:
            for _ in range(rnd.randrange(1,8)):
                p=rnd.randrange(lo,len(b)); b[p]^=1<<rnd.randrange(8)
        p=os.path.join(d,'f.bin'); open(p,'wb').write(b)
        try:
            r=subprocess.run([vdec,p,os.path.join(d,'o.yuv')],timeout=30,capture_output=True)
        except subprocess.TimeoutExpired:
            print('HANG',os.path.basename(f),mode,i); bad+=1
            open(os.path.join(d,'hang_%d.bin'%i),'wb').write(b); continue
        if r.returncode<0 or r.returncode>=100:
            print('CRASH',r.returncode,os.path.basename(f),mode,i,r.stderr[:160]); bad+=1
            open(os.path.join(os.environ.get('FUZZ_OUT','/tmp'),'vp9_crash_%d.bin'%i),'wb').write(b)
        else:
            stats['ok' if r.returncode==0 else 'err']+=1
finally:
    for x in os.listdir(d): os.remove(os.path.join(d,x))
    os.rmdir(d)
print('vp9 fuzz runs=%d bad=%d %s'%(runs,bad,stats))
sys.exit(1 if bad else 0)

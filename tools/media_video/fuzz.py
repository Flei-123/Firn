import os,random,subprocess,sys,tempfile
root,vprobe,vdec,runs=sys.argv[1],sys.argv[2],sys.argv[3],int(sys.argv[4])
files=[os.path.join(root,f) for f in sorted(os.listdir(root))]
rnd=random.Random(int(os.environ.get("FUZZ_SEED","7")))
bad=0
stats={'ok':0,'err':0}
d=tempfile.mkdtemp()
try:
    for i in range(runs):
        f=rnd.choice(files)
        b=bytearray(open(f,'rb').read())
        mode=rnd.randrange(4)
        if mode==0:
            b=b[:rnd.randrange(1,len(b))]
        elif mode==1:
            for _ in range(rnd.randrange(1,20)):
                b[rnd.randrange(len(b))]=rnd.randrange(256)
        elif mode==2:
            p=rnd.randrange(len(b));n=rnd.randrange(1,64)
            b[p:p+n]=bytes(rnd.randrange(256) for _ in range(n))
        else:
            for _ in range(rnd.randrange(1,8)):
                p=rnd.randrange(len(b)); b[p]^=1<<rnd.randrange(8)
        p=os.path.join(d,'f.bin'); open(p,'wb').write(b)
        for cmd in ([vprobe,p,'dump'],[vdec,p,os.path.join(d,'o.yuv')]):
            try:
                r=subprocess.run(cmd,timeout=20,capture_output=True)
            except subprocess.TimeoutExpired:
                print('HANG',f,mode,i); bad+=1; open('/tmp/g5_hang_%d.bin'%i,'wb').write(b); continue
            if r.returncode<0 or r.returncode>=100 and r.returncode!=101 or r.returncode==101:
                print('CRASH',r.returncode,os.path.basename(f),mode,i,r.stderr[:120]); bad+=1
                open('/tmp/g5_crash_%d.bin'%i,'wb').write(b)
            else:
                stats['ok' if r.returncode==0 else 'err']+=1
finally:
    for x in os.listdir(d): os.remove(os.path.join(d,x))
    os.rmdir(d)
print('fuzz runs=%d (x2 tools) bad=%d %s'%(runs,bad,stats))
sys.exit(1 if bad else 0)

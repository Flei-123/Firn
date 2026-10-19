from collections import deque
st=[777]
def nxt():
    st[0]=(st[0]*1103515245+12345)&0x7fffffff
    return st[0]
def ck(arr):
    h=7
    for a in arr: h=(h*31+a)&0xffffffff
    return h
def run(maxlen, n):
    d=deque(maxlen=maxlen) if maxlen else deque()
    acc=0
    for i in range(n):
        r=nxt(); op=(r>>4)%9; v=(r>>12)%1000
        if op==0 or op==1: d.append(v)
        elif op==2 or op==3: d.appendleft(v)
        elif op==4:
            if d: acc=(acc*31+d.popleft())&0xffffffff
        elif op==5:
            if d: acc=(acc*31+d.pop())&0xffffffff
        elif op==6: d.rotate(((r>>20)%21)-10)
        elif op==7: d.extend([v,v+1,v+2])
        else:
            d.extendleft([v,v+1])
    return d,acc
for ml,n in [(0,5000),(5,5000),(1,300),(64,8000)]:
    d,acc=run(ml,n)
    print("RUN",ml,n,"len",len(d),"ck",ck(list(d)),"acc",acc,"first",list(d)[:3])
d=deque(range(10)); d.reverse(); print("REV",ck(list(d)))
d=deque(range(7)); d.rotate(-3); print("ROTL",list(d)); d.rotate(100); print("ROT100",list(d)); d.rotate(-2**63 if False else -700); print("ROT-700",list(d))

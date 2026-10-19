import heapq
st=[12345]
def nxt():
    st[0]=(st[0]*1103515245+12345)&0x7fffffff
    return st[0]
def val(): return (nxt()>>8)%1000
def ck(arr):
    h=7
    for a in arr: h=(h*31+a)&0xffffffff
    return h
# A
h=[]
for v in [5,3,8,1,9,2,7,3]: heapq.heappush(h,v)
print("A",h)
pops=[heapq.heappop(h) for _ in range(len(h))]
print("Apops",pops)
# B
h=[]; acc=0
for i in range(3000):
    r=nxt()
    if r%4==0 and h:
        acc=(acc*31+heapq.heappop(h))&0xffffffff
    elif r%7==0 and h:
        acc=(acc*31+heapq.heappushpop(h,val()))&0xffffffff
    elif r%11==0 and h:
        acc=(acc*31+heapq.heapreplace(h,val()))&0xffffffff
    else:
        heapq.heappush(h,val())
print("B len",len(h),"ck",ck(h),"acc",acc)
# C heapify
arr=[val() for _ in range(500)]
heapq.heapify(arr)
print("C ck",ck(arr),arr[:5])
# D nsmallest/nlargest stable with ties on key
items=[(val()%20,i) for i in range(300)]
ns=heapq.nsmallest(10,items,key=lambda t:t[0])
nl=heapq.nlargest(10,items,key=lambda t:t[0])
print("D ns",[t[1] for t in ns]); print("D nl",[t[1] for t in nl])
ints=[val() for _ in range(400)]
print("E ns",ck(heapq.nsmallest(25,ints)),"nl",ck(heapq.nlargest(25,ints)), "n0",heapq.nsmallest(0,ints), "nbig",ck(heapq.nsmallest(1000,ints)))

import socket,struct,sys
def rc(port,pw,cmds):
    s=socket.create_connection(("127.0.0.1",port));
    def send(i,t,b):
        d=struct.pack("<ii",i,t)+b.encode()+b"\0\0"; s.sendall(struct.pack("<i",len(d))+d)
        n=struct.unpack("<i",s.recv(4))[0]; r=b""
        while len(r)<n: r+=s.recv(n-len(r))
        return r[8:-2].decode()
    send(1,3,pw)
    for c in cmds: print(c,"->",send(2,2,c))
rc(int(sys.argv[1]),sys.argv[2],sys.argv[3:])

import socket, sys, threading
port = int(sys.argv[1])
body = b"FIRN-OK\n" + b"A" * 1024
srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port))
srv.listen(8)
print("ready", flush=True)
def serve():
    while True:
        c, _ = srv.accept()
        c.recv(4096)
        c.sendall(body)
        c.close()
threading.Thread(target=serve, daemon=True).start()
import time
time.sleep(120)

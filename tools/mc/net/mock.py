#!/usr/bin/env python3
"""An independent status server (own VarInt / framing) for the mc_net cross-check.

usage: mock.py <scenario> <portfile>   (writes the port into <portfile>, serves ONE connection, exits)
scenarios: ok-string | ok-component | closed | garbage | silent | late-listen
"""
import json, socket, struct, sys, time


def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        out.append(b | 0x80 if n else b)
        if not n:
            return bytes(out)


def read_varint(c):
    r = sh = 0
    while True:
        b = c.recv(1)
        if not b:
            raise EOFError
        r |= (b[0] & 0x7F) << sh
        sh += 7
        if not b[0] & 0x80:
            return r


def read_packet(c):
    n = read_varint(c)
    d = b""
    while len(d) < n:
        part = c.recv(n - len(d))
        if not part:
            raise EOFError
        d += part
    return d


def send(c, pid, body):
    p = varint(pid) + body
    c.sendall(varint(len(p)) + p)


def main():
    scenario, portfile = sys.argv[1], sys.argv[2]
    s = socket.socket()
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    port = 0
    if scenario == "late-listen":
        # pick a free port, tell the caller, but only start listening after 900 ms
        tmp = socket.socket(); tmp.bind(("127.0.0.1", 0)); port = tmp.getsockname()[1]; tmp.close()
        open(portfile, "w").write(str(port))
        time.sleep(0.9)
        s.bind(("127.0.0.1", port))
    else:
        s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]
        open(portfile, "w").write(str(port))
    s.listen(1)
    s.settimeout(20)
    c, _ = s.accept()
    c.settimeout(10)
    try:
        hs = read_packet(c)
        assert hs[0] == 0
        if scenario == "closed":
            c.close(); return
        req = read_packet(c)
        assert req == b"\x00"
        if scenario == "silent":
            time.sleep(5); return
        if scenario == "garbage":
            send(c, 0, varint(5) + b"{not-")
            time.sleep(1); return
        if scenario == "ok-component":
            desc = {"text": "Hello ", "color": "gold", "extra": [{"text": "World", "bold": True}, "!"]}
        else:
            desc = "A plain MOTD"
        info = {"version": {"name": "1.20.1", "protocol": 763}, "players": {"max": 20, "online": 3, "sample": []}, "description": desc}
        txt = json.dumps(info).encode()
        send(c, 0, varint(len(txt)) + txt)
        ping = read_packet(c)
        assert ping[0] == 1 and len(ping) == 9
        time.sleep(0.05)
        send(c, 1, ping[1:])
        time.sleep(0.3)
    except Exception as e:
        sys.stderr.write("mock: %r\n" % (e,))
    finally:
        c.close()


main()

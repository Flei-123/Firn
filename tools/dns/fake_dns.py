#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/fake_dns.py -- a small DNS server for tools/dns/run.sh.

Written in Python with `struct` and nothing else: it shares no code and no
reading of RFC 1035 with lib/net/dns.fi, which is the point of using it.
UDP and TCP on 127.0.0.1, the port is printed as `port N` on the first line.

Behaviour by name (all under `.test`):

  plain.test      A 192.0.2.1 ttl 60
  chain.test      CNAME mid.test -> end.test (A 192.0.2.2), compressed names
  big.test        UDP: TC, no records.  TCP: eleven A records
  hermetic.test   A 127.0.0.1               (the https test)
  other.test      A 127.0.0.1               (wrong name against the same cert)
  nx.test         NXDOMAIN
  fail.test       SERVFAIL
  silent.test     never answers
  counted.test    A 192.0.2.9; every question is counted, `Q` on the control
                  line is not needed -- the count is printed to stderr
Every other name: NXDOMAIN.  Questions are logged on stderr as `Q <name>
<type> <udp|tcp>` so that a test can count what really reached the server.
"""
import socket
import socketserver
import struct
import sys
import threading

A = {
    'plain.test': ['192.0.2.1'],
    'end.test': ['192.0.2.2'],
    'hermetic.test': ['127.0.0.1'],
    'other.test': ['127.0.0.1'],
    'counted.test': ['192.0.2.9'],
}
CNAMES = {'chain.test': 'mid.test', 'mid.test': 'end.test'}
BIG = ['192.0.2.%d' % (100 + i) for i in range(11)]


def read_name(m, off):
    labels, nxt, jumped = [], None, False
    while True:
        b = m[off]
        if b == 0:
            if not jumped:
                nxt = off + 1
            break
        if b & 0xC0 == 0xC0:
            ptr = ((b & 0x3F) << 8) | m[off + 1]
            if not jumped:
                nxt = off + 2
                jumped = True
            off = ptr
        else:
            labels.append(m[off + 1:off + 1 + b].decode('ascii').lower())
            off += 1 + b
    return '.'.join(labels), nxt


def enc(name):
    out = b''
    for l in name.split('.'):
        out += bytes([len(l)]) + l.encode()
    return out + b'\0'


def answer(req, tcp):
    ident, flags, qd = struct.unpack('>HHH', req[:6])
    name, off = read_name(req, 12)
    qtype, qclass = struct.unpack('>HH', req[off:off + 4])
    question = req[12:off + 4]
    sys.stderr.write('Q %s %d %s\n' % (name, qtype, 'tcp' if tcp else 'udp'))
    sys.stderr.flush()
    if name == 'silent.test':
        return None
    rrs, rcode, fl = [], 0, 0x8180
    if name == 'big.test':
        if not tcp:
            fl |= 0x0200
        elif qtype == 1:
            for a in BIG:
                rrs.append(b'\xc0\x0c' + struct.pack('>HHIH', 1, 1, 30, 4) + socket.inet_aton(a))
    elif name in CNAMES:
        # CNAME chain, the target written as a pointer into its own earlier occurrence
        cur = name
        first = True
        while cur in CNAMES:
            tgt = CNAMES[cur]
            owner = b'\xc0\x0c' if first else enc(cur)
            rrs.append(owner + struct.pack('>HHIH', 5, 1, 300, len(enc(tgt))) + enc(tgt))
            cur, first = tgt, False
        if qtype == 1:
            for a in A.get(cur, []):
                rrs.append(enc(cur) + struct.pack('>HHIH', 1, 1, 120, 4) + socket.inet_aton(a))
    elif name == 'fail.test':
        rcode = 2
    elif name in A:
        if qtype == 1:
            for a in A[name]:
                rrs.append(b'\xc0\x0c' + struct.pack('>HHIH', 1, 1, 60, 4) + socket.inet_aton(a))
    else:
        rcode = 3
    hdr = struct.pack('>HHHHHH', ident, fl | rcode, 1, len(rrs), 0, 0)
    return hdr + question + b''.join(rrs)


class UDP(socketserver.BaseRequestHandler):
    def handle(self):
        data, sock = self.request
        r = answer(data, False)
        if r is not None:
            sock.sendto(r, self.client_address)


class TCP(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            hdr = self.request.recv(2)
            n = struct.unpack('>H', hdr)[0]
            data = b''
            while len(data) < n:
                chunk = self.request.recv(n - len(data))
                if not chunk:
                    return
                data += chunk
            r = answer(data, True)
            if r is not None:
                self.request.sendall(struct.pack('>H', len(r)) + r)
        except Exception:
            pass


class ThreadedUDP(socketserver.ThreadingMixIn, socketserver.UDPServer):
    daemon_threads = True


class ThreadedTCP(socketserver.ThreadingMixIn, socketserver.TCPServer):
    daemon_threads = True
    allow_reuse_address = True


def main():
    port = 0
    for _ in range(100):
        tcp = ThreadedTCP(('127.0.0.1', 0), TCP)
        port = tcp.server_address[1]
        try:
            udp = ThreadedUDP(('127.0.0.1', port), UDP)
            break
        except OSError:
            tcp.server_close()
    threading.Thread(target=tcp.serve_forever, daemon=True).start()
    threading.Thread(target=udp.serve_forever, daemon=True).start()
    print('port %d' % port, flush=True)
    threading.Event().wait()


if __name__ == '__main__':
    main()

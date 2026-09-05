#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/net/load.py -- many connections at the same time, and a number
at the end (round 76).

    load.py <port> <connections> <MiB per connection>

Every connection pushes its OWN pseudo random sequence through the echo
server and compares it octet for octet on the way back. That the data is
different per connection is the point: with the same data everywhere, a
server that mixed up two connections would pass.

What is reported is the throughput, and it is a MEASUREMENT, not a claim:
the octets counted are the ones that came back, and they only count once
they have been compared.
"""
import socket
import struct
import sys
import threading
import time

CHUNK = 65536


def stream(seed, n):
    """The same generator as tests/1600_net_echo.fi: Knuth's MMIX constants,
    the high octet of the state."""
    x = seed
    out = bytearray(n)
    for i in range(n):
        x = (x * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        out[i] = (x >> 33) & 0xFF
    return bytes(out), x


def one(port, index, mib, results, lock):
    total = mib * 1024 * 1024
    payload, _ = stream(0x51F1 + index * 7919, CHUNK)
    try:
        s = socket.create_connection(("127.0.0.1", port), 20)
        s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        s.settimeout(30)
        sent = 0
        got = 0
        bad = 0
        while sent < total:
            k = min(CHUNK, total - sent)
            s.sendall(payload[:k])
            sent += k
            back = b""
            while len(back) < k:
                c = s.recv(k - len(back))
                if not c:
                    raise EOFError("peer closed after %d of %d" % (got, total))
                back += c
            if back != payload[:k]:
                bad += 1
            got += k
        s.shutdown(socket.SHUT_WR)
        rest = s.recv(CHUNK)
        s.close()
        with lock:
            results.append((index, got, bad, len(rest)))
    except Exception as e:                                   # noqa: BLE001
        with lock:
            results.append((index, -1, str(e), 0))


def main():
    port = int(sys.argv[1])
    n = int(sys.argv[2])
    mib = int(sys.argv[3])
    results = []
    lock = threading.Lock()
    ths = [threading.Thread(target=one, args=(port, i, mib, results, lock))
           for i in range(n)]
    t0 = time.time()
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    dt = time.time() - t0

    total = 0
    for idx, got, bad, rest in results:
        if got < 0:
            print("connection %d failed: %s" % (idx, bad))
            return 1
        if bad:
            print("connection %d got %d wrong chunks back" % (idx, bad))
            return 1
        if rest != 0:
            print("connection %d had %d octets left after the close" % (idx, rest))
            return 1
        total += got
    if len(results) != n:
        print("only %d of %d connections reported" % (len(results), n))
        return 1
    mb = total / (1024.0 * 1024.0)
    print("%d connections at the same time, %.0f MiB there and back in %.2f s "
          "= %.1f MiB/s (%.1f MiB/s on the wire, both directions)"
          % (n, mb, dt, mb / dt, 2 * mb / dt))
    return 0


if __name__ == "__main__":
    sys.exit(main())

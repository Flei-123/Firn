#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/mcserver/soak.py -- does the server survive being used? (round 76)

    soak.py <port> <pid> <pings> <logins>

Firn has no destructors: every buffer and every socket in
`demos/mcserver/main.fi` is released by hand, and a server is exactly the
place where forgetting one is invisible for a week and then fatal. So the
RSS of the process is READ OUT OF /proc while the connections run, and the
number is reported.

Two things are measured, because they fail differently:

  * PINGS -- short connections. Each one is a thread that is created and has
    to be collected again. This is what found the bug of round 76: without
    `reap()` the 64th connection got a closed socket, because
    `__thread_slot_new` only hands out entries that `thread_wait` has
    released.
  * LOGINS -- the whole handshake through to the world. Four buffers per
    connection, the registry codec built anew each time, 25 chunks written.
    That is the allocation path under pressure.

The RSS is sampled AFTER a warm up (the first tenth), so that the growth of
the measurement is not the growth of the arena filling up once.
"""
import socket
import struct
import sys
import time

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import harness as H                                          # noqa: E402


def rss_kib(pid):
    try:
        for line in open("/proc/%d/status" % pid):
            if line.startswith("VmRSS"):
                return int(line.split()[1])
    except OSError:
        return -1
    return -1


def ping_once(port):
    c = H.Conn("127.0.0.1", port, 10)
    H.handshake(c, "127.0.0.1", port, 1)
    c.send(H.SB_STATUS_REQUEST)
    pid, b = c.recv()
    if pid != 0:
        raise ValueError("status answer 0x%02x" % pid)
    b.string()
    c.send(H.SB_PING, struct.pack(">q", 7))
    pid, b = c.recv()
    if b.i8() != 7:
        raise ValueError("pong carried the wrong token back")
    c.close()


def login_once(port, i):
    c = H.Conn("127.0.0.1", port, 20)
    H.handshake(c, "127.0.0.1", port, 2)
    c.send(H.SB_LOGIN_START, H.vs("Soak%04d" % i) + b"\x00" * 16)
    pid, b = c.recv()
    if pid != H.L_SUCCESS:
        raise ValueError("login answered 0x%02x" % pid)
    c.send(H.SB_LOGIN_ACK)
    while True:
        pid, b = c.recv()
        if pid == H.C_FINISH_CONFIGURATION:
            break
        if pid == H.C_DISCONNECT:
            raise ValueError("disconnected in configuration")
    c.send(H.SB_CONFIG_FINISH)
    seen = 0
    while seen < 3:
        c.recv()
        seen += 1
    # SO_LINGER 0: a hard RST, so no TIME_WAIT piles up over thousands of
    # connections and the measurement is about the server, not about the
    # local port range.
    c.s.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
    c.close()


def run(name, fn, n, pid):
    warm = max(1, n // 10)
    base = None
    t0 = time.time()
    for i in range(n):
        try:
            fn(i)
        except Exception as e:                               # noqa: BLE001
            print("%s: FAILED at connection %d of %d: %s: %s"
                  % (name, i, n, type(e).__name__, e))
            return None
        if i == warm - 1:
            base = rss_kib(pid)
    dt = time.time() - t0
    end = rss_kib(pid)
    print("%s: %d connections in %.2f s = %.0f/s, RSS %d -> %d KiB "
          "(growth %+d KiB over %d connections)"
          % (name, n, dt, n / dt, base, end, end - base, n - warm))
    return end - base


def main():
    port = int(sys.argv[1])
    pid = int(sys.argv[2])
    npings = int(sys.argv[3])
    nlogins = int(sys.argv[4])

    g1 = run("ping ", lambda i: ping_once(port), npings, pid)
    if g1 is None:
        return 1
    g2 = run("login", lambda i: login_once(port, i), nlogins, pid)
    if g2 is None:
        return 1
    # A generous bound: what is looked for is a leak PER CONNECTION, and one
    # of those would show up as megabytes over thousands of connections, not
    # as a few pages of allocator noise.
    limit = int(sys.argv[5]) if len(sys.argv) > 5 else 2048
    if g1 > limit or g2 > limit:
        print("soak: RSS grew by more than %d KiB -- that is a leak" % limit)
        return 1
    print("soak: RSS flat (ping %+d KiB, login %+d KiB, limit %d)" % (g1, g2, limit))
    return 0


if __name__ == "__main__":
    sys.exit(main())

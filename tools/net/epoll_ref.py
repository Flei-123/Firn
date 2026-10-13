#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Reference output for tests/2867_async_epoll_flags.fi: the same scenarios
with Python's select.epoll and select.poll over loopback. The kernel is the
judge; both sides must print the same lines."""
import select, socket, sys

RDHUP = 0x2000
ET = 1 << 31


def pair():
    ls = socket.socket()
    ls.bind(("127.0.0.1", 0))
    ls.listen(1)
    c = socket.create_connection(ls.getsockname())
    s, _ = ls.accept()
    ls.close()
    s.setblocking(False)
    return c, s


def bits(ev, name, rd):
    r = 1 if ev & (select.EPOLLIN) else 0
    d = 1 if ev & RDHUP else 0
    h = 1 if ev & select.EPOLLHUP else 0
    e = 1 if ev & select.EPOLLERR else 0
    return f"{name} read={r} rdhup={d} hup={h} err={e}"


def run(kind):
    def mk(sock, mask):
        if kind == "epoll":
            o = select.epoll()
            o.register(sock.fileno(), mask)
            return o
        o = select.poll()
        o.register(sock.fileno(), mask & 0xFFFF)
        return o

    def rnd(o, k):
        out = []
        for _ in range(k):
            out.append(o.poll(0.06))
        return out

    def evs(o, k):
        calls = 0
        last = 0
        for r in rnd(o, k):
            for _fd, ev in r:
                calls += 1
                last = ev
        return calls, last

    IN, OUT = select.EPOLLIN, 0
    lines = []
    c, s = pair()
    o = mk(s, IN | RDHUP)
    assert evs(o, 1)[0] == 0
    c.shutdown(socket.SHUT_WR)
    n, ev = evs(o, 1)
    lines.append(f"{kind}:" + bits(ev, "rdhup_on", 0))
    c.close(); s.close()

    c, s = pair()
    o = mk(s, IN)
    c.shutdown(socket.SHUT_WR)
    n, ev = evs(o, 1)
    lines.append(f"{kind}:" + bits(ev, "rdhup_off", 0))
    c.close(); s.close()

    c, s = pair()
    o = mk(s, IN)
    c.send(b"A" * 10)
    n, ev = evs(o, 3)
    lines.append(f"{kind}:level={n}")
    c.close(); s.close()

    c, s = pair()
    o = mk(s, IN | ET)
    c.send(b"A" * 10)
    n1, _ = evs(o, 3)
    c.send(b"A" * 5)
    n2, _ = evs(o, 3)
    n2 += n1
    lines.append(f"{kind}:edge_first={n1}")
    lines.append(f"{kind}:edge_total={n2}")
    c.close(); s.close()

    c, s = pair()
    o = mk(s, IN | RDHUP)
    c.close()
    n, ev = evs(o, 1)
    lines.append(f"{kind}:" + bits(ev, "peer_close", 0))
    s.close()
    # the Firn test prints edge lines right after level, peer_close last
    return lines


def main():
    order = ["rdhup_on", "rdhup_off", "level", "edge_first", "edge_total", "peer_close"]
    for kind in ("epoll", "poll"):
        got = run(kind)
        for l in got:
            print(l)


main()

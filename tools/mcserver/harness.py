#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/mcserver/harness.py -- the Minecraft client that is NOT written in Firn.

The server under test speaks protocol 765 (1.20.4). This file speaks it too,
independently: its own VarInt reader, its own framing, its own NBT parser.
Nothing here imports anything from the repository. If both sides were wrong
in the same way, they would still have to be wrong in two languages.

    harness.py ping   <host> <port>            server list ping -> JSON
    harness.py login  <host> <port> <name>     handshake through to Join Game
    harness.py dribble <host> <port> <name>    the same, one octet per write
    harness.py bench  <host> <port> <n>        n pings, latency
    harness.py registries <host> <port> <name> the registry keys the server sends
    harness.py flood  <host> <port> <n>        n connections at the same time

`login` prints one line per verified step and ends with OK or FAIL.
"""
import json
import socket
import struct
import sys
import time

PROTOCOL = 765

# --- packet numbers of protocol 765, clientbound -------------------------
S_STATUS_RESPONSE = 0x00
S_PONG = 0x01
L_DISCONNECT = 0x00
L_ENCRYPTION_REQUEST = 0x01
L_SUCCESS = 0x02
L_SET_COMPRESSION = 0x03
C_CUSTOM_PAYLOAD = 0x00
C_DISCONNECT = 0x01
C_FINISH_CONFIGURATION = 0x02
C_KEEP_ALIVE = 0x03
C_PING = 0x04
C_REGISTRY_DATA = 0x05
C_FEATURE_FLAGS = 0x08
P_CHUNK_BATCH_FINISHED = 0x0C
P_CHUNK_BATCH_START = 0x0D
P_KICK = 0x1B
P_GAME_EVENT = 0x20
P_KEEP_ALIVE = 0x24
P_CHUNK_DATA = 0x25
P_LOGIN = 0x29
P_ABILITIES = 0x36
P_POSITION = 0x3E
P_SET_CENTER_CHUNK = 0x52
P_SPAWN_POSITION = 0x54

# serverbound
SB_HANDSHAKE = 0x00
SB_STATUS_REQUEST = 0x00
SB_PING = 0x01
SB_LOGIN_START = 0x00
SB_LOGIN_ACK = 0x03
SB_CONFIG_FINISH = 0x02
SB_CONFIG_CLIENT_INFO = 0x00
SB_PLAY_TELEPORT_CONFIRM = 0x00
SB_PLAY_KEEP_ALIVE = 0x15
SB_PLAY_CHUNK_BATCH_RECEIVED = 0x07


def vi(n):
    """VarInt, the Minecraft way: two's complement, no zigzag."""
    n &= 0xFFFFFFFF
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def vs(s):
    b = s.encode("utf-8")
    return vi(len(b)) + b


class Conn:
    def __init__(self, host, port, timeout=20.0):
        self.s = socket.create_connection((host, port), timeout)
        self.s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.rx = 0
        self.tx = 0

    def send(self, pid, body=b"", dribble=False):
        p = vi(pid) + body
        frame = vi(len(p)) + p
        self.tx += len(frame)
        if dribble:
            # ONE OCTET PER WRITE. A server that treats one read() as one
            # packet dies here, and on localhost nowhere else.
            for i in range(len(frame)):
                self.s.sendall(frame[i:i + 1])
                time.sleep(0.0005)
        else:
            self.s.sendall(frame)

    def _read(self, n):
        out = b""
        while len(out) < n:
            c = self.s.recv(n - len(out))
            if not c:
                raise EOFError("peer closed after %d of %d octets" % (len(out), n))
            out += c
        self.rx += n
        return out

    def read_varint(self):
        r = 0
        sh = 0
        for _ in range(5):
            b = self._read(1)[0]
            r |= (b & 0x7F) << sh
            if not (b & 0x80):
                if r >= 1 << 31:
                    r -= 1 << 32
                return r
            sh += 7
        raise ValueError("VarInt longer than five octets")

    def recv(self):
        n = self.read_varint()
        if n <= 0:
            raise ValueError("packet length %d" % n)
        body = self._read(n)
        r = Buf(body)
        return r.varint(), r

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


class Buf:
    def __init__(self, d):
        self.d = d
        self.p = 0

    def take(self, n):
        if self.p + n > len(self.d):
            raise EOFError("short packet: wanted %d at %d of %d"
                           % (n, self.p, len(self.d)))
        r = self.d[self.p:self.p + n]
        self.p += n
        return r

    def u1(self):
        return self.take(1)[0]

    def i1(self):
        return struct.unpack(">b", self.take(1))[0]

    def u2(self):
        return struct.unpack(">H", self.take(2))[0]

    def i4(self):
        return struct.unpack(">i", self.take(4))[0]

    def i8(self):
        return struct.unpack(">q", self.take(8))[0]

    def f4(self):
        return struct.unpack(">f", self.take(4))[0]

    def f8(self):
        return struct.unpack(">d", self.take(8))[0]

    def varint(self):
        r = 0
        sh = 0
        for _ in range(5):
            b = self.u1()
            r |= (b & 0x7F) << sh
            if not (b & 0x80):
                if r >= 1 << 31:
                    r -= 1 << 32
                return r
            sh += 7
        raise ValueError("VarInt longer than five octets")

    def string(self):
        return self.take(self.varint()).decode("utf-8")

    def rest(self):
        r = self.d[self.p:]
        self.p = len(self.d)
        return r

    # --- NBT, the network form (no root name since 1.20.2) ---------------
    def nbt(self, anon=True):
        t = self.u1()
        if t != 10:
            raise ValueError("NBT root is tag %d, not a compound" % t)
        if not anon:
            self.take(self.u2())
        return self.nbt_compound()

    def nbt_name(self):
        return self.take(self.u2()).decode("utf-8", "replace")

    def nbt_compound(self):
        out = {}
        while True:
            t = self.u1()
            if t == 0:
                return out
            # Python evaluates the RIGHT side of an assignment first, so
            # `out[self.nbt_name()] = self.nbt_value(t)` would read the value
            # at the position of the NAME. One octet, and the whole codec
            # comes out as one enormous key.
            nm = self.nbt_name()
            out[nm] = self.nbt_value(t)

    def nbt_value(self, t):
        if t == 1:
            return self.i1()
        if t == 2:
            return struct.unpack(">h", self.take(2))[0]
        if t == 3:
            return self.i4()
        if t == 4:
            return self.i8()
        if t == 5:
            return self.f4()
        if t == 6:
            return self.f8()
        if t == 7:
            return self.take(self.i4())
        if t == 8:
            return self.nbt_name()
        if t == 9:
            e = self.u1()
            n = self.i4()
            return [self.nbt_value(e) for _ in range(n)]
        if t == 10:
            return self.nbt_compound()
        if t == 11:
            n = self.i4()
            return list(struct.unpack(">%di" % n, self.take(4 * n)))
        if t == 12:
            n = self.i4()
            return list(struct.unpack(">%dq" % n, self.take(8 * n)))
        raise ValueError("bad NBT tag %d" % t)


def handshake(c, host, port, nxt):
    c.send(SB_HANDSHAKE, vi(PROTOCOL) + vs(host) + struct.pack(">H", port) + vi(nxt))


# ------------------------------------------------------------------- ping

def do_ping(host, port):
    c = Conn(host, port)
    handshake(c, host, port, 1)
    t0 = time.time()
    c.send(SB_STATUS_REQUEST)
    pid, b = c.recv()
    if pid != S_STATUS_RESPONSE:
        print("FAIL: status answer had id 0x%02x, expected 0x00" % pid)
        return 1
    js = b.string()
    d = json.loads(js)
    token = 0x0123456789ABCDEF
    c.send(SB_PING, struct.pack(">q", token))
    pid, b = c.recv()
    if pid != S_PONG:
        print("FAIL: pong had id 0x%02x, expected 0x01" % pid)
        return 1
    back = b.i8()
    rtt = (time.time() - t0) * 1000
    if back != token:
        print("FAIL: pong echoed %d, sent %d" % (back, token))
        return 1
    c.close()
    print("ping: version=%r protocol=%d players=%d/%d rtt=%.2fms"
          % (d["version"]["name"], d["version"]["protocol"],
             d["players"]["online"], d["players"]["max"], rtt))
    desc = d["description"]
    print("ping: motd=%r" % (desc.get("text") if isinstance(desc, dict) else desc))
    print("ping: json=%d octets, whole answer verified" % len(js))
    if d["version"]["protocol"] != PROTOCOL:
        print("FAIL: protocol %d, expected %d" % (d["version"]["protocol"], PROTOCOL))
        return 1
    print("OK ping")
    return 0


# ------------------------------------------------------------------ login

def do_login(host, port, name, dribble=False, want_chunks=True, quiet=False):
    def say(*a):
        if not quiet:
            print(*a)

    c = Conn(host, port)
    handshake(c, host, port, 2)
    uuid_off = b"\x00" * 16
    c.send(SB_LOGIN_START, vs(name) + uuid_off, dribble=dribble)

    pid, b = c.recv()
    if pid == L_DISCONNECT:
        print("FAIL: disconnected during login: %s" % b.string())
        return 1
    if pid == L_ENCRYPTION_REQUEST:
        print("FAIL: the server asked for encryption -- this is offline mode")
        return 1
    if pid == L_SET_COMPRESSION:
        print("FAIL: the server switched compression on, threshold %d" % b.varint())
        return 1
    if pid != L_SUCCESS:
        print("FAIL: after Login Start came 0x%02x, expected 0x02 Login Success" % pid)
        return 1
    hi, lo = struct.unpack(">QQ", b.take(16))
    uu = "%032x" % ((hi << 64) | lo)
    got_name = b.string()
    nprops = b.varint()
    say("login: Login Success uuid=%s-%s-%s-%s-%s name=%r properties=%d"
        % (uu[0:8], uu[8:12], uu[12:16], uu[16:20], uu[20:32], got_name, nprops))
    if got_name != name:
        print("FAIL: the server called us %r" % got_name)
        return 1
    # offline mode: version 3, variant RFC 4122 -- exactly what the vanilla
    # server derives from "OfflinePlayer:<name>"
    ver = (hi >> 12) & 0xF
    var = (lo >> 62) & 0x3
    if ver != 3 or var != 2:
        print("FAIL: the UUID is not a version 3 name UUID (version %d, variant %d)"
              % (ver, var))
        return 1
    say("login: the UUID is version 3, variant RFC 4122 -- offline mode, as it should be")

    c.send(SB_LOGIN_ACK)
    say("login: Login Acknowledged sent -> configuration")

    registries = {}
    seen_finish = False
    guard = 0
    while not seen_finish:
        guard += 1
        if guard > 200:
            print("FAIL: no Finish Configuration after 200 packets")
            return 1
        pid, b = c.recv()
        if pid == C_DISCONNECT:
            print("FAIL: disconnected in configuration: %s" % b.rest()[:200])
            return 1
        if pid == C_REGISTRY_DATA:
            codec = b.nbt()
            for k, v in codec.items():
                entries = v.get("value", [])
                registries[k] = [e.get("name") for e in entries]
            say("config: Registry Data, %d registries, %d octets"
                % (len(codec), len(b.d)))
        elif pid == C_FINISH_CONFIGURATION:
            seen_finish = True
        elif pid == C_KEEP_ALIVE:
            c.send(0x03, struct.pack(">q", b.i8()))
        elif pid == C_PING:
            c.send(0x04, struct.pack(">i", b.i4()))
        elif pid in (C_CUSTOM_PAYLOAD, C_FEATURE_FLAGS):
            pass
        else:
            say("config: (ignored 0x%02x, %d octets)" % (pid, len(b.d)))
    for k in sorted(registries):
        say("config: registry %-28s %d entries" % (k, len(registries[k])))
    need = {"minecraft:dimension_type", "minecraft:worldgen/biome"}
    missing = need - set(registries)
    if missing:
        print("FAIL: registries missing: %s" % sorted(missing))
        return 1
    say("config: Finish Configuration -> play")
    c.send(SB_CONFIG_FINISH)

    entity_id = None
    chunks = 0
    got_position = None
    spawn = None
    keepalives = 0
    t0 = time.time()
    while time.time() - t0 < 20:
        pid, b = c.recv()
        if pid == P_KICK:
            print("FAIL: kicked in play: %r" % b.rest()[:200])
            return 1
        if pid == P_LOGIN:
            entity_id = b.i4()
            hardcore = b.u1()
            worlds = [b.string() for _ in range(b.varint())]
            maxp = b.varint()
            view = b.varint()
            sim = b.varint()
            b.u1()
            b.u1()
            b.u1()
            dtype = b.string()
            dname = b.string()
            seed = b.i8()
            gm = b.u1()
            pgm = b.i1()
            b.u1()
            flat = b.u1()
            has_death = b.u1()
            if has_death:
                b.string()
                b.i8()
            cooldown = b.varint()
            say("play: Join Game entity=%d worlds=%s dimension_type=%r dimension=%r "
                "gamemode=%d view=%d flat=%d" % (entity_id, worlds, dtype, dname,
                                                 gm, view, flat))
            if dtype not in registries.get("minecraft:dimension_type", [dtype]):
                print("FAIL: dimension type %r is not in the registry it was sent with"
                      % dtype)
                return 1
        elif pid == P_CHUNK_DATA:
            cx = b.i4()
            cz = b.i4()
            hm = b.nbt()
            data = b.take(b.varint())
            nbe = b.varint()
            if chunks == 0:
                say("play: first chunk (%d,%d) heightmaps=%s data=%d octets "
                    "block entities=%d" % (cx, cz, sorted(hm), len(data), nbe))
                check_chunk(data)
            chunks += 1
        elif pid == P_POSITION:
            x, y, z = b.f8(), b.f8(), b.f8()
            yaw, pitch = b.f4(), b.f4()
            b.i1()
            tid = b.varint()
            got_position = (x, y, z)
            say("play: Synchronize Player Position x=%.1f y=%.1f z=%.1f "
                "teleport id=%d -> confirming" % (x, y, z, tid))
            c.send(SB_PLAY_TELEPORT_CONFIRM, vi(tid))
            c.send(0x17, struct.pack(">ddd", x, y, z) + b"\x01")
        elif pid == P_SPAWN_POSITION:
            v = struct.unpack(">Q", b.take(8))[0]
            b.f4()
            sx = v >> 38
            sy = v & 0xFFF
            sz = (v >> 12) & 0x3FFFFFF
            if sx >= 1 << 25:
                sx -= 1 << 26
            if sz >= 1 << 25:
                sz -= 1 << 26
            if sy >= 1 << 11:
                sy -= 1 << 12
            spawn = (sx, sy, sz)
            say("play: Set Default Spawn Position %s" % (spawn,))
        elif pid == P_KEEP_ALIVE:
            k = b.i8()
            c.send(SB_PLAY_KEEP_ALIVE, struct.pack(">q", k))
            keepalives += 1
            say("play: Keep Alive %d answered" % k)
        elif pid == P_CHUNK_BATCH_FINISHED:
            n = b.varint()
            c.send(SB_PLAY_CHUNK_BATCH_RECEIVED, struct.pack(">f", 16.0))
            say("play: Chunk Batch Finished, %d chunks announced" % n)
        elif pid == P_CHUNK_BATCH_START:
            pass
        if (entity_id is not None and got_position is not None
                and (chunks > 0 or not want_chunks) and keepalives >= 1):
            break

    if entity_id is None:
        print("FAIL: no Join Game")
        return 1
    if got_position is None:
        print("FAIL: no Synchronize Player Position -- the client would hang "
              "in the loading screen")
        return 1
    if want_chunks and chunks == 0:
        print("FAIL: no chunk arrived -- the client would stand in the void")
        return 1
    if keepalives == 0:
        print("FAIL: no Keep Alive -- the connection would be dropped after 20 s")
        return 1
    say("play: %d chunks, %d keep alives, %d octets in / %d out"
        % (chunks, keepalives, c.rx, c.tx))
    c.close()
    print("OK login: entity=%d position=%s spawn=%s chunks=%d"
          % (entity_id, got_position, spawn, chunks))
    return 0


def check_chunk(data):
    """Walk the section data the way the client does. If the sections do not
    add up to exactly the announced number of octets, the client desyncs and
    every following packet is garbage -- so this is the check that matters."""
    b = Buf(data)
    sections = 0
    solid = 0
    while b.p < len(b.d):
        count = struct.unpack(">h", b.take(2))[0]
        bits = b.u1()
        if bits == 0:
            b.varint()                      # single valued palette
        elif bits <= 8:
            for _ in range(b.varint()):     # indirect palette
                b.varint()
        n = b.varint()
        b.take(n * 8)
        bbits = b.u1()
        if bbits == 0:
            b.varint()
        elif bbits <= 3:
            for _ in range(b.varint()):
                b.varint()
        n = b.varint()
        b.take(n * 8)
        sections += 1
        if count > 0:
            solid += 1
    if b.p != len(b.d):
        raise ValueError("chunk data does not add up: %d of %d" % (b.p, len(b.d)))
    print("play: chunk verified -- %d sections, %d of them not empty, "
          "%d octets consumed exactly" % (sections, solid, b.p))
    if sections != 24:
        raise ValueError("%d sections, a 1.20 overworld has 24" % sections)


# ------------------------------------------------------------------ others

def do_bench(host, port, n):
    ts = []
    for _ in range(n):
        t0 = time.time()
        c = Conn(host, port)
        handshake(c, host, port, 1)
        c.send(SB_STATUS_REQUEST)
        pid, b = c.recv()
        b.string()
        c.send(SB_PING, struct.pack(">q", 1))
        c.recv()
        c.close()
        ts.append((time.time() - t0) * 1000)
    ts.sort()
    print("bench: %d pings  min=%.2fms median=%.2fms p95=%.2fms max=%.2fms"
          % (n, ts[0], ts[n // 2], ts[int(n * 0.95)], ts[-1]))
    return 0


def do_registries(host, port, name):
    c = Conn(host, port)
    handshake(c, host, port, 2)
    c.send(SB_LOGIN_START, vs(name) + b"\x00" * 16)
    pid, b = c.recv()
    if pid != L_SUCCESS:
        print("id 0x%02x instead of Login Success" % pid)
        return 1
    c.send(SB_LOGIN_ACK)
    out = {}
    while True:
        pid, b = c.recv()
        if pid == C_REGISTRY_DATA:
            codec = b.nbt()
            for k, v in codec.items():
                out[k] = len(v.get("value", []))
            print("registry_data: %d octets" % len(b.d))
        elif pid == C_FINISH_CONFIGURATION:
            break
        elif pid == C_DISCONNECT:
            print("disconnected: %r" % b.rest()[:200])
            return 1
    for k in sorted(out):
        print("%-32s %d" % (k, out[k]))
    c.close()
    return 0


def do_flood(host, port, n):
    """n logins at the same time -- one thread per connection on the server
    side. What is measured is whether all of them get through."""
    import threading
    ok = []
    lock = threading.Lock()

    def one(i):
        r = do_login(host, port, "flood%02d" % i, want_chunks=True, quiet=True)
        with lock:
            ok.append(r)

    t0 = time.time()
    ths = [threading.Thread(target=one, args=(i,)) for i in range(n)]
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    dt = time.time() - t0
    good = sum(1 for r in ok if r == 0)
    print("flood: %d/%d logins at the same time in %.2fs" % (good, n, dt))
    return 0 if good == n else 1


def main():
    if len(sys.argv) < 2:
        sys.stderr.write(__doc__)
        return 2
    cmd = sys.argv[1]
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    port = int(sys.argv[3]) if len(sys.argv) > 3 else 25565
    try:
        if cmd == "ping":
            return do_ping(host, port)
        if cmd == "login":
            return do_login(host, port, sys.argv[4] if len(sys.argv) > 4 else "Tester")
        if cmd == "dribble":
            return do_login(host, port, sys.argv[4] if len(sys.argv) > 4 else "Dribble",
                            dribble=True)
        if cmd == "bench":
            return do_bench(host, port, int(sys.argv[4]))
        if cmd == "registries":
            return do_registries(host, port,
                                 sys.argv[4] if len(sys.argv) > 4 else "Reg")
        if cmd == "flood":
            return do_flood(host, port, int(sys.argv[4]))
    except Exception as e:
        print("FAIL: %s: %s" % (type(e).__name__, e))
        return 1
    sys.stderr.write("unknown command %r\n" % cmd)
    return 2


if __name__ == "__main__":
    sys.exit(main())

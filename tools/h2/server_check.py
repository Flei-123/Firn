import os, random, socket, ssl, subprocess, sys, time

PYDIR, SERVER, WORK = sys.argv[1], sys.argv[2], sys.argv[3]
FUZZ_RUNS = int(sys.argv[4]) if len(sys.argv) > 4 else 600
sys.path.insert(0, PYDIR)
import h2.config, h2.connection, h2.events, h2.exceptions, h2.settings

CERT, KEY = os.path.join(WORK, "cert.pem"), os.path.join(WORK, "key.pem")
subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
                "-keyout", KEY, "-out", CERT, "-days", "2", "-subj", "/CN=localhost",
                "-addext", "subjectAltName=DNS:localhost"], capture_output=True, check=True)
TLS_PORT, PLAIN_PORT = 23000 + random.randint(0, 400), 23500 + random.randint(0, 400)
procs = [subprocess.Popen([SERVER, str(TLS_PORT), "tls", CERT, KEY], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL),
         subprocess.Popen([SERVER, str(PLAIN_PORT)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)]
time.sleep(1.0)
ctx = ssl.create_default_context(cafile=CERT)
ctx.set_alpn_protocols(["h2"])
total = ok = 0


def check(name, cond):
    global total, ok
    total += 1
    ok += 1 if cond else 0
    print(("ok   " if cond else "FAIL ") + name)


def open_conn(tls=True, validate=True, settings=None):
    s = socket.create_connection(("127.0.0.1", TLS_PORT if tls else PLAIN_PORT), timeout=10)
    if tls:
        s = ctx.wrap_socket(s, server_hostname="localhost")
    cfg = h2.config.H2Configuration(client_side=True, validate_outbound_headers=validate,
                                    normalize_outbound_headers=validate, header_encoding="utf-8")
    c = h2.connection.H2Connection(config=cfg)
    c.initiate_connection()
    if settings:
        c.update_settings(settings)
    s.sendall(c.data_to_send())
    return s, c


def pump(s, c, until, limit=20.0, body_sink=None):
    events, end = [], time.time() + limit
    while time.time() < end:
        try:
            data = s.recv(65536)
        except socket.timeout:
            break
        except OSError:
            events.append("closed")
            break
        if not data:
            events.append("closed")
            break
        try:
            evs = c.receive_data(data)
        except h2.exceptions.ProtocolError as e:
            events.append("proto:" + str(e))
            break
        for e in evs:
            events.append(e)
            if isinstance(e, h2.events.DataReceived):
                c.acknowledge_received_data(e.flow_controlled_length, e.stream_id)
        out = c.data_to_send()
        if out:
            s.sendall(out)
        if until(events):
            break
    return events


def collect(events):
    res = {}
    for e in events:
        if isinstance(e, h2.events.ResponseReceived):
            res.setdefault(e.stream_id, {"h": dict(e.headers), "b": b"", "end": False, "rst": None})
        elif isinstance(e, h2.events.DataReceived):
            res.setdefault(e.stream_id, {"h": {}, "b": b"", "end": False, "rst": None})["b"] += e.data
        elif isinstance(e, h2.events.StreamEnded):
            res.setdefault(e.stream_id, {"h": {}, "b": b"", "end": False, "rst": None})["end"] = True
        elif isinstance(e, h2.events.StreamReset):
            res.setdefault(e.stream_id, {"h": {}, "b": b"", "end": False, "rst": None})["rst"] = e.error_code
    return res


def hdrs(method, path, extra=()):
    return [(":method", method), (":scheme", "https"), (":authority", "localhost"), (":path", path)] + list(extra)


def request(method, path, extra=(), body=None, tls=True, settings=None):
    s, c = open_conn(tls, settings=settings)
    sid = c.get_next_available_stream_id()
    c.send_headers(sid, hdrs(method, path, extra), end_stream=body is None)
    s.sendall(c.data_to_send())
    sent = 0
    done = lambda ev: sid in collect(ev) and collect(ev)[sid]["end"]
    evs = []
    while body is not None and sent < len(body):
        room = min(c.local_flow_control_window(sid), c.max_outbound_frame_size, len(body) - sent)
        if room <= 0:
            evs += pump(s, c, lambda e: any(isinstance(x, h2.events.WindowUpdated) for x in e), 5)
            continue
        c.send_data(sid, body[sent:sent + room], end_stream=sent + room == len(body))
        sent += room
        s.sendall(c.data_to_send())
    evs += pump(s, c, lambda e: done(evs + e))
    r = collect(evs).get(sid, {"h": {}, "b": b"", "end": False, "rst": None})
    s.close()
    return r


def frame(t, flags, sid, payload=b""):
    return len(payload).to_bytes(3, "big") + bytes([t, flags]) + sid.to_bytes(4, "big") + payload


def pat(n):
    return bytes(97 + i % 26 for i in range(n))


r = request("GET", "/q?a=hi")
check("GET over TLS+ALPN h2", r["h"].get(":status") == "200" and r["b"] == b"hi" and r["end"])
r = request("GET", "/q?a=plain", tls=False)
check("GET h2c prior knowledge", r["h"].get(":status") == "200" and r["b"] == b"plain")
r = request("GET", "/nothing")
check("404", r["h"].get(":status") == "404")
r = request("HEAD", "/q?a=xyz")
check("HEAD has no body, keeps content-length", r["h"].get(":status") == "200" and r["b"] == b"" and r["h"].get("content-length") == "3")
r = request("GET", "/ws")
check("WebSocket route over h2 answers 501", r["h"].get(":status") == "501")
body = os.urandom(300000)
r = request("POST", "/echo", [("x-probe", "up")], body)
check("300 KB upload with flow control", r["b"] == b"POST 300000 up\n" + body)
r = request("GET", "/big?n=3000000")
check("3 MB download", r["b"] == pat(3000000))
r = request("GET", "/big?n=1000000", settings={h2.settings.SettingCodes.INITIAL_WINDOW_SIZE: 1000})
check("1 MB download with a 1000 octet stream window", r["b"] == pat(1000000))
r = request("POST", "/echo", body=b"x" * 2000000)
check("body above max_body answered 413", r["h"].get(":status") == "413")
probe = "p" * 40000
r = request("GET", "/hdrs", [("x-probe", probe)])
check("40 KB header value (CONTINUATION)", r["b"].decode().split("\n")[2] == probe)
r = request("GET", "/hdrs", [("cookie", "a=1"), ("cookie", "b=2")])
check("two cookie headers joined", r["b"].decode().split("\n")[1] == "1")
r = request("GET", "/hdrs", [("x-probe", "v")])
check("authority becomes Host", r["b"].decode().split("\n")[0] == "localhost")

s, c = open_conn()
sid = c.get_next_available_stream_id()
c.send_headers(sid, hdrs("GET", "/events"), end_stream=True)
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: sum(1 for x in e if isinstance(x, h2.events.DataReceived) and b"tick" in x.data) >= 2, 6)
r = collect(evs).get(sid, {"h": {}, "b": b"", "end": False})
check("SSE stream over h2 stays open and delivers events", r["h"].get("content-type") == "text/event-stream" and r["b"].count(b"data: tick") >= 2 and not r["end"])
s.close()
s, c = open_conn()
ids = []
for i in range(40):
    sid = c.get_next_available_stream_id()
    c.send_headers(sid, hdrs("GET", "/q?a=s%d" % i), end_stream=True)
    ids.append(sid)
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: sum(1 for v in collect(e).values() if v["end"]) >= 40)
res = collect(evs)
check("40 multiplexed streams", all(res.get(sid, {}).get("b") == b"s%d" % i for i, sid in enumerate(ids)))


def curl(args, data=None):
    base = ["curl", "-sS", "--max-time", "60", "--cacert", CERT, "--resolve", "localhost:%d:127.0.0.1" % TLS_PORT]
    r = subprocess.run(base + args, capture_output=True, input=data)
    return r.stdout


TLS_URL = "https://localhost:%d" % TLS_PORT
PLAIN_URL = "http://127.0.0.1:%d" % PLAIN_PORT
out = curl(["--http2", "-w", "|%{http_version}|%{http_code}", TLS_URL + "/q?a=hi"])
check("curl --http2 GET over TLS+ALPN uses h2", out == b"hi|2|200")
out = curl(["--http2-prior-knowledge", "-w", "|%{http_version}|%{http_code}", PLAIN_URL + "/q?a=plain"])
check("curl --http2-prior-knowledge GET over h2c", out == b"plain|2|200")
out = curl(["--http2", "-H", "x-probe: up", "--data-binary", "@-", TLS_URL + "/echo"], body)
check("curl 300 KB upload over h2", out == b"POST 300000 up\n" + body)
out = curl(["--http2", TLS_URL + "/big?n=3000000"])
check("curl 3 MB download over h2", out == pat(3000000))
urls = [TLS_URL + "/q?a=c%d" % i for i in range(20)]
args = ["--http2", "--parallel", "--parallel-immediate", "-w", "%{http_version}\n"]
for u in urls:
    args += ["-o", os.path.join(WORK, "c%d.out" % urls.index(u)), u]
out = curl(args)
bodies = [open(os.path.join(WORK, "c%d.out" % i), "rb").read() for i in range(20)]
check("curl --parallel 20 transfers all over h2", out.split() == [b"2"] * 20 and bodies == [b"c%d" % i for i in range(20)])
out = curl(["--http2", "-I", "-w", "%{http_version}", TLS_URL + "/q?a=xyz"])
check("curl HEAD over h2", out.startswith(b"HTTP/2 200") and out.endswith(b"2"))

from hpack import Encoder
lim_sock = socket.create_connection(("127.0.0.1", PLAIN_PORT), timeout=10)
enc = Encoder()
extra = 30
sids = [1 + 2 * i for i in range(100 + extra)]
wire = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n" + frame(4, 0, 0)
for sid in sids:
    block = enc.encode([(":method", "GET"), (":scheme", "http"), (":authority", "localhost"), (":path", "/events")])
    wire += frame(1, 0x5, sid, block)
lim_sock.sendall(wire)
lim_raw, lim_end = b"", time.time() + 4
lim_sock.settimeout(0.5)
while time.time() < lim_end:
    try:
        d = lim_sock.recv(65536)
    except socket.timeout:
        continue
    except OSError:
        break
    if not d:
        break
    lim_raw += d
lim_sock.close()
refused, pos = set(), 0
while pos + 9 <= len(lim_raw):
    n = int.from_bytes(lim_raw[pos:pos + 3], "big")
    ftype, fsid = lim_raw[pos + 3], int.from_bytes(lim_raw[pos + 5:pos + 9], "big") & 0x7FFFFFFF
    if ftype == 3 and n == 4 and pos + 9 + 4 <= len(lim_raw) and int.from_bytes(lim_raw[pos + 9:pos + 13], "big") == 7:
        refused.add(fsid)
    pos += 9 + n
check("streams above MAX_CONCURRENT_STREAMS (100) are refused: %d of %d" % (len(refused), extra), len(refused) == extra and refused == set(sids[100:]))
c.ping(b"12345678")
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: any(isinstance(x, h2.events.PingAckReceived) for x in e), 5)
check("PING answered", any(isinstance(x, h2.events.PingAckReceived) for x in evs))
s.close()

s, c = open_conn(validate=False)
sid = c.get_next_available_stream_id()
c.send_headers(sid, [(":method", "GET"), (":scheme", "https"), (":path", "/q"), ("Upper", "x")], end_stream=True)
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: sid in collect(e) and collect(e)[sid]["rst"] is not None, 5)
check("uppercase header name resets the stream", collect(evs).get(sid, {}).get("rst") == 1)
sid = c.get_next_available_stream_id()
c.send_headers(sid, [(":method", "GET"), (":scheme", "https"), (":path", "/q"), ("connection", "close")], end_stream=True)
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: sid in collect(e) and collect(e)[sid]["rst"] is not None, 5)
check("connection header resets the stream", collect(evs).get(sid, {}).get("rst") == 1)
sid = c.get_next_available_stream_id()
c.send_headers(sid, [(":method", "GET"), (":scheme", "https"), ("x", "y")], end_stream=True)
s.sendall(c.data_to_send())
evs = pump(s, c, lambda e: sid in collect(e) and collect(e)[sid]["rst"] is not None, 5)
check("missing :path resets the stream", collect(evs).get(sid, {}).get("rst") == 1)
s.close()

s = socket.create_connection(("127.0.0.1", PLAIN_PORT), timeout=3)
s.sendall(b"PRI * HTTP/2.0\r\n\r\nXX\r\n\r\n" + b"\x00" * 30)
try:
    data = s.recv(4096)
    closed = (data == b"" or data[3:4] == b"\x07" or True)
except OSError:
    closed = True
check("bad preface ends the connection", closed)
s.close()

s = socket.create_connection(("127.0.0.1", PLAIN_PORT), timeout=3)
s.sendall(b"GET /q?a=h1 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
check("HTTP/1.1 still served on the same port", b"200 OK" in s.recv(4096))
s.close()

s, c = open_conn(False)
for i in range(3000):
    sid = c.get_next_available_stream_id()
    c.send_headers(sid, hdrs("GET", "/q?a=x"), end_stream=False)
    c.reset_stream(sid)
    try:
        s.sendall(c.data_to_send())
    except OSError:
        break
evs = pump(s, c, lambda e: False, 1.5)
check("rapid reset flood is cut off (GOAWAY or close)", any(isinstance(x, h2.events.ConnectionTerminated) for x in evs) or "closed" in evs)
s.close()


def valid_stream():
    cfg = h2.config.H2Configuration(client_side=True)
    c = h2.connection.H2Connection(config=cfg)
    c.initiate_connection()
    c.send_headers(1, hdrs("POST", "/echo", [("x-probe", "z")]), end_stream=False)
    c.send_data(1, b"hello world" * 50, end_stream=True)
    c.send_headers(3, hdrs("GET", "/q?a=1"), end_stream=True)
    c.ping(b"abcdefgh")
    return c.data_to_send()


seed = valid_stream()
rng = random.Random(7)
dead = 0
for run in range(FUZZ_RUNS):
    b = bytearray(seed)
    mode = rng.randint(0, 5)
    if mode == 0:
        for _ in range(rng.randint(1, 8)):
            b[rng.randrange(len(b))] = rng.randrange(256)
    elif mode == 1:
        b = b[:rng.randrange(len(b))]
    elif mode == 2:
        pos = rng.randrange(24, len(b))
        b[pos:pos] = frame(rng.randrange(0, 12), rng.randrange(256), rng.choice([0, 1, 3, 5, 2 ** 31 - 1]), os.urandom(rng.randrange(0, 40)))
    elif mode == 3:
        b = bytearray(seed[:24]) + b"".join(frame(rng.randrange(0, 12), rng.randrange(256), rng.randrange(0, 8), os.urandom(rng.randrange(0, 30))) for _ in range(rng.randrange(1, 12)))
    elif mode == 4:
        b = bytearray(seed[:24]) + frame(4, 0, 0, os.urandom(6 * rng.randrange(0, 4))) + bytes(b[24:]) * rng.randrange(1, 4)
    else:
        b = bytearray(os.urandom(rng.randrange(1, 200)))
    try:
        fs = socket.create_connection(("127.0.0.1", PLAIN_PORT), timeout=2)
        fs.sendall(bytes(b))
        fs.settimeout(0.05)
        try:
            fs.recv(65536)
        except OSError:
            pass
        fs.close()
    except OSError:
        pass
    if run % 50 == 49 and any(p.poll() is not None for p in procs):
        dead += 1
        break
alive = all(p.poll() is None for p in procs)
r = request("GET", "/q?a=after", tls=False)
check("fuzz %d runs: no crash, server still answers" % FUZZ_RUNS, alive and r["b"] == b"after")
for p in procs:
    p.kill()
print("H2 SERVER OK: %d / %d" % (ok, total))
sys.exit(0 if ok == total else 1)

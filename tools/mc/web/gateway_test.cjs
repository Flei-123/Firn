// SPDX-License-Identifier: MPL-2.0
// tools/mc/web/gateway_test.cjs -- tests ws_gateway.cjs against a fake Minecraft status server (handshake + status + ping/pong).
// Prints `RESULT mc-ws-gateway: ok`. NOT tested: a real Minecraft server (see tools/mc/net), TLS, browser WebSocket client.
"use strict";
const net = require("net"), http = require("http"), crypto = require("crypto");
const { createGateway, frame, parser, isPrivate } = require("./ws_gateway.cjs");
const fails = [];
const ck = (n, ok) => { console.log("   " + (ok ? "ok   " : "FAIL ") + n); if (!ok) fails.push(n); };
const vi = (n) => { const o = []; do { let b = n & 127; n >>>= 7; if (n) b |= 128; o.push(b); } while (n); return Buffer.from(o); };
const pkt = (id, body) => { const b = Buffer.concat([vi(id), body]); return Buffer.concat([vi(b.length), b]); };
const str = (s) => { const b = Buffer.from(s); return Buffer.concat([vi(b.length), b]); };
function readVarint(buf, o) { let v = 0, s = 0; for (;;) { if (o >= buf.length) return null; const b = buf[o++]; v |= (b & 127) << s; if (!(b & 128)) return [v, o]; s += 7; } }

// fake MC server: Handshake(0) + Status Request(0) -> Status Response; Ping(1) -> Pong(1) same payload
const STATUS = JSON.stringify({ version: { name: "1.20.1", protocol: 763 }, players: { max: 20, online: 0 }, description: "fake" });
const mc = net.createServer((s) => {
  let buf = Buffer.alloc(0);
  s.on("data", (d) => {
    buf = Buffer.concat([buf, d]);
    for (;;) {
      const l = readVarint(buf, 0); if (!l || buf.length < l[1] + l[0]) return;
      const body = buf.subarray(l[1], l[1] + l[0]); buf = buf.subarray(l[1] + l[0]);
      const id = readVarint(body, 0);
      if (id[0] === 0 && body.length === id[1]) s.write(pkt(0, str(STATUS)));        // status request (empty body)
      else if (id[0] === 1) s.write(pkt(1, body.subarray(id[1])));                    // ping -> pong
    }
  });
});

function wsConnect(port, path) {   // minimal masked-frame client; resolves {status, send(), recv(), close()}
  return new Promise((resolve) => {
    const key = crypto.randomBytes(16).toString("base64");
    const req = http.request({ port, host: "127.0.0.1", path, headers: { Connection: "Upgrade", Upgrade: "websocket", "Sec-WebSocket-Key": key, "Sec-WebSocket-Version": "13" } });
    req.on("response", (res) => resolve({ status: res.statusCode }));
    req.on("upgrade", (res, sock, head) => {
      const acc = crypto.createHash("sha1").update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
      let waiter = null; const q = [];
      const push = (m) => { if (waiter) { const w = waiter; waiter = null; w(m); } else q.push(m); };
      // server frames are unmasked: parse by hand
      let buf = head;
      const onData = (d) => {
        buf = Buffer.concat([buf, d]);
        for (;;) {
          if (buf.length < 2) return; let len = buf[1] & 127, off = 2;
          if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; } else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); off = 10; }
          if (buf.length < off + len) return; push({ op: buf[0] & 15, data: buf.subarray(off, off + len) }); buf = buf.subarray(off + len);
        }
      };
      sock.on("data", onData); if (head.length) { const h = head; buf = Buffer.alloc(0); onData(h); }
      const mask = (op, p) => { const m = crypto.randomBytes(4), o = Buffer.from(p); for (let i = 0; i < o.length; i++) o[i] ^= m[i & 3]; const n = o.length; const h = n < 126 ? Buffer.from([0x80 | op, 0x80 | n]) : n < 65536 ? Buffer.from([0x80 | op, 0x80 | 126, n >> 8, n & 255]) : (() => { const b = Buffer.alloc(10); b[0] = 0x80 | op; b[1] = 0x80 | 127; b.writeBigUInt64BE(BigInt(n), 2); return b; })(); return Buffer.concat([h, m, o]); };
      resolve({ status: 101, accept_ok: res.headers["sec-websocket-accept"] === acc, send: (p) => sock.write(mask(2, p)), sendRaw: (op, p) => sock.write(mask(op, p)),
        recv: (ms = 3000) => new Promise((r) => { if (q.length) return r(q.shift()); waiter = r; setTimeout(() => { if (waiter === r) { waiter = null; r(null); } }, ms); }), sock });
    });
    req.end();
  });
}
async function recvBytes(c, n) { let b = Buffer.alloc(0); while (b.length < n) { const m = await c.recv(); if (!m || m.op !== 2) return null; b = Buffer.concat([b, m.data]); } return b; }

(async () => {
  await new Promise((r) => mc.listen(0, "127.0.0.1", r));
  const mport = mc.address().port, target = "127.0.0.1:" + mport;
  const gw = createGateway({ allow: [target, "127.0.0.2:1"], allowPrivate: true });
  const strict = createGateway({ allow: [target] });
  await new Promise((r) => gw.listen(0, "127.0.0.1", r)); await new Promise((r) => strict.listen(0, "127.0.0.1", r));
  const gp = gw.address().port, sp = strict.address().port;

  // 1. handshake + status over the gateway; the request is split across two WS messages (stream semantics)
  const c = await wsConnect(gp, "/ws?target=" + target);
  ck("upgrade 101 + accept key", c.status === 101 && c.accept_ok);
  const hs = pkt(0, Buffer.concat([vi(763), str("127.0.0.1"), Buffer.from([mport >> 8, mport & 255]), vi(1)]));
  c.send(hs.subarray(0, 3)); c.send(Buffer.concat([hs.subarray(3), pkt(0, Buffer.alloc(0))]));
  let acc = Buffer.alloc(0), json = null;
  while (!json) { const m = await c.recv(); if (!m) break; acc = Buffer.concat([acc, m.data]); const l = readVarint(acc, 0); if (l && acc.length >= l[1] + l[0]) { const id = readVarint(acc, l[1]); const sl = readVarint(acc, id[1]); json = JSON.parse(acc.subarray(sl[1], sl[1] + sl[0]).toString()); } }
  ck("status response through gateway (protocol 763)", json && json.version.protocol === 763);
  // 2. ping/pong keeps the payload (8 bytes)
  const payload = crypto.randomBytes(8); c.send(pkt(1, payload));
  const pong = await recvBytes(c, 10);
  ck("ping -> pong payload identical", pong && pong.subarray(2).equals(payload));
  // 3. WS ping answered with WS pong
  c.sendRaw(9, Buffer.from("hi")); let m = await c.recv(); ck("ws ping -> ws pong", m && m.op === 10 && m.data.toString() === "hi");
  // 4. big payload through (65 KiB, > 16-bit frame length)
  c.send(Buffer.alloc(70000, 7)); await new Promise((r) => setTimeout(r, 300)); ck("70 KB binary message accepted", gw.stats.bytes_up > 70000);
  // 5. close frame closes the TCP side
  c.sendRaw(8, Buffer.from([3, 232])); m = await c.recv(); ck("close frame answered", m && m.op === 8);
  // 6. refusals
  ck("target not on allow-list -> 403", (await wsConnect(gp, "/ws?target=127.0.0.1:9")).status === 403);
  ck("bad target -> 400", (await wsConnect(gp, "/ws?target=;rm")).status === 400);
  ck("loopback refused without allowPrivate -> 403", (await wsConnect(sp, "/ws?target=" + target)).status === 403);
  ck("wrong path -> 400", (await wsConnect(gp, "/x?target=" + target)).status === 400);
  ck("unreachable allowed target -> 502/504", [502, 504].includes((await wsConnect(gp, "/ws?target=127.0.0.2:1")).status));
  ck("private ranges classified", isPrivate("10.1.2.3") && isPrivate("192.168.0.1") && isPrivate("172.16.0.1") && isPrivate("127.0.0.1") && isPrivate("169.254.1.1") && isPrivate("::1") && isPrivate("fd00::1") && !isPrivate("8.8.8.8") && !isPrivate("172.32.0.1"));
  ck("stats: bytes counted", gw.stats.accepted >= 1 && gw.stats.bytes_up > 70000 && gw.stats.bytes_down > 0);
  gw.close(); strict.close(); mc.close();
  console.log("RESULT mc-ws-gateway: " + (fails.length ? "FAIL " + fails.join("; ") : "ok"));
  process.exit(fails.length ? 1 : 0);
})();
setTimeout(() => { console.log("RESULT mc-ws-gateway: FAIL timeout"); process.exit(1); }, 30000).unref();

// SPDX-License-Identifier: MPL-2.0
// tools/mc/web/ws_gateway.cjs -- r106/web-gateway prototype (docs/mc/web-gateway.md): WebSocket (binary) <-> TCP byte pipe.
//   node tools/mc/web/ws_gateway.cjs --port 8081 --allow mc.example.org:25565[,host:port...] [--allow-private]
//   browser: new WebSocket("ws://gw:8081/ws?target=mc.example.org:25565"); every binary message = a chunk of the MC byte stream.
// Own RFC 6455 framing (no dependencies). PROTOTYPE: no TLS (put a reverse proxy in front), no signed token yet (open item), no rate limit per IP.
// Safety: target must be on the allow-list; private/loopback/link-local targets are refused unless --allow-private (tests only).
"use strict";
const http = require("http"), net = require("net"), dns = require("dns"), crypto = require("crypto");
const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_MSG = 1 << 20;        // largest accepted WS message (MC packets are <= 2 MiB compressed; chunks come in pieces)
const IDLE_MS = 60000;          // no traffic in either direction
const CONNECT_MS = 8000;        // TCP connect timeout

function isPrivate(ip) {
  if (net.isIPv4(ip)) {
    const [a, b] = ip.split(".").map(Number);
    return a === 10 || a === 127 || a === 0 || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
  }
  const s = ip.toLowerCase();
  return s === "::1" || s === "::" || s.startsWith("fc") || s.startsWith("fd") || s.startsWith("fe80") || (s.startsWith("::ffff:") && isPrivate(s.slice(7)));
}

function frame(op, payload) {
  const n = payload.length;
  const h = n < 126 ? Buffer.from([0x80 | op, n]) : n < 65536 ? Buffer.from([0x80 | op, 126, n >> 8, n & 255]) : (() => { const b = Buffer.alloc(10); b[0] = 0x80 | op; b[1] = 127; b.writeBigUInt64BE(BigInt(n), 2); return b; })();
  return Buffer.concat([h, payload]);
}

// Parser for client frames (must be masked). onMsg(opcode, Buffer); returns false on protocol error.
function parser(onMsg, onErr) {
  let buf = Buffer.alloc(0), frag = null, fragOp = 0;
  return (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      if (buf.length < 2) return;
      const fin = (buf[0] & 0x80) !== 0, op = buf[0] & 15, masked = (buf[1] & 0x80) !== 0;
      let len = buf[1] & 127, off = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; }
      else if (len === 127) { if (buf.length < 10) return; const l = buf.readBigUInt64BE(2); if (l > BigInt(MAX_MSG)) return onErr(1009); len = Number(l); off = 10; }
      if (!masked) return onErr(1002);
      if (len > MAX_MSG) return onErr(1009);
      if (buf.length < off + 4 + len) return;
      const mask = buf.subarray(off, off + 4), p = Buffer.from(buf.subarray(off + 4, off + 4 + len));
      for (let i = 0; i < len; i++) p[i] ^= mask[i & 3];
      buf = buf.subarray(off + 4 + len);
      if (op >= 8) { onMsg(op, p); continue; }
      if (op === 0) { if (!frag) return onErr(1002); frag.push(p); if (frag.reduce((s, x) => s + x.length, 0) > MAX_MSG) return onErr(1009); if (fin) { const m = Buffer.concat(frag); frag = null; onMsg(fragOp, m); } continue; }
      if (frag) return onErr(1002);
      if (fin) onMsg(op, p); else { frag = [p]; fragOp = op; }
    }
  };
}

function createGateway(opts) {
  const allow = new Set((opts.allow || []).map((s) => s.toLowerCase()));
  const stats = { accepted: 0, refused: 0, bytes_up: 0, bytes_down: 0 };
  const srv = http.createServer((req, res) => { res.writeHead(426, { "content-type": "text/plain" }); res.end("websocket only\n"); });
  srv.on("upgrade", (req, sock) => {
    const refuse = (code, why) => { stats.refused++; sock.end(`HTTP/1.1 ${code} ${why}\r\nconnection: close\r\ncontent-length: 0\r\n\r\n`); };
    let u;
    try { u = new URL(req.url, "http://x"); } catch (e) { return refuse(400, "Bad Request"); }
    const key = req.headers["sec-websocket-key"];
    if (u.pathname !== "/ws" || !key || String(req.headers.upgrade).toLowerCase() !== "websocket") return refuse(400, "Bad Request");
    const target = (u.searchParams.get("target") || "").toLowerCase();
    const m = /^([a-z0-9.\-]+):(\d{1,5})$/.exec(target);
    if (!m || +m[2] < 1 || +m[2] > 65535) return refuse(400, "Bad Target");
    if (!allow.has(target)) return refuse(403, "Target Not Allowed");
    dns.lookup(m[1], (err, ip) => {
      if (err) return refuse(502, "Bad Gateway");
      if (!opts.allowPrivate && isPrivate(ip)) return refuse(403, "Private Target");
      const tcp = net.connect({ host: ip, port: +m[2] });
      let done = false, open = false;
      const kill = () => { if (done) return; done = true; tcp.destroy(); sock.destroy(); };
      const ct = setTimeout(() => { if (!open) { kill(); refuse(504, "Gateway Timeout"); } }, CONNECT_MS);
      tcp.once("error", () => { clearTimeout(ct); if (!open) { done = true; refuse(502, "Bad Gateway"); } else kill(); });
      tcp.once("connect", () => {
        clearTimeout(ct); open = true; stats.accepted++;
        const acc = crypto.createHash("sha1").update(key + GUID).digest("base64");
        sock.write(`HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: ${acc}\r\n\r\n`);
        let idle = setTimeout(kill, IDLE_MS); const touch = () => { clearTimeout(idle); idle = setTimeout(kill, IDLE_MS); };
        const close = (code) => { const b = Buffer.alloc(2); b.writeUInt16BE(code); sock.end(frame(8, b)); tcp.destroy(); done = true; clearTimeout(idle); };
        tcp.on("data", (d) => { touch(); stats.bytes_down += d.length; if (!sock.write(frame(2, d))) { tcp.pause(); sock.once("drain", () => tcp.resume()); } });
        tcp.on("end", () => close(1000)); tcp.on("close", () => { clearTimeout(idle); sock.destroy(); });
        sock.on("data", parser((op, p) => {
          touch();
          if (op === 2) { stats.bytes_up += p.length; if (!tcp.write(p)) { sock.pause(); tcp.once("drain", () => sock.resume()); } }
          else if (op === 9) sock.write(frame(10, p));
          else if (op === 8) close(1000);
          else if (op === 1) close(1003);
        }, (code) => close(code)));
        sock.on("close", () => { clearTimeout(idle); tcp.destroy(); }); sock.on("error", kill);
      });
    });
  });
  srv.stats = stats;
  return srv;
}

module.exports = { createGateway, frame, parser, isPrivate };
if (require.main === module) {
  const a = process.argv.slice(2), get = (k, d) => { const i = a.indexOf(k); return i >= 0 ? a[i + 1] : d; };
  const gw = createGateway({ allow: (get("--allow", "") || "").split(",").filter(Boolean), allowPrivate: a.includes("--allow-private") });
  gw.listen(+get("--port", 8081), get("--host", "127.0.0.1"), () => console.log("ws_gateway listening on " + gw.address().port));
}

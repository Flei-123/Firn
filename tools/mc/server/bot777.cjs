const net = require('net')
const zlib = require('zlib')
const fs = require('fs')

const [host, portArg, name, scriptPath, outPath, langPath] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const PROTOCOL = 777
const ID = {
  cfgKeepAliveIn: 4, cfgKeepAliveOut: 4, cfgFinishIn: 3, cfgFinishOut: 3, cfgKnownIn: 15, cfgKnownOut: 7, cfgDisconnect: 2,
  keepAliveIn: 45, keepAliveOut: 28, positionIn: 73, teleportConfirm: 0, commands: 16, systemChat: 124, suggestions: 15,
  blockEntity: 6, chunk: 46, tickEnd: 13, chatCommand: 7, chatCommandSigned: 8, suggestOut: 15, disconnect: 0x1d,
}
const lang = langPath && fs.existsSync(langPath) ? JSON.parse(fs.readFileSync(langPath, 'utf8')) : {}

function varint(n) {
  const b = []
  let v = n >>> 0
  do {
    let x = v & 127
    v >>>= 7
    if (v) x |= 128
    b.push(x)
  } while (v)
  return Buffer.from(b)
}

function readVarint(buf, at) {
  let r = 0
  let shift = 0
  for (;;) {
    if (at >= buf.length) return null
    const b = buf[at++]
    r |= (b & 127) << shift
    if (!(b & 128)) return [r >>> 0, at]
    shift += 7
  }
}

function str(s) {
  const b = Buffer.from(s, 'utf8')
  return Buffer.concat([varint(b.length), b])
}

function i64(n) {
  const b = Buffer.alloc(8)
  b.writeBigInt64BE(BigInt(n))
  return b
}

class Cur {
  constructor(buf, at = 0) { this.buf = buf; this.at = at }
  u8() { return this.buf[this.at++] }
  i16() { const v = this.buf.readInt16BE(this.at); this.at += 2; return v }
  i32() { const v = this.buf.readInt32BE(this.at); this.at += 4; return v }
  i64() { const v = this.buf.readBigInt64BE(this.at); this.at += 8; return v }
  f32() { const v = this.buf.readFloatBE(this.at); this.at += 4; return v }
  f64() { const v = this.buf.readDoubleBE(this.at); this.at += 8; return v }
  varint() { const r = readVarint(this.buf, this.at); this.at = r[1]; return r[0] }
  string() { const n = this.varint(); const s = this.buf.toString('utf8', this.at, this.at + n); this.at += n; return s }
  nstring() { const n = this.i16() & 0xffff; const s = this.buf.toString('utf8', this.at, this.at + n); this.at += n; return s }
}

function readPayload(c, tag) {
  switch (tag) {
    case 1: return c.u8()
    case 2: return c.i16()
    case 3: return c.i32()
    case 4: return c.i64()
    case 5: return c.f32()
    case 6: return c.f64()
    case 7: { const n = c.i32(); c.at += n; return '[bytes]' }
    case 8: return c.nstring()
    case 9: {
      const t = c.u8()
      const n = c.i32()
      const a = []
      for (let i = 0; i < n; i++) a.push(readPayload(c, t))
      return a
    }
    case 10: {
      const o = {}
      for (;;) {
        const t = c.u8()
        if (t === 0) break
        const k = c.nstring()
        o[k] = readPayload(c, t)
      }
      return o
    }
    case 11: { const n = c.i32(); c.at += 4 * n; return '[ints]' }
    case 12: { const n = c.i32(); c.at += 8 * n; return '[longs]' }
  }
  throw new Error('bad nbt tag ' + tag)
}

function readAnonNbt(c) {
  const t = c.u8()
  if (t === 0) return null
  return readPayload(c, t)
}

function render(v) {
  if (v === null || v === undefined) return ''
  if (typeof v === 'string' || typeof v === 'number' || typeof v === 'bigint') return String(v)
  if (Array.isArray(v)) return v.map(render).join('')
  let s = ''
  if (v.text !== undefined) s += String(v.text)
  if (v.translate !== undefined) {
    const args = (v.with || []).map(render)
    const tpl = lang[v.translate]
    if (tpl) {
      let i = 0
      s += tpl.replace(/%(\d+\$)?s/g, (m, n) => (n ? args[parseInt(n, 10) - 1] : args[i++]) ?? '')
    } else {
      s += v.translate + (args.length ? '[' + args.join('|') + ']' : '')
    }
  }
  if (v.extra) s += render(v.extra)
  return s
}

function offlineUuid(n) {
  if (process.env.BOT_UUID) return Buffer.from(process.env.BOT_UUID.replace(/-/g, ''), 'hex')
  const h = require('crypto').createHash('md5').update('OfflinePlayer:' + n).digest()
  h[6] = (h[6] & 0x0f) | 0x30
  h[8] = (h[8] & 0x3f) | 0x80
  return h
}

let threshold = -1
const sock = net.connect(port, host)
let acc = Buffer.alloc(0)
let state = 'login'
const result = { declare: null, cmds: [], tabs: [], blockEntities: [], chunkBlockEntities: [] }
const lines = fs.readFileSync(scriptPath, 'utf8').split('\n').filter((l) => l.length > 0)
let current = null
let joinedAt = 0
let done = false
let tickTimer = null

function send(id, body) {
  const payload = Buffer.concat([varint(id), body])
  let frame
  if (threshold < 0) frame = Buffer.concat([varint(payload.length), payload])
  else if (payload.length < threshold) frame = Buffer.concat([varint(payload.length + 1), Buffer.from([0]), payload])
  else {
    const z = zlib.deflateSync(payload)
    const head = varint(payload.length)
    frame = Buffer.concat([varint(head.length + z.length), head, z])
  }
  sock.write(frame)
}

function finish() {
  if (done) return
  done = true
  if (tickTimer) clearInterval(tickTimer)
  fs.writeFileSync(outPath, JSON.stringify(result))
  sock.destroy()
  process.exit(0)
}

function sendCommand(text) {
  const signed = /^(say|msg|tell|w|me|teammsg|tm)\b/.test(text)
  if (!signed) return send(ID.chatCommand, str(text))
  send(ID.chatCommandSigned, Buffer.concat([str(text), i64(Date.now()), i64(0), varint(0), varint(0), Buffer.alloc(3), Buffer.from([0])]))
}

function nextLine(i) {
  if (i >= lines.length) return setTimeout(finish, 600)
  const line = lines[i]
  if (line.startsWith('?')) {
    const text = line.slice(1)
    current = { tab: text, txn: i + 1 }
    send(ID.suggestOut, Buffer.concat([varint(i + 1), str(text)]))
  } else {
    current = { line, out: [] }
    result.cmds.push(current)
    sendCommand(line)
  }
  setTimeout(() => nextLine(i + 1), 450)
}

function onPlay(id, body) {
  if (id === ID.keepAliveIn) send(ID.keepAliveOut, body.subarray(0, 8))
  else if (id === ID.positionIn) {
    if (process.env.BOT_DEBUG) console.error('pos', body.toString('hex'))
    const r = readVarint(body, 0)
    const f = Buffer.alloc(32)
    body.copy(f, 0, r[1], r[1] + 24)
    body.copy(f, 24, r[1] + 48, r[1] + 56)
    send(ID.teleportConfirm, Buffer.concat([varint(r[0]), f]))
    if (!joinedAt) {
      joinedAt = Date.now()
      tickTimer = setInterval(() => send(ID.tickEnd, Buffer.alloc(0)), 50)
      setTimeout(() => nextLine(0), 2500)
    }
  } else if (id === ID.commands) result.declare = body.toString('hex')
  else if (id === ID.systemChat) {
    try {
      const c = new Cur(body)
      const raw = readAnonNbt(c)
      if (process.env.BOT_RAW) console.error(JSON.stringify(raw, (k, v) => typeof v === 'bigint' ? Number(v) : v))
      const msg = render(raw)
      if (current && current.out) current.out.push(msg)
    } catch (e) {
      if (current && current.out) current.out.push('<undecodable ' + e.message + '>')
    }
  } else if (id === ID.suggestions) {
    const c = new Cur(body)
    const txn = c.varint()
    const start = c.varint()
    const length = c.varint()
    const n = c.varint()
    const matches = []
    for (let i = 0; i < n; i++) {
      matches.push(c.string())
      if (c.u8()) readAnonNbt(c)
    }
    result.tabs.push({ txn, start, length, matches })
  } else if (id === ID.blockEntity) {
    const c = new Cur(body)
    const v = c.i64()
    const type = c.varint()
    const nbt = body.subarray(c.at).toString('hex')
    result.blockEntities.push({ pos: v.toString(), type, nbt })
  } else if (id === ID.disconnect) {
    result.disconnect = body.toString('hex')
  }
}

function onPacket(id, body) {
  if (state === 'login') {
    if (id === 3) threshold = readVarint(body, 0)[0]
    else if (id === 2) { send(3, Buffer.alloc(0)); state = 'config' }
    else if (id === 0) { console.error('login disconnect', body.toString('utf8')); process.exit(2) }
    return
  }
  if (state === 'config') {
    if (id === ID.cfgKnownIn) send(ID.cfgKnownOut, varint(0))
    else if (id === ID.cfgKeepAliveIn) send(ID.cfgKeepAliveOut, body)
    else if (id === ID.cfgFinishIn) { send(ID.cfgFinishOut, Buffer.alloc(0)); state = 'play' }
    else if (id === ID.cfgDisconnect) { console.error('config disconnect'); process.exit(2) }
    return
  }
  onPlay(id, body)
}

sock.on('connect', () => {
  send(0, Buffer.concat([varint(PROTOCOL), str(host), Buffer.from([port >> 8, port & 255]), varint(2)]))
  send(0, Buffer.concat([str(name), offlineUuid(name)]))
})

sock.on('data', (d) => {
  acc = Buffer.concat([acc, d])
  for (;;) {
    const l = readVarint(acc, 0)
    if (!l || acc.length < l[1] + l[0]) break
    let payload = acc.subarray(l[1], l[1] + l[0])
    acc = acc.subarray(l[1] + l[0])
    if (threshold >= 0) {
      const u = readVarint(payload, 0)
      payload = u[0] === 0 ? payload.subarray(u[1]) : zlib.inflateSync(payload.subarray(u[1]))
    }
    const idr = readVarint(payload, 0)
    try { onPacket(idr[0], payload.subarray(idr[1])) } catch (e) { console.error('packet error', idr[0], e.message) }
  }
})
sock.on('error', (e) => { console.error('socket error', e.message); finish() })
sock.on('close', finish)
setTimeout(finish, 120000)

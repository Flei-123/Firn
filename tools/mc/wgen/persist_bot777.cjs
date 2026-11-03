'use strict'
const net = require('net')
const zlib = require('zlib')
const fs = require('fs')

const [host, portArg, mode, stateFile] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const PROTOCOL = 777
const SECTIONS = 24
const MIN_Y = -64
const GLOBAL_BITS = 16
const BIOME_BITS = 7
const STONE = 1
const C2S = { teleportConfirm: 0, tickEnd: 13, keepAlive: 28, positionLook: 31, blockDig: 41, heldItem: 54, creativeSlot: 57, blockPlace: 66 }
const S2C = { blockChange: 8, windowItems: 18, setSlot: 20, keepAlive: 45, mapChunk: 46, login: 50, position: 73, updateTime: 115, disconnect: 0x1d }

const out = { mode, pos: null, time: null, gameMode: null, window: {}, changes: [], errors: [] }
const chunks = {}
const overlay = {}
let seq = 0
let done = false
let threshold = -1
let state = 'login'
let acc = Buffer.alloc(0)
let joined = false
let tickTimer = null

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

function str(s) {
  const b = Buffer.from(s, 'utf8')
  return Buffer.concat([varint(b.length), b])
}

function f64(n) { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b }
function f32(n) { const b = Buffer.alloc(4); b.writeFloatBE(n); return b }
function i16(n) { const b = Buffer.alloc(2); b.writeInt16BE(n); return b }

class Cur {
  constructor(buf, at = 0) { this.buf = buf; this.at = at }
  u8() { return this.buf[this.at++] }
  i16() { const v = this.buf.readInt16BE(this.at); this.at += 2; return v }
  i32() { const v = this.buf.readInt32BE(this.at); this.at += 4; return v }
  i64() { const v = this.buf.readBigInt64BE(this.at); this.at += 8; return v }
  f64() { const v = this.buf.readDoubleBE(this.at); this.at += 8; return v }
  varint() {
    let r = 0, s = 0, b
    do { b = this.buf[this.at++]; r |= (b & 127) << s; s += 7 } while (b & 128)
    return r >>> 0
  }
  varlong() {
    let r = 0n, s = 0n, b
    do { b = this.buf[this.at++]; r |= BigInt(b & 127) << s; s += 7n } while (b & 128)
    return r
  }
  string() { const n = this.varint(); const s = this.buf.toString('utf8', this.at, this.at + n); this.at += n; return s }
}

function packPos(x, y, z) {
  return Buffer.from(((BigInt(x) & 0x3FFFFFFn) << 38n | (BigInt(z) & 0x3FFFFFFn) << 12n | (BigInt(y) & 0xFFFn)).toString(16).padStart(16, '0'), 'hex')
}

function unpackPos(v) {
  const sx = Number(BigInt.asIntN(26, v >> 38n))
  const sz = Number(BigInt.asIntN(26, v >> 12n))
  const sy = Number(BigInt.asIntN(12, v))
  return { x: sx, y: sy, z: sz }
}

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

function container(c, cells, indirectMax, directBits) {
  const bits = c.u8()
  if (bits === 0) return { single: c.varint() }
  let pal = null
  if (bits <= indirectMax) {
    const n = c.varint()
    pal = []
    for (let i = 0; i < n; i++) pal.push(c.varint())
  }
  const width = pal ? bits : directBits
  const per = Math.floor(64 / width)
  const count = Math.ceil(cells / per)
  const longs = []
  for (let i = 0; i < count; i++) longs.push(c.i64())
  return { bits: width, pal, longs, per }
}

function entry(ct, idx) {
  if (ct.single !== undefined) return ct.single
  const l = BigInt.asUintN(64, ct.longs[Math.floor(idx / ct.per)])
  const v = Number((l >> BigInt((idx % ct.per) * ct.bits)) & ((1n << BigInt(ct.bits)) - 1n))
  return ct.pal ? ct.pal[v] : v
}

function indexChunk(body) {
  const c = new Cur(body)
  const x = c.i32()
  const z = c.i32()
  const nh = c.varint()
  for (let i = 0; i < nh; i++) {
    c.varint()
    const n = c.varint()
    c.at += n * 8
  }
  const size = c.varint()
  const end = c.at + size
  const secs = []
  for (let s = 0; s < SECTIONS && c.at < end; s++) {
    c.i16()
    c.i16()
    secs.push(container(c, 4096, 8, GLOBAL_BITS))
    container(c, 64, 3, BIOME_BITS)
  }
  if (c.at !== end) out.errors.push(`chunk ${x},${z} parse ended at ${c.at - (end - size)} of ${size}`)
  chunks[`${x},${z}`] = secs
}

function blockState(x, y, z) {
  const o = overlay[`${x},${y},${z}`]
  if (o !== undefined) return o
  const secs = chunks[`${x >> 4},${z >> 4}`]
  if (!secs) return -1
  const sec = secs[(y - MIN_Y) >> 4]
  if (!sec) return -1
  return entry(sec, (((y - MIN_Y) & 15) * 16 + (z & 15)) * 16 + (x & 15))
}

function readSlot(c) {
  const count = c.varint()
  if (count === 0) return null
  const id = c.varint()
  const added = c.varint()
  const removed = c.varint()
  if (added !== 0 || removed !== 0) throw new Error('slot with components')
  return { id, count }
}

function onLogin(body) {
  const c = new Cur(body)
  c.i32()
  c.u8()
  const nw = c.varint()
  for (let i = 0; i < nw; i++) c.string()
  c.varint()
  c.varint()
  c.varint()
  c.u8()
  c.u8()
  c.u8()
  c.varint()
  c.string()
  c.i64()
  out.gameMode = c.u8()
}

function onPosition(body) {
  const c = new Cur(body)
  const id = c.varint()
  const x = c.f64(), y = c.f64(), z = c.f64()
  c.at += 24
  const yaw = body.readFloatBE(c.at), pitch = body.readFloatBE(c.at + 4)
  send(C2S.teleportConfirm, Buffer.concat([varint(id), f64(x), f64(y), f64(z), f32(yaw), f32(pitch)]))
  send(C2S.positionLook, Buffer.concat([f64(x), f64(y), f64(z), f32(yaw), f32(pitch), Buffer.from([1])]))
  if (!out.pos) out.pos = { x, y, z }
  if (!joined) {
    joined = true
    tickTimer = setInterval(() => send(C2S.tickEnd, Buffer.alloc(0)), 50)
    setTimeout(mode === 'edit' ? edit : check, 4000)
  }
}

function onPlay(id, body) {
  if (id === S2C.keepAlive) send(C2S.keepAlive, body.subarray(0, 8))
  else if (id === S2C.login) onLogin(body)
  else if (id === S2C.position) onPosition(body)
  else if (id === S2C.mapChunk) indexChunk(body)
  else if (id === S2C.updateTime) {
    const c = new Cur(body)
    c.i64()
    const n = c.varint()
    if (n > 0) { c.varint(); out.time = Number(c.varlong()) }
  } else if (id === S2C.blockChange) {
    const c = new Cur(body)
    const p = unpackPos(BigInt.asUintN(64, c.i64()))
    const state = c.varint()
    out.changes.push({ ...p, state })
    overlay[`${p.x},${p.y},${p.z}`] = state
  } else if (id === S2C.setSlot) {
    const c = new Cur(body)
    const win = c.varint()
    c.varint()
    const slot = c.i16()
    const it = readSlot(c)
    if (win === 0 && slot >= 36 && slot <= 44 && it) out.window[slot] = it.id
  } else if (id === S2C.windowItems) {
    const c = new Cur(body)
    const win = c.varint()
    c.varint()
    const n = c.varint()
    for (let i = 0; i < n; i++) {
      const it = readSlot(c)
      if (win === 0 && i >= 36 && i <= 44 && it) out.window[i] = it.id
    }
  } else if (id === S2C.disconnect) out.errors.push('disconnect ' + body.toString('hex'))
}

function onPacket(id, body) {
  if (state === 'login') {
    if (id === 3) threshold = new Cur(body).varint()
    else if (id === 2) { send(3, Buffer.alloc(0)); state = 'config' }
    else if (id === 0) { out.errors.push('login disconnect ' + body.toString('utf8')); finish() }
    return
  }
  if (state === 'config') {
    if (id === 15) send(7, varint(0))
    else if (id === 4) send(4, body)
    else if (id === 3) { send(3, Buffer.alloc(0)); state = 'play' }
    else if (id === 2) { out.errors.push('config disconnect'); finish() }
    return
  }
  onPlay(id, body)
}

function dig(x, y, z) {
  send(C2S.blockDig, Buffer.concat([varint(0), packPos(x, y, z), Buffer.from([1]), varint(++seq)]))
}

function place(x, y, z, face) {
  send(C2S.blockPlace, Buffer.concat([varint(0), packPos(x, y, z), varint(face), f32(0.5), f32(0.5), f32(0.5), Buffer.from([0, 0]), varint(++seq)]))
}

function edit() {
  const px = Math.floor(out.pos.x), py = Math.floor(out.pos.y), pz = Math.floor(out.pos.z)
  send(C2S.creativeSlot, Buffer.concat([i16(36), varint(1), varint(STONE), varint(0), varint(0)]))
  const steps = [
    () => { out.before = { dig: blockState(px + 2, py - 1, pz), floor: blockState(px + 3, py - 1, pz) } },
    () => dig(px + 2, py - 1, pz),
    () => dig(px + 2, py - 2, pz),
    () => send(C2S.heldItem, i16(0)),
    () => place(px + 3, py - 1, pz, 1),
    () => send(C2S.positionLook, Buffer.concat([f64(out.pos.x - 4.25), f64(out.pos.y), f64(out.pos.z + 1.5), f32(33), f32(5), Buffer.from([1])])),
  ]
  steps.forEach((f, i) => setTimeout(f, 400 * (i + 1)))
  setTimeout(() => {
    out.expect = [
      { x: px + 2, y: py - 1, z: pz, block: 0 },
      { x: px + 2, y: py - 2, z: pz, block: 0 },
      { x: px + 3, y: py, z: pz, block: STONE },
    ]
    out.final_pos = { x: out.pos.x - 4.25, y: out.pos.y, z: out.pos.z + 1.5 }
    out.stone = STONE
    out.after = out.expect.map((e) => blockState(e.x, e.y, e.z))
    finish()
  }, 400 * (steps.length + 1) + 1500)
}

function check() {
  const st = JSON.parse(fs.readFileSync(stateFile, 'utf8'))
  out.state = st
  out.after = st.expect.map((e) => blockState(e.x, e.y, e.z))
  const px = Math.floor(st.pos.x), py = Math.floor(st.pos.y), pz = Math.floor(st.pos.z)
  send(C2S.heldItem, i16(0))
  setTimeout(() => place(px + 2, py - 2, pz, 1), 300)
  setTimeout(() => {
    out.placed = blockState(px + 2, py - 1, pz)
    finish()
  }, 1500)
}

function chunkRow(y) {
  const parts = []
  const base = out.state ? out.state.pos : out.pos
  const mx = Math.floor(base.x / 16), mz = Math.floor(base.z / 16)
  for (let dz = -1; dz <= 1; dz++) for (let dx = -1; dx <= 1; dx++) {
    const cx = mx + dx, cz = mz + dz
    if (!chunks[`${cx},${cz}`]) { parts.push('none'); continue }
    for (let z = 0; z < 16; z++) for (let x = 0; x < 16; x++) parts.push(blockState(cx * 16 + x, y, cz * 16 + z))
  }
  return require('crypto').createHash('md5').update(parts.join(',')).digest('hex') + ':' + parts.filter((p) => p === 'none').length
}

function finish() {
  if (done) return
  done = true
  out.row = chunkRow(20)
  if (tickTimer) clearInterval(tickTimer)
  if (mode === 'edit') fs.writeFileSync(stateFile, JSON.stringify(out))
  console.log(JSON.stringify(out))
  try { sock.destroy() } catch (e) {}
  setTimeout(() => process.exit(0), 200)
}

const sock = net.connect(port, host)
sock.on('connect', () => {
  const h = Buffer.concat([varint(PROTOCOL), str(host), Buffer.from([port >> 8, port & 255]), varint(2)])
  send(0, h)
  const md5 = require('crypto').createHash('md5').update('OfflinePlayer:PersistBot').digest()
  md5[6] = (md5[6] & 0x0f) | 0x30
  md5[8] = (md5[8] & 0x3f) | 0x80
  send(0, Buffer.concat([str('PersistBot'), md5]))
})
sock.on('data', (d) => {
  acc = Buffer.concat([acc, d])
  for (;;) {
    let len = 0, shift = 0, p = 0, b
    do {
      if (p >= acc.length) return
      b = acc[p++]
      len |= (b & 127) << shift
      shift += 7
    } while (b & 128)
    if (acc.length < p + len) return
    let payload = acc.subarray(p, p + len)
    acc = acc.subarray(p + len)
    if (threshold >= 0) {
      const c = new Cur(payload)
      const u = c.varint()
      payload = u === 0 ? payload.subarray(c.at) : zlib.inflateSync(payload.subarray(c.at))
    }
    const c = new Cur(payload)
    const id = c.varint()
    try { onPacket(id, payload.subarray(c.at)) } catch (e) { out.errors.push(`packet ${id}: ${e.message}`) }
  }
})
sock.on('error', (e) => { out.errors.push('socket ' + e.message); finish() })
sock.on('close', finish)
setTimeout(finish, 40000)

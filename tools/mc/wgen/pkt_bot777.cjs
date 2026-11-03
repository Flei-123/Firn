'use strict'
const net = require('net')
const zlib = require('zlib')

const [host, portArg] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const PROTOCOL = 777
const C2S = { teleportConfirm: 0, chatCommand: 7, tickEnd: 13, keepAlive: 28, positionLook: 31, heldItem: 54, creativeSlot: 57, blockPlace: 66 }
const OBSIDIAN = 394
const FLINT = 1005
const S2C = { mapChunk: 46, login: 50, position: 73, respawn: 84, keepAlive: 45, disconnect: 0x1d }

const PROTO = JSON.parse(require('fs').readFileSync(require('path').join(__dirname, '../../mcgen/data_extra/pc/26.3/protocol.json'), 'utf8'))
const NAMES = {}
for (const [k, v] of Object.entries(PROTO.play.toClient.types.packet[1][0].type[1].mappings)) NAMES[parseInt(k, 16)] = v.replace(/^packet_/, '')
const out = { marks: [], errors: [] }
let mark = { cmd: 'join', packets: [] }
out.marks.push(mark)
const SKIP = new Set(['keep_alive', 'chunk_biomes'])
const DIM_NAMES = ['minecraft:overworld', 'minecraft:the_nether', 'minecraft:the_end']
let current = 'login'
let done = false
let started = false
let threshold = -1
let state = 'login'
let acc = Buffer.alloc(0)
let tickTimer = null
let pos = { x: 0, y: 0, z: 0 }
let seq = 0
let lastSeed = ''

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
  current = c.string()
  lastSeed = String(c.i64())
}

function onRespawn(body) {
  const c = new Cur(body)
  c.varint()
  current = c.string()
  lastSeed = String(c.i64())
}

function onPosition(body) {
  const c = new Cur(body)
  const id = c.varint()
  const x = c.f64(), y = c.f64(), z = c.f64()
  c.at += 24
  const yaw = body.readFloatBE(c.at), pitch = body.readFloatBE(c.at + 4)
  pos = { x, y, z }
  send(C2S.teleportConfirm, Buffer.concat([varint(id), f64(x), f64(y), f64(z), f32(yaw), f32(pitch)]))
  send(C2S.positionLook, Buffer.concat([f64(x), f64(y), f64(z), f32(yaw), f32(pitch), Buffer.from([1])]))
  if (!started) {
    started = true
    tickTimer = setInterval(() => send(C2S.tickEnd, Buffer.alloc(0)), 50)
    setTimeout(run, 4000)
  }
}

const cmds = [
  'tp @s 9 52 9',
  'execute in minecraft:the_nether run tp @s 8 70 8',
  'execute in minecraft:overworld run tp @s 0 120 0',
  'execute in minecraft:the_end run tp @s 100 49 0',
  'execute in minecraft:the_nether run tp @s 8 70 8',
]
if (process.env.DIM_CMDS) { cmds.length = 0; cmds.push(...JSON.parse(process.env.DIM_CMDS)) }

function packPos(x, y, z) {
  return Buffer.from(((BigInt(x) & 0x3FFFFFFn) << 38n | (BigInt(z) & 0x3FFFFFFn) << 12n | (BigInt(y) & 0xFFFn)).toString(16).padStart(16, '0'), 'hex')
}
function i16(n) { const b = Buffer.alloc(2); b.writeInt16BE(n); return b }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const place = (x, y, z) => send(C2S.blockPlace, Buffer.concat([varint(0), packPos(x, y - 1, z), varint(1), f32(0.5), f32(1), f32(0.5), Buffer.from([0, 0]), varint(++seq)]))
const give = (slot, id, n) => send(C2S.creativeSlot, Buffer.concat([i16(slot), varint(n), varint(id), varint(0), varint(0)]))
const move = (x, y, z) => send(C2S.positionLook, Buffer.concat([f64(x), f64(y), f64(z), f32(0), f32(0), Buffer.from([1])]))
const ACTIONS = {
  async frame() {
    give(36, OBSIDIAN, 64)
    give(37, FLINT, 1)
    await sleep(300)
    send(C2S.heldItem, i16(0))
    await sleep(300)
    const frame = []
    for (const x of [10, 11]) frame.push([x, 149, 0], [x, 153, 0])
    for (const x of [9, 12]) for (const y of [150, 151, 152]) frame.push([x, y, 0])
    for (const [x, y, z] of frame) { place(x, y, z); await sleep(60) }
    await sleep(500)
  },
  async ignite() {
    send(C2S.heldItem, i16(1))
    await sleep(300)
    place(10, 150, 0)
    await sleep(1000)
  },
  async enter() {
    move(10.5, 150, 0.5)
    await sleep(3500)
  },
  async leave() {
    const there = pos
    move(there.x, there.y, there.z + 3)
    await sleep(1500)
    move(there.x, there.y, there.z)
    await sleep(3500)
  },
}

function run() {
  cmds.forEach((cmd, i) => setTimeout(() => {
    mark = { cmd, packets: [] }
    out.marks.push(mark)
    if (cmd.startsWith('raw:')) { ACTIONS[cmd.slice(4)]().catch((e) => out.errors.push(String(e))); return }
    send(C2S.chatCommand, str(cmd))
  }, i * 7000))
  setTimeout(finish, cmds.length * 7000 + 3000)
}

function describe(id, body) {
  const name = NAMES[id] || ('id' + id)
  if (SKIP.has(name)) return null
  if (id === S2C.position) return 'position'
  if (id === S2C.respawn) return 'respawn ' + body.toString('latin1').match(/minecraft:[a-z_]+/g).slice(0, 2).join(' ')
  if (id === S2C.login) return 'login'
  if (name === 'difficulty') return 'difficulty ' + JSON.stringify({ difficulty: body[0], difficultyLocked: body[1] !== 0 })
  if (name === 'set_slot') { const c = new Cur(body); const w = c.varint(); c.varint(); return 'set_slot ' + JSON.stringify({ windowId: w, slot: c.i16() }) }
  if (name === 'world_event') return 'world_event ' + JSON.stringify({ effectId: body.readInt32BE(0) })
  return name
}

function onPlay(id, body) {
  if (id === S2C.login) onLogin(body)
  else if (id === S2C.respawn) onRespawn(body)
  let d = describe(id, body)
  if (d && (id === S2C.login || id === S2C.respawn)) d += ' ' + JSON.stringify({ hashedSeed: lastSeed })
  if (d) mark.packets.push(d)
  if (id === S2C.keepAlive) send(C2S.keepAlive, body.subarray(0, 8))
  else if (id === S2C.position) onPosition(body)
  else if (id === S2C.disconnect) out.errors.push('disconnect ' + body.toString('hex'))
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

function finish() {
  if (done) return
  done = true
  if (tickTimer) clearInterval(tickTimer)
  console.log(JSON.stringify(out))
  try { sock.destroy() } catch (e) {}
  setTimeout(() => process.exit(0), 200)
}

const sock = net.connect(port, host)
sock.on('connect', () => {
  const h = Buffer.concat([varint(PROTOCOL), str(host), Buffer.from([port >> 8, port & 255]), varint(2)])
  send(0, h)
  const md5 = require('crypto').createHash('md5').update('OfflinePlayer:DimBot').digest()
  md5[6] = (md5[6] & 0x0f) | 0x30
  md5[8] = (md5[8] & 0x3f) | 0x80
  send(0, Buffer.concat([str('DimBot'), md5]))
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
setTimeout(finish, 110000)

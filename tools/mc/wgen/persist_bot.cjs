'use strict'
const path = require('path')
const fs = require('fs')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const mcData = require(path.join(root, 'minecraft-data'))('1.20.1')

const [host, portArg, mode, stateFile] = process.argv.slice(2)
const out = { mode, pos: null, time: null, gameMode: null, window: {}, changes: [], blocks: {}, errors: [] }
const chunks = {}
const overlay = {}
let seq = 0
let done = false

function varint(buf, o) {
  let r = 0, s = 0, b
  do { b = buf[o.p++]; r |= (b & 127) << s; s += 7 } while (b & 128)
  return r
}
function container(buf, o, maxIndirect) {
  const bits = buf[o.p++]
  let pal = null, single = 0
  if (bits === 0) single = varint(buf, o)
  else if (bits <= maxIndirect) { const n = varint(buf, o); pal = []; for (let i = 0; i < n; i++) pal.push(varint(buf, o)) }
  const n = varint(buf, o)
  const longs = []
  for (let i = 0; i < n; i++) { longs.push(buf.readBigUInt64BE(o.p)); o.p += 8 }
  return { bits, pal, single, longs }
}
function entry(c, idx) {
  if (c.bits === 0) return c.single
  const per = Math.floor(64 / c.bits)
  const l = c.longs[Math.floor(idx / per)]
  const v = Number((l >> BigInt((idx % per) * c.bits)) & ((1n << BigInt(c.bits)) - 1n))
  return c.pal ? c.pal[v] : v
}
function indexChunk(data) {
  const buf = data.chunkData
  const o = { p: 0 }
  const secs = []
  for (let sec = 0; sec < 24; sec++) {
    o.p += 2
    secs.push(container(buf, o, 8))
    container(buf, o, 3)
  }
  chunks[`${data.x},${data.z}`] = secs
}
function blockName(x, y, z) {
  const o = overlay[`${x},${y},${z}`]
  if (o) return o
  const secs = chunks[`${x >> 4},${z >> 4}`]
  if (!secs) return 'unloaded'
  const sec = (y + 64) >> 4
  const idx = (((y + 64) & 15) * 16 + (z & 15)) * 16 + (x & 15)
  const b = mcData.blocksByStateId[entry(secs[sec], idx)]
  return b ? 'minecraft:' + b.name : 'unknown'
}

const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'PersistBot', version: '1.20.1', auth: 'offline', hideErrors: true })
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
client.on('login', (p) => { out.gameMode = p.gameMode })
client.on('map_chunk', indexChunk)
client.on('update_time', (p) => { out.time = Number(p.time) })
client.on('block_change', (d) => {
  out.changes.push({ x: d.location.x, y: d.location.y, z: d.location.z, type: d.type })
  const b = mcData.blocksByStateId[d.type]
  overlay[`${d.location.x},${d.location.y},${d.location.z}`] = b ? 'minecraft:' + b.name : 'unknown'
})
client.on('set_slot', (d) => { if (d.windowId === 0 && d.slot >= 36 && d.slot <= 44 && d.item.present) out.window[d.slot] = d.item.itemId })
client.on('window_items', (d) => {
  if (d.windowId === 0) d.items.forEach((it, i) => { if (it.present && i >= 36 && i <= 44) out.window[i] = it.itemId })
})
client.on('position', (d) => {
  if (!out.pos) out.pos = { x: d.x, y: d.y, z: d.z }
  client.write('teleport_confirm', { teleportId: d.teleportId })
  client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
  if (!out.started) { out.started = true; setTimeout(mode === 'edit' ? edit : check, 4000) }
})

function dig(x, y, z) { client.write('block_dig', { status: 0, location: { x, y, z }, face: 1, sequence: ++seq }) }
function place(x, y, z, face) {
  client.write('block_place', { hand: 0, location: { x, y, z }, direction: face, cursorX: 0.5, cursorY: 0.5, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
}

function edit() {
  const px = Math.floor(out.pos.x), py = Math.floor(out.pos.y), pz = Math.floor(out.pos.z)
  const stone = mcData.itemsByName.stone.id
  out.stone = stone
  client.write('set_creative_slot', { slot: 36, item: { present: true, itemId: stone, itemCount: 1, nbtData: undefined } })
  const steps = [
    () => { out.before = { dig: blockName(px + 2, py - 1, pz), floor: blockName(px + 3, py - 1, pz) } },
    () => dig(px + 2, py - 1, pz),
    () => dig(px + 2, py - 2, pz),
    () => client.write('held_item_slot', { slotId: 0 }),
    () => place(px + 3, py - 1, pz, 1),
    () => client.write('position_look', { x: out.pos.x - 4.25, y: out.pos.y, z: out.pos.z + 1.5, yaw: 33, pitch: 5, onGround: true }),
  ]
  steps.forEach((f, i) => setTimeout(f, 400 * (i + 1)))
  setTimeout(() => {
    out.expect = [
      { x: px + 2, y: py - 1, z: pz, block: 'minecraft:air' },
      { x: px + 2, y: py - 2, z: pz, block: 'minecraft:air' },
      { x: px + 3, y: py, z: pz, block: 'minecraft:stone' },
    ]
    out.final_pos = { x: out.pos.x - 4.25, y: out.pos.y, z: out.pos.z + 1.5 }
    out.after = out.expect.map((e) => blockName(e.x, e.y, e.z))
    finish()
  }, 400 * (steps.length + 1) + 1500)
}

function check() {
  const st = JSON.parse(fs.readFileSync(stateFile, 'utf8'))
  out.state = st
  out.after = st.expect.map((e) => blockName(e.x, e.y, e.z))
  const px = Math.floor(st.pos.x), py = Math.floor(st.pos.y), pz = Math.floor(st.pos.z)
  client.write('held_item_slot', { slotId: 0 })
  setTimeout(() => place(px + 2, py - 2, pz, 1), 300)
  setTimeout(() => {
    out.placed = blockName(px + 2, py - 1, pz)
    finish()
  }, 1500)
}

function finish() {
  if (done) return
  done = true
  if (mode === 'edit') fs.writeFileSync(stateFile, JSON.stringify(out))
  console.log(JSON.stringify(out))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}
setTimeout(() => finish(), 40000)

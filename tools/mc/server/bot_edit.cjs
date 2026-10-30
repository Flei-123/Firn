'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const data = require(path.join(root, 'minecraft-data'))('1.20.1')

const [host, portArg, mode, nameArg] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const name = nameArg || 'EditBot'
const item = (n) => data.itemsByName[n].id
const res = { pos: null, expect: [], changes: [], acks: [], window: {}, errors: [], kicked: null, mode }
let seq = 0
let done = false

const client = mc.createClient({ host, port, username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
client.on('error', (e) => res.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => { res.kicked = p.reason })
client.on('packet', (d, meta) => {
  if (meta.state !== 'play' || done) return
  if (meta.name === 'position') {
    if (!res.pos) res.pos = { x: d.x, y: d.y, z: d.z }
    client.write('teleport_confirm', { teleportId: d.teleportId })
    client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
    if (!res.started) { res.started = true; if (mode === 'build') setTimeout(build, 2500); else setTimeout(() => finish(), 4000) }
  } else if (meta.name === 'block_change') {
    res.changes.push({ x: d.location.x, y: d.location.y, z: d.location.z, type: d.type })
  } else if (meta.name === 'acknowledge_player_digging') {
    res.acks.push(d.sequenceId)
  } else if (meta.name === 'window_items' && d.windowId === 0) {
    d.items.forEach((it, i) => { if (it.present && i >= 36 && i <= 44) res.window[i] = it.itemId })
  }
})

function dig(x, y, z) {
  client.write('block_dig', { status: 0, location: { x, y, z }, face: 1, sequence: ++seq })
}
function place(x, y, z, face) {
  client.write('block_place', { hand: 0, location: { x, y, z }, direction: face, cursorX: 0.5, cursorY: 0.5, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
}
function held(slot) { client.write('held_item_slot', { slotId: slot }) }
function give(slot, n) { client.write('set_creative_slot', { slot, item: { present: true, itemId: item(n), itemCount: 1, nbtData: undefined } }) }

function build() {
  const px = Math.floor(res.pos.x)
  const py = Math.floor(res.pos.y)
  const pz = Math.floor(res.pos.z)
  give(36, 'stone')
  give(37, 'cobblestone')
  const steps = [
    () => dig(px + 2, py - 1, pz),
    () => dig(px + 2, py - 2, pz),
    () => dig(px + 2, py - 3, pz),
    () => dig(px + 3, py - 1, pz),
    () => dig(px + 3, py - 2, pz),
    () => held(0),
    () => place(px + 3, py - 3, pz, 1),
    () => held(1),
    () => place(px + 3, py - 2, pz, 1),
    () => place(px + 3, py - 1, pz, 1),
    () => place(px + 3, py, pz, 5),
  ]
  res.expect = [
    { x: px + 2, y: py - 1, z: pz, block: 'minecraft:air' },
    { x: px + 2, y: py - 2, z: pz, block: 'minecraft:air' },
    { x: px + 2, y: py - 3, z: pz, block: 'minecraft:air' },
    { x: px + 3, y: py - 2, z: pz, block: 'minecraft:stone' },
    { x: px + 3, y: py - 1, z: pz, block: 'minecraft:cobblestone' },
    { x: px + 3, y: py, z: pz, block: 'minecraft:cobblestone' },
    { x: px + 4, y: py, z: pz, block: 'minecraft:cobblestone' },
  ]
  steps.forEach((f, i) => setTimeout(f, 300 * (i + 1)))
  setTimeout(() => {
    res.final_pos = { x: res.pos.x - 4.25, y: res.pos.y, z: res.pos.z + 1.5 }
    client.write('position_look', { x: res.final_pos.x, y: res.final_pos.y, z: res.final_pos.z, yaw: 33, pitch: 5, onGround: true })
  }, 300 * (steps.length + 1) + 700)
  setTimeout(() => finish(), 300 * (steps.length + 1) + 2000)
}

function finish() {
  if (done) return
  done = true
  console.log(JSON.stringify(res))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}
setTimeout(() => finish(), 20000)

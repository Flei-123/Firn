'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const mcData = require(path.join(root, 'minecraft-data'))('1.20.1')
const [host, portArg] = process.argv.slice(2)
const out = { events: [], portalBlocks: 0, errors: [] }
const portalMin = mcData.blocksByName.nether_portal.minStateId
const obsidian = mcData.itemsByName.obsidian.id
const flint = mcData.itemsByName.flint_and_steel.id
const FX = parseInt(process.env.FRAME_X || '10', 10)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
let current = 'login'
let pos = null
const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'PortalBot', version: '1.20.1', auth: 'offline', hideErrors: true })
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
client.on('login', (p) => { current = p.worldName; out.events.push('login ' + p.worldName) })
client.on('respawn', (p) => { current = p.worldName; out.events.push('respawn ' + p.worldName) })
client.on('block_change', (p) => { (out.changes = out.changes || []).push([p.location.x, p.location.y, p.location.z, p.type]); if (p.type === portalMin || p.type === portalMin + 1) out.portalBlocks++ })
client.on('position', (d) => {
  pos = { x: d.x, y: d.y, z: d.z }
  out.events.push(`position ${current} ${d.x.toFixed(2)} ${d.y.toFixed(2)} ${d.z.toFixed(2)}`)
  client.write('teleport_confirm', { teleportId: d.teleportId })
  client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
  if (!out.started) { out.started = true; run().catch((e) => { out.errors.push(String(e)); finish() }) }
})
const item = (id, n) => ({ present: true, itemId: id, itemCount: n, nbtData: undefined })
const cmd = (c) => client.write('chat_command', { command: c, timestamp: BigInt(Date.now()), salt: 0n, argumentSignatures: [], messageCount: 0, acknowledged: Buffer.alloc(3) })
let seq = 0
const place = (x, y, z) => client.write('block_place', { hand: 0, location: { x, y: y - 1, z }, direction: 1, cursorX: 0.5, cursorY: 1, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
const move = (x, y, z) => client.write('position', { x, y, z, onGround: true })
async function run() {
  await sleep(1500)
  cmd(`tp ${FX + 0.5} 150 3.5`)
  await sleep(1500)
  client.write('set_creative_slot', { slot: 36, item: item(obsidian, 64) })
  client.write('set_creative_slot', { slot: 37, item: item(flint, 1) })
  await sleep(300)
  client.write('held_item_slot', { slotId: 0 })
  await sleep(300)
  const frame = []
  for (const x of [FX, FX + 1]) { frame.push([x, 149, 0], [x, 153, 0]) }
  for (const x of [FX - 1, FX + 2]) for (const y of [150, 151, 152]) frame.push([x, y, 0])
  for (const [x, y, z] of frame) { place(x, y, z); await sleep(60) }
  await sleep(500)
  client.write('held_item_slot', { slotId: 1 })
  await sleep(300)
  client.write('block_place', { hand: 0, location: { x: FX, y: 149, z: 0 }, direction: 1, cursorX: 0.5, cursorY: 1, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
  await sleep(1000)
  out.afterIgnite = out.portalBlocks
  move(FX + 0.5, 150, 0.5)
  await sleep(2500)
  out.nether = pos
  const there = pos
  move(there.x, there.y, there.z + 3)
  await sleep(1500)
  move(there.x, there.y, there.z)
  await sleep(2500)
  out.final = current
  out.back = pos
  finish()
}
function finish() {
  console.log(JSON.stringify(out))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}
setTimeout(finish, 90000)

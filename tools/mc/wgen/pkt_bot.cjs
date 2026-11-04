'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const mcData = require(path.join(root, 'minecraft-data'))('1.20.1')
const [host, portArg] = process.argv.slice(2)
const out = { marks: [], errors: [] }
let mark = { cmd: 'join', packets: [] }
out.marks.push(mark)
const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'DimBot', version: '1.20.1', auth: 'offline', hideErrors: true })
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
const SKIP = new Set(['keep_alive', 'chunk_biomes'])
const FIELDS = {
  login: ['entityId', 'isHardcore', 'gameMode', 'previousGameMode', 'worldName', 'hashedSeed', 'maxPlayers', 'viewDistance', 'simulationDistance', 'reducedDebugInfo', 'enableRespawnScreen', 'isDebug', 'isFlat'],
  respawn: ['dimension', 'worldName', 'hashedSeed', 'gamemode', 'previousGamemode', 'isDebug', 'isFlat', 'copyMetadata'],
  position: ['x', 'y', 'z', 'yaw', 'pitch', 'flags'],
  update_view_position: ['chunkX', 'chunkZ'],
  game_state_change: ['reason', 'gameMode'],
  abilities: ['flags', 'flyingSpeed', 'walkingSpeed'],
  update_health: ['health', 'food', 'foodSaturation'],
  experience: ['experienceBar', 'level', 'totalExperience'],
  difficulty: ['difficulty', 'difficultyLocked'],
  entity_status: ['entityId', 'entityStatus'],
  held_item_slot: ['slotId'],
  world_event: ['effectId', 'data'],
  world_border: [],
  initialize_world_border: ['x', 'z', 'oldDiameter', 'newDiameter', 'speed', 'portalTeleportBoundary', 'warningBlocks', 'warningTime'],
  spawn_position: ['location', 'angle'],
  update_view_distance: ['viewDistance'],
  update_time: ['age', 'time'],
  window_items: ['windowId', 'stateId'],
  set_slot: ['windowId', 'stateId', 'slot'],
  entity_effect: ['entityId', 'effectId'],
  chunk_batch_finished: ['batchSize'],
  declare_commands: [],
  simulation_distance: ['distance'],
}
function show(name, data) {
  const f = FIELDS[name]
  if (!f || !f.length) return name
  const o = {}
  for (const k of f) if (data[k] !== undefined) o[k] = typeof data[k] === 'bigint' ? String(data[k]) : data[k]
  return name + ' ' + JSON.stringify(o)
}
let pos = { x: 0, y: 0, z: 0 }
client.on('packet', (data, meta) => {
  if (meta.state !== 'play' || SKIP.has(meta.name)) return
  if (meta.name === 'position') {
    pos = { x: data.x, y: data.y, z: data.z }
    client.write('teleport_confirm', { teleportId: data.teleportId })
    client.write('position_look', { x: data.x, y: data.y, z: data.z, yaw: 0, pitch: 0, onGround: true })
    if (!out.started) { out.started = true; setTimeout(run, 4000) }
  }
  mark.packets.push(show(meta.name, data))
})
const cmds = [
  'tp @s 9 52 9',
  'execute in minecraft:the_nether run tp @s 8 70 8',
  'execute in minecraft:overworld run tp @s 0 120 0',
  'execute in minecraft:the_end run tp @s 100 49 0',
  'execute in minecraft:the_nether run tp @s 8 70 8',
]
if (process.env.DIM_CMDS) { cmds.length = 0; cmds.push(...JSON.parse(process.env.DIM_CMDS)) }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
let seq = 0
const item = (id, n) => ({ present: true, itemId: id, itemCount: n, nbtData: undefined })
const place = (x, y, z, dir) => client.write('block_place', { hand: 0, location: { x, y, z }, direction: dir, cursorX: 0.5, cursorY: 1, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
const move = (x, y, z) => client.write('position', { x, y, z, onGround: true })
const ACTIONS = {
  async frame() {
    client.write('set_creative_slot', { slot: 36, item: item(mcData.itemsByName.obsidian.id, 64) })
    client.write('set_creative_slot', { slot: 37, item: item(mcData.itemsByName.flint_and_steel.id, 1) })
    await sleep(300)
    client.write('held_item_slot', { slotId: 0 })
    await sleep(300)
    const frame = []
    for (const x of [10, 11]) frame.push([x, 149, 0], [x, 153, 0])
    for (const x of [9, 12]) for (const y of [150, 151, 152]) frame.push([x, y, 0])
    for (const [x, y, z] of frame) { place(x, y - 1, z, 1); await sleep(60) }
    await sleep(500)
  },
  async ignite() {
    client.write('held_item_slot', { slotId: 1 })
    await sleep(300)
    place(10, 149, 0, 1)
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
  cmds.forEach((c, i) => setTimeout(() => {
    mark = { cmd: c, packets: [] }
    out.marks.push(mark)
    if (c.startsWith('raw:')) { ACTIONS[c.slice(4)]().catch((e) => out.errors.push(String(e))); return }
    client.write('chat_command', { command: c, timestamp: BigInt(Date.now()), salt: 0n, argumentSignatures: [], messageCount: 0, acknowledged: Buffer.alloc(3) })
  }, i * 7000))
  setTimeout(finish, cmds.length * 7000 + 3000)
}
function finish() {
  console.log(JSON.stringify(out))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}
setTimeout(finish, 90000)

'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const [host, portArg] = process.argv.slice(2)
const out = { marks: [], errors: [] }
let mark = { cmd: 'join', packets: [] }
out.marks.push(mark)
const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'DimBot', version: '1.20.1', auth: 'offline', hideErrors: true })
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
const SKIP = new Set(['keep_alive', 'chunk_biomes'])
const FIELDS = {
  login: ['entityId', 'isHardcore', 'gameMode', 'previousGameMode', 'worldName', 'maxPlayers', 'viewDistance', 'simulationDistance', 'reducedDebugInfo', 'enableRespawnScreen', 'isDebug', 'isFlat'],
  respawn: ['dimension', 'worldName', 'gamemode', 'previousGamemode', 'isDebug', 'isFlat', 'copyMetadata'],
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
client.on('packet', (data, meta) => {
  if (meta.state !== 'play' || SKIP.has(meta.name)) return
  if (meta.name === 'position') {
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
function run() {
  cmds.forEach((c, i) => setTimeout(() => {
    mark = { cmd: c, packets: [] }
    out.marks.push(mark)
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

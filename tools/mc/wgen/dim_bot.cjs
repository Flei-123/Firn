'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const [host, portArg] = process.argv.slice(2)
const out = { events: [], chunks: {}, errors: [] }
let current = 'login'
const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'DimBot', version: '1.20.1', auth: 'offline', hideErrors: true })
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
client.on('system_chat', (p) => out.events.push('chat ' + JSON.stringify(p.content).slice(0, 160)))
client.on('login', (p) => { current = p.worldName; out.events.push('login ' + p.worldName) })
client.on('respawn', (p) => { current = p.worldName; out.events.push('respawn ' + p.worldName) })
client.on('map_chunk', () => { out.chunks[current] = (out.chunks[current] || 0) + 1 })
client.on('position', (d) => {
  out.events.push(`position ${current} ${d.x} ${d.y} ${d.z}`)
  client.write('teleport_confirm', { teleportId: d.teleportId })
  client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
  if (!out.started) { out.started = true; run() }
})
const cmds = [
  'tp 9 52 9',
  'execute in minecraft:the_nether run tp 8 70 8',
  'execute in minecraft:overworld run tp 0 120 0',
  'execute in minecraft:the_end run tp 100 50 0',
  'execute in minecraft:the_nether run tp 8 70 8',
]
if (process.env.DIM_CMDS) { cmds.length = 0; cmds.push(...JSON.parse(process.env.DIM_CMDS)) }
function run() {
  cmds.forEach((c, i) => setTimeout(() => client.write('chat_command', { command: c, timestamp: BigInt(Date.now()), salt: 0n, argumentSignatures: [], messageCount: 0, acknowledged: Buffer.alloc(3) }), 3000 + i * 6000))
  setTimeout(finish, 3000 + cmds.length * 6000 + 4000)
}
function finish() {
  out.final = current
  console.log(JSON.stringify(out))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}
setTimeout(finish, 60000)

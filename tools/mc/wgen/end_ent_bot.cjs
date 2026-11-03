'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const [host, portArg] = process.argv.slice(2)
const out = { crystals: [], gateways: [], errors: [] }
const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'EndBot', version: '1.20.1', auth: 'offline', hideErrors: true })
const typeNames = {}
client.on('error', (e) => out.errors.push(String(e.message)))
client.on('kick_disconnect', (p) => out.errors.push('kick ' + JSON.stringify(p)))
client.on('login', () => { const reg = require(path.join(root, 'minecraft-data'))('1.20.1'); out.crystalType = reg.entitiesByName.end_crystal.id })
client.on('spawn_entity', (p) => { if (p.type === out.crystalType) out.crystals.push({ x: p.x, y: p.y, z: p.z, yaw: p.yaw }) })
client.on('map_chunk', (p) => { for (const be of p.blockEntities || []) out.gateways.push({ cx: p.x, cz: p.z, type: be.type, y: be.y, nbt: JSON.stringify(be.nbtData || null).slice(0, 200) }) })
client.on('position', (d) => {
  client.write('teleport_confirm', { teleportId: d.teleportId })
  client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
})
setTimeout(() => {
  out.crystals.sort((a, b) => a.x - b.x || a.z - b.z)
  console.log(JSON.stringify(out))
  try { client.end('bye') } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}, 12000)

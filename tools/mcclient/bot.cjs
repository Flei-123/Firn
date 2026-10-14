#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// bot.cjs -- a second client for the entity test of tools/mcclient/run.sh: it joins, stays online,
// answers the server (keep alive is automatic, teleports are confirmed) and moves on command.
//
//   NODE_PATH=... node bot.cjs <host> <port> <name>      commands on stdin, one per line:
//     walk <steps> <dx> <dz> [ms]   send <steps> position packets, each moving (dx, dz) blocks (the server
//                                   then broadcasts relative moves to the other players)
//     look <yaw> <pitch>            send a look packet (degrees)
//     quit
//   stdout: `ready` after the first position, `pos x y z` after every walk, `bye` on quit.
//
// MC_NMP = node-minecraft-protocol version string (as in ref_dump.cjs), MC_PROTOCOL = protocol number.
'use strict'
const mc = require('minecraft-protocol')
const readline = require('readline')

const host = process.argv[2]
const port = parseInt(process.argv[3], 10)
const name = process.argv[4]
const NMP = process.env.MC_NMP || '1.20.1'
const PROTOCOL = parseInt(process.env.MC_PROTOCOL || '763', 10)
const LEGACY8 = PROTOCOL <= 47
const SETTINGS_OLD = PROTOCOL <= 340
// 1.21.2+ (768): the movement packets carry `flags` {onGround, horizontalCollision} instead of onGround
const mv = (o) => (PROTOCOL >= 768 ? Object.assign({}, o, { onGround: undefined, flags: { onGround: !!o.onGround, horizontalCollision: false } }) : o)

// lpVec3 (1.21.9+ low precision vector) is unknown to node-minecraft-protocol 1.54: same reader as in ref_dump.cjs (the value is
// not used; without it the bot dies on the first entity_velocity / add_entity packet of 1.21.9+ and the entity scenario is empty).
{
  const cm = require('minecraft-protocol/src/datatypes/compiler-minecraft')
  const varintSize = (v) => { v >>>= 0; let n = 1; while (v >= 0x80) { n++; v >>>= 7 } return n }
  cm.Read.lpVec3 = ['native', (buffer, offset) => {
    const lowest = buffer.readUInt8(offset)
    if (lowest === 0) return { value: { lowest, middle: 0, highest: 0, ext: 0 }, size: 1 }
    const middle = buffer.readUInt8(offset + 1)
    const highest = buffer.readUInt32BE(offset + 2)
    let size = 6
    let ext = 0
    if ((lowest & 4) !== 0) {
      let shift = 0
      let b
      do { b = buffer.readUInt8(offset + size); size++; ext |= (b & 0x7f) << shift; shift += 7 } while (b & 0x80)
      ext >>>= 0
    }
    return { value: { lowest, middle, highest, ext }, size }
  }]
  cm.Write.lpVec3 = ['native', (value, buffer, offset) => {
    buffer.writeUInt8(value.lowest, offset++)
    return offset
  }]
  cm.SizeOf.lpVec3 = ['native', (value) => (value.lowest === 0 ? 1 : 6 + ((value.lowest & 4) !== 0 ? varintSize(value.ext) : 0))]
}

const client = mc.createClient({ host, port, username: name, version: NMP, auth: 'offline', viewDistance: '2' })
let pos = null // { x, y, z, yaw, pitch }
let ready = false

client.on('error', (e) => { console.error('bot error: ' + (e && e.message)) })
client.on('end', () => { console.log('bye'); process.exit(0) })
client.on('login', () => {
  if (SETTINGS_OLD) client.write('settings', { locale: 'en_US', viewDistance: 2, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1 })
  else client.write('settings', { locale: 'en_US', viewDistance: 2, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1, enableTextFiltering: false, enableServerListing: true, particleStatus: 0 })
})
client.on('chunk_batch_finished', () => { client.write('chunk_batch_received', { chunksPerTick: 10 }) })
client.on('position', (p) => {
  // absolute positions only (console tp / join); relative flags are applied for completeness
  const f = p.flags || 0
  const fl = typeof f === 'object' ? f : { x: f & 1, y: f & 2, z: f & 4, yaw: f & 8, pitch: f & 16 }
  const base = pos || { x: 0, y: 0, z: 0, yaw: 0, pitch: 0 }
  pos = {
    x: fl.x ? base.x + p.x : p.x, y: fl.y ? base.y + p.y : p.y, z: fl.z ? base.z + p.z : p.z,
    yaw: fl.yaw ? base.yaw + p.yaw : p.yaw, pitch: fl.pitch ? base.pitch + p.pitch : p.pitch
  }
  if (!LEGACY8) client.write('teleport_confirm', { teleportId: p.teleportId })
  client.write('position_look', mv({ x: pos.x, y: pos.y, z: pos.z, yaw: pos.yaw, pitch: pos.pitch, onGround: false }))
  if (!ready) { ready = true; console.log('ready') }
})

const rl = readline.createInterface({ input: process.stdin })
let busy = Promise.resolve()
rl.on('line', (line) => {
  busy = busy.then(() => run(line.trim().split(/\s+/)))
})
rl.on('close', () => { busy.then(() => { try { client.end('bot done') } catch (e) { /* ignore */ } setTimeout(() => process.exit(0), 100) }) })

function sleep (ms) { return new Promise((resolve) => setTimeout(resolve, ms)) }

async function run (a) {
  if (a[0] === 'walk' && pos) {
    const steps = parseInt(a[1], 10); const dx = parseFloat(a[2]); const dz = parseFloat(a[3])
    const ms = a[4] ? parseInt(a[4], 10) : 120
    for (let i = 0; i < steps; i++) {
      pos.x += dx; pos.z += dz
      client.write('position', mv({ x: pos.x, y: pos.y, z: pos.z, onGround: false }))
      await sleep(ms)
    }
    console.log(`pos ${pos.x} ${pos.y} ${pos.z}`)
  } else if (a[0] === 'look' && pos) {
    pos.yaw = parseFloat(a[1]); pos.pitch = parseFloat(a[2])
    client.write('look', mv({ yaw: pos.yaw, pitch: pos.pitch, onGround: false }))
    await sleep(100)
  } else if (a[0] === 'quit') {
    try { client.end('bot quit') } catch (e) { /* ignore */ }
  }
}

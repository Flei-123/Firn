#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// probe_tp.cjs -- passive packet probe for tools/mcclient/probe_tp.sh: joins, stays online and logs every inbound play packet
// whose name matches PROBE_RE (default: teleport / sync_entity / vehicle / position) with its numeric id and the raw body (hex).
// At the end (stdin `quit` or `end`) it prints `count <name> <n>` for ALL inbound play packets (so "never sent" is a measured fact).
//   NODE_PATH=... MC_NMP=1.21.4 node probe_tp.cjs <host> <port> <name> <logfile>
'use strict'
const mc = require('minecraft-protocol')
const fs = require('fs')
const readline = require('readline')

const [host, portS, name, logfile] = process.argv.slice(2)
const NMP = process.env.MC_NMP || '1.21.4'
const PROTOCOL = parseInt(process.env.MC_PROTOCOL || '769', 10)
const RE = new RegExp(process.env.PROBE_RE || 'teleport|sync_entity|vehicle|position|set_passengers|attach')
const out = fs.createWriteStream(logfile)
const counts = {}
{
  // lpVec3 (1.21.9+) is unknown to node-minecraft-protocol: skip it like bot.cjs does
  const cm = require('minecraft-protocol/src/datatypes/compiler-minecraft')
  cm.Read.lpVec3 = ['native', (buffer, offset) => {
    const lowest = buffer.readUInt8(offset)
    if (lowest === 0) return { value: { lowest, middle: 0, highest: 0, ext: 0 }, size: 1 }
    let size = 6
    if ((lowest & 4) !== 0) { let b; do { b = buffer.readUInt8(offset + size); size++ } while (b & 0x80) }
    return { value: { lowest, middle: buffer.readUInt8(offset + 1), highest: buffer.readUInt32BE(offset + 2), ext: 0 }, size }
  }]
  cm.Write.lpVec3 = ['native', (v, b, o) => { b.writeUInt8(v.lowest, o++); return o }]
  cm.SizeOf.lpVec3 = ['native', () => 1]
}
const client = mc.createClient({ host, port: parseInt(portS, 10), username: name, version: NMP, auth: 'offline', viewDistance: '2' })
client.on('error', (e) => console.error('probe error: ' + (e && e.message)))
client.on('end', () => finish())
client.on('login', () => {
  client.write('settings', { locale: 'en_US', viewDistance: 2, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1, enableTextFiltering: false, enableServerListing: true, particleStatus: 0 })
})
client.on('chunk_batch_finished', () => { client.write('chunk_batch_received', { chunksPerTick: 10 }) })
client.on('position', (p) => {
  const mv = { x: p.x, y: p.y, z: p.z, yaw: p.yaw, pitch: p.pitch }
  try { client.write('teleport_confirm', { teleportId: p.teleportId }) } catch (e) { /* ignore */ }
  try { client.write('position_look', Object.assign(mv, { flags: { onGround: true, horizontalCollision: false } })) } catch (e) { /* ignore */ }
})
let ready = false
// what node-minecraft-protocol (minecraft-data layout) makes of the teleport packets: shows whether the stale layout still parses
client.on('entity_teleport', (d) => { out.write(`${Date.now()} nmp_entity_teleport ${JSON.stringify(d)}\n`) })
// 4th argument `full` = the whole packet; the 3rd one (and the 'raw' event) is cut after the fields minecraft-data knows, which hid
// the real 64 byte entity_teleport body behind a 29 byte stale-layout slice in the first version of this probe
client.on('packet', (data, meta, cut, full) => {
  if (meta.state !== 'play') return
  const buf = (full && cut && full.length > cut.length) ? full : (cut || Buffer.alloc(0))
  counts[meta.name] = (counts[meta.name] || 0) + 1
  if (RE.test(meta.name)) out.write(`${Date.now()} ${meta.name} len=${buf.length} hex=${buf.toString('hex')}\n`)
  if (!ready && meta.name === 'position') { ready = true; console.log('ready') }
})
let done = false
function finish() {
  if (done) return
  done = true
  for (const k of Object.keys(counts).sort()) out.write(`count ${k} ${counts[k]}\n`)
  out.end(() => { console.log('bye'); process.exit(0) })
}
readline.createInterface({ input: process.stdin }).on('line', (l) => { if (l.trim() === 'quit') { try { client.end('bye') } catch (e) { /* ignore */ } finish() } })

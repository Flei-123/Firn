#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// sound_cap.cjs -- joins a server (node-minecraft-protocol) and prints one JSON line per sound related packet:
//   {"pkt":"sound_effect","id":<wire id>,"hex":"<body octets>","p":{parsed by nmp}}
// usage: NODE_PATH=... node sound_cap.cjs <host> <port> <name> [nmp-version]    (stops at EOF of stdin / SIGTERM)
'use strict'
const mc = require('minecraft-protocol')
const [host, port, name, nmp] = [process.argv[2], parseInt(process.argv[3], 10), process.argv[4], process.argv[5] || '1.20.1']
const client = mc.createClient({ host, port, username: name, version: nmp, auth: 'offline', viewDistance: '2' })
const WANT = new Set(['sound_effect', 'entity_sound_effect', 'stop_sound', 'named_sound_effect', 'world_event'])
function readVarint(b) {
  let v = 0, s = 0, i = 0
  for (;;) { const c = b[i++]; v |= (c & 0x7f) << s; if (!(c & 0x80)) break; s += 7 }
  return [v, i]
}
client.on('error', (e) => { console.error('cap error: ' + (e && e.message)) })
client.on('end', () => process.exit(0))
client.on('chunk_batch_finished', () => { client.write('chunk_batch_received', { chunksPerTick: 10 }) })
client.on('position', (p) => {
  client.write('teleport_confirm', { teleportId: p.teleportId })
  client.write('position_look', { x: p.x, y: p.y, z: p.z, yaw: 0, pitch: 0, onGround: true })
  if (!global.ready) { global.ready = true; console.log('ready') }
})
client.on('packet', (data, meta, buf) => {
  if (!WANT.has(meta.name)) return
  // buf = the raw packet (id varint + body) exactly as received, paired with the parse by nmp
  const [id, n] = readVarint(buf)
  console.log(JSON.stringify({ pkt: meta.name, id, hex: buf.subarray(n).toString('hex'), p: data }, (k, v) => typeof v === 'bigint' ? v.toString() : v))
})
process.stdin.resume()
process.stdin.on('end', () => process.exit(0))

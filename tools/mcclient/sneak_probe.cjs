#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// sneak_probe.cjs -- does a right click on a chest open it after Player Input (shift)? Independent check of the sneak packet
// (node-minecraft-protocol serialiser, not our encoder). Used by tools/mcclient/sneak_probe.sh.
//   NODE_PATH=... MC_NMP=1.21.4 node sneak_probe.cjs <host> <port> <name> <variant>
// variants: action (entity_action start_sneaking, <= 1.21.1), shift (player_input shift only), shift_loaded (also player_loaded first), none (no sneak), shift_move (shift + position packet)
// stdin `go`: run the click sequence, prints `result <variant> window=<0|1> placed=<0|1>` and exits on `quit`.
'use strict'
const mc = require('minecraft-protocol')
const readline = require('readline')
const [host, portS, name, variant] = process.argv.slice(2)
const NMP = process.env.MC_NMP || '1.21.4'
const client = mc.createClient({ host, port: parseInt(portS, 10), username: name, version: NMP, auth: 'offline', viewDistance: '2' })
client.on('error', (e) => console.error('probe error: ' + (e && e.message)))
let ownId = -1, pos = null, windowSeen = 0, placed = 0, seq = 0, chest = null
client.on('login', (d) => {
  ownId = d.entityId
  client.write('settings', { locale: 'en_US', viewDistance: 2, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1, enableTextFiltering: false, enableServerListing: true, particleStatus: 0 })
})
client.on('chunk_batch_finished', () => { client.write('chunk_batch_received', { chunksPerTick: 10 }) })
client.on('position', (p) => {
  pos = p
  try { client.write('teleport_confirm', { teleportId: p.teleportId }) } catch (e) {}
  try { client.write('position_look', { x: p.x, y: p.y, z: p.z, yaw: p.yaw, pitch: p.pitch, flags: { onGround: true, horizontalCollision: false } }) } catch (e) {}
  if (!client.__ready) {
    client.__ready = true; console.log('ready')
    // PROBE_EARLY=1: sneak before the server changes the game mode / gives the item (like mcplay with MCPLAY_FORCE_SNEAK=1)
    if (process.env.PROBE_EARLY || variant === 'early') setTimeout(() => {
      if (process.env.PROBE_LOADED) send('player_loaded', {})
      if (variant === 'action' || variant === 'early') send('entity_action', { entityId: ownId, actionId: 0, jumpBoost: 0 })
      else send('player_input', { inputs: { shift: true } })
    }, 500)
    // PROBE_TICK=1: send what a vanilla client sends every tick (client_tick_end + a status-only on-ground packet)
    if (process.env.PROBE_TICK) setInterval(() => { try { client.writeRaw(Buffer.from([0x0b])); client.writeRaw(Buffer.from([0x1f, 0x01])) } catch (e) {} }, 50)
  }
})
client.on('open_window', (d) => { windowSeen = 1; if (process.env.PROBE_DEBUG) console.log('open_window ' + JSON.stringify(d)) })
client.on('block_change', (d) => { if (process.env.PROBE_DEBUG && chest && d.location.x === chest.x && d.location.z === chest.z) console.log('block_change ' + JSON.stringify(d)); if (chest && d.type !== 0 && d.location.x === chest.x && d.location.y === chest.y + 1 && d.location.z === chest.z) placed = 1 })
client.on('entity_metadata', (d) => { if (process.env.PROBE_DEBUG && d.entityId === ownId) console.log('entity_metadata ' + JSON.stringify(d).slice(0, 300)) })
function send(n, d) { try { client.write(n, d) } catch (e) { console.error('write ' + n + ': ' + e.message) } }
readline.createInterface({ input: process.stdin }).on('line', async (l) => {
  if (l.trim() === 'quit') process.exit(0)
  if (l.trim() !== 'go' || !pos) return
  chest = { x: Math.floor(pos.x) + 1, y: Math.floor(pos.y), z: Math.floor(pos.z) }
  if (variant === 'shift_loaded') send('player_loaded', {})
  if (process.env.PROBE_DEBUG) { try { console.log('player_input bytes ' + client.serializer.createPacketBuffer({ name: 'player_input', params: { inputs: { shift: true } } }).toString('hex')) } catch (e) { console.log('serialize: ' + e.message) } }
  if (variant === 'early') { /* sneak was sent right after the join */ }
  else if (variant === 'raw_loaded') { client.writeRaw(Buffer.from([0x2a])); await new Promise((r) => setTimeout(r, 300)); client.writeRaw(Buffer.from([0x29, 0x20])) }
  else if (variant === 'raw') client.writeRaw(Buffer.from([0x29, 0x20]))
  else if (variant === 'action') send('entity_action', { entityId: ownId, actionId: 0, jumpBoost: 0 })
  else if (variant !== 'none') send('player_input', { inputs: { forward: false, backward: false, left: false, right: false, jump: false, shift: true, sprint: false } })
  await new Promise((r) => setTimeout(r, 600))
  if (variant === 'shift_move') send('position_look', { x: pos.x, y: pos.y, z: pos.z, yaw: pos.yaw, pitch: pos.pitch, flags: { onGround: true, horizontalCollision: false } })
  send('block_place', { hand: 0, location: chest, direction: 1, cursorX: 0.5, cursorY: 1, cursorZ: 0.5, insideBlock: false, worldBorderHit: false, sequence: ++seq })
  await new Promise((r) => setTimeout(r, 2000))
  console.log(`result ${variant} window=${windowSeen} placed=${placed} chest=${chest.x},${chest.y},${chest.z}`)
})

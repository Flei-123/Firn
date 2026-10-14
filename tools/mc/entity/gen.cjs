#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// gen.cjs -- random entity packet stream for one version, serialized by the protodef serializer
// (node-minecraft-protocol where it supports the version, else the same compiler on minecraft-data),
// plus the expected canonical entity text from tools/mcclient/ref_entities.cjs.
//
//   NODE_PATH=<tools/mcserver/node_modules> node gen.cjs <release> <outdir> [count=3000] [seed=1]
//
// writes <outdir>/script.txt   (`<packet name> <hex body>` lines, fed to tools/mc/entity/feed.fi)
//        <outdir>/expected.txt (what the reference tracker computes from the SAME parameter objects)
//        <outdir>/protocol     (protocol number)
// The field values are generated from the field list of the version's protocol.json (types decide the
// ranges: int8 angles over the full range, int16/int8 deltas over the full range, f64 positions with
// fractions, int32 fixed point for 1.8 ...), so nothing is written by hand per version.
'use strict'
const fs = require('fs')
const path = require('path')
const { playToClient } = require('./codec.cjs')
const { EntityTracker } = require('../../mcclient/ref_entities.cjs')

const release = process.argv[2]
const outDir = process.argv[3]
const COUNT = parseInt(process.argv[4] || '3000', 10)
const SEED = parseInt(process.argv[5] || '1', 10)
if (!release || !outDir) { console.error('usage: gen.cjs <release> <outdir> [count] [seed]'); process.exit(2) }
const V = require('../../mcgen/versions.json').versions[release]
if (!V) { console.error('unknown release ' + release); process.exit(2) }

function mulberry32 (a) {
  return function () {
    a |= 0; a = (a + 0x6D2B79F5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}
const rnd = mulberry32(SEED * 7919 + release.length)
const ri = (lo, hi) => lo + Math.floor(rnd() * (hi - lo + 1)) // inclusive
const pick = (a) => a[Math.floor(rnd() * a.length)]

const codec = playToClient(V.nmp || V.data || release)
const types = codec.data.protocol.play.toClient.types
const has = (n) => !!types['packet_' + n]
const fieldsOf = (n) => types['packet_' + n][1]
const FIXED = V.protocol <= 47
const tracker = new EntityTracker(V.protocol)

// ---- value generators by wire type
const uuids = []
function newUuid () {
  const h = () => ri(0, 0xffff).toString(16).padStart(4, '0')
  const u = `${h()}${h()}-${h()}-${h()}-${h()}-${h()}${h()}${h()}`
  return u
}
function genPos (type, name) {
  if (type === 'i32') return pick([0, -1, 1, 2147483647, -2147483648, ri(-100000, 100000), ri(-2000000000, 2000000000), 32 * ri(-5000, 5000)])
  const frac = [0, 0.5, 0.25, 0.1, 0.123456789, -0.987654321]
  return pick([0, 64, -64.5, ri(-30000000, 30000000) + pick(frac), (rnd() - 0.5) * 2000, (rnd() - 0.5) * 6e7, 1.0e-9, 255.999999])
}
function genScalar (type, name) {
  switch (type) {
    case 'varint': return ri(0, 200)
    case 'i8': return /^d[XYZ]$/.test(name) ? pick([-128, 127, 0, 1, -1, ri(-128, 127)]) : ri(-128, 127)
    case 'u8': return ri(0, 255)
    case 'i16': return /^d[XYZ]$/.test(name) ? pick([-32768, 32767, 0, 1, -1, ri(-32768, 32767)]) : ri(-32768, 32767)
    case 'i32': return /^[xyz]$/.test(name) ? genPos('i32', name) : ri(0, 5)
    case 'f64': return /^[xyz]$/.test(name) ? genPos('f64', name) : (rnd() - 0.5) * 8
    case 'f32': return Math.fround(pick([0, 90, -180, 359.5, (rnd() - 0.5) * 720, 1e-3]))
    case 'bool': return rnd() < 0.5
    case 'UUID': return newUuid()
    case 'vec3i16': return { x: ri(-32768, 32767), y: ri(-32768, 32767), z: ri(-32768, 32767) }
    case 'lpVec3': return rnd() < 0.5 ? { lowest: 0, middle: 0, highest: 0, ext: 0 } : { lowest: ri(1, 7), middle: ri(0, 255), highest: ri(0, 0xffffffff), ext: ri(0, 300) }
    case 'entityMetadata': return []
    default: return undefined
  }
}
function build (name, over) {
  const p = {}
  for (const f of fieldsOf(name)) {
    if (over && over[f.name] !== undefined) { p[f.name] = over[f.name]; continue }
    if (typeof f.type === 'string') {
      p[f.name] = genScalar(f.type, f.name)
    } else if (f.type[0] === 'switch') { // 1.8 spawn_entity velocity: only with objectData != 0
      p[f.name] = undefined
    } else if (f.type[0] === 'array') {
      p[f.name] = []
    } else {
      throw new Error(`${name}.${f.name}: unhandled type ${JSON.stringify(f.type)}`)
    }
  }
  if (name === 'spawn_entity' && FIXED) { p.objectData = 0; p.velocity = undefined }
  return p
}

// ---- the stream
const lines = []
const live = [] // ids of entities that should exist
let nextId = 1
let ok = 0
// 1.21.2+ (protocol 768+): minecraft-data still lists the old entity_teleport layout, the game sends
// id, pos f64x3, delta f64x3, yaw f32, pitch f32, flags i32, on ground = sync_entity_position + an i32 before the last octet
const TELEPORT_V21 = V.protocol >= 768
function teleportV21Body (params) {
  const sync = Buffer.from(codec.serialize('sync_entity_position', params))
  const flags = Buffer.alloc(4)
  flags.writeInt32BE(ri(-2147483648, 2147483647), 0)
  return Buffer.concat([sync.subarray(0, sync.length - 1), flags, sync.subarray(sync.length - 1)])
}
function emit (name, params) {
  let body
  if (name === 'entity_teleport' && TELEPORT_V21) {
    params = build('sync_entity_position', { entityId: params.entityId })
    body = teleportV21Body(params)
    params = Object.assign({ v21: true }, params)
  } else {
    body = codec.serialize(name, params)
  }
  lines.push(`${name} ${Buffer.from(body).toString('hex')}`)
  tracker.onPacket(name, params)
  ok++
}
const spawnNames = ['spawn_entity', 'spawn_entity_living', 'named_entity_spawn'].filter(has)
const moveNames = ['rel_entity_move', 'entity_move_look', 'entity_look', 'entity_teleport', 'sync_entity_position', 'entity_velocity'].filter(has)
const uuidKey = { spawn_entity: 'objectUUID', spawn_entity_living: 'entityUUID', named_entity_spawn: 'playerUUID' }
const infoField = fieldsOf('player_info').find((f) => f.name === 'action').type
const bitflags = Array.isArray(infoField) && infoField[0] === 'bitflags'
function flagsObj (names) {
  const o = {}
  let v = 0
  const all = infoField[1].flags
  all.forEach((n, i) => { o[n] = names.includes(n); if (names.includes(n)) v |= (1 << i) })
  o._value = v
  return o
}
function playerAdd (u, nm) {
  if (bitflags) {
    emit('player_info', { action: flagsObj(['add_player', 'update_latency']), data: [{ uuid: u, player: { name: nm, properties: [] }, latency: ri(0, 300) }] })
  } else {
    emit('player_info', { action: 'add_player', data: [{ uuid: u, name: nm, properties: [], gamemode: 0, ping: ri(0, 300), displayName: undefined }] })
  }
}
function playerRemove (u) {
  if (has('player_remove')) emit('player_remove', { players: [u] })
  else emit('player_info', { action: 'remove_player', data: [{ uuid: u }] })
}
const NAMES = ['Bot', 'Dumper', 'Alice', 'Zed_9', 'x', 'LongNameOf16Chr_']
const known = []

while (ok < COUNT) {
  const r = rnd()
  if (live.length < 3 || r < 0.18) { // spawn (also: respawn of a live id, the new one replaces it)
    const name = pick(spawnNames)
    const id = (live.length > 3 && rnd() < 0.1) ? pick(live) : nextId++
    const over = { entityId: id }
    const uk = uuidKey[name]
    if (uk && rnd() < 0.5 && known.length) over[uk] = pick(known).uuid // a known player: gets its name
    if (name === 'named_entity_spawn' && !over.playerUUID && known.length && rnd() < 0.7) over.playerUUID = pick(known).uuid
    emit(name, build(name, over))
    if (!live.includes(id)) live.push(id)
  } else if (r < 0.62) { // move / look / teleport / sync / velocity
    const name = pick(moveNames)
    const id = rnd() < 0.08 ? 900000 + ri(0, 50) : pick(live) // some unknown ids
    emit(name, build(name, { entityId: id }))
  } else if (r < 0.72 && live.length > 3) { // destroy 1-3
    const n = ri(1, 3)
    const ids = []
    for (let i = 0; i < n; i++) {
      const id = rnd() < 0.1 ? 800000 + ri(0, 9) : live.splice(ri(0, live.length - 1), 1)[0]
      if (id !== undefined) ids.push(id)
    }
    emit('entity_destroy', { entityIds: ids })
  } else if (r < 0.85) { // player added
    const u = newUuid()
    const nm = pick(NAMES)
    known.push({ uuid: u, name: nm })
    playerAdd(u, nm)
  } else if (r < 0.9 && known.length) { // player removed
    const i = ri(0, known.length - 1)
    playerRemove(known[i].uuid)
    known.splice(i, 1)
  } else if (r < 0.93 && bitflags) { // info without a name (latency update only)
    emit('player_info', { action: flagsObj(['update_latency']), data: [{ uuid: pick(known.length ? known : [{ uuid: newUuid() }]).uuid, latency: 5 }] })
  }
}
fs.mkdirSync(outDir, { recursive: true })
fs.writeFileSync(path.join(outDir, 'script.txt'), lines.join('\n') + '\n')
fs.writeFileSync(path.join(outDir, 'expected.txt'), tracker.dump())
fs.writeFileSync(path.join(outDir, 'protocol'), String(V.protocol))
const c = tracker.count
console.log(`gen ${release} protocol ${V.protocol}: ${lines.length} packets, ${tracker.ents.size} entities at the end, spawns=${c.spawn} moves=${c.move} teleports=${c.teleport} looks=${c.look} destroys=${c.destroy} infos=${c.info} misses=${c.miss}`)

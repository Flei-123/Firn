#!/usr/bin/env node
// ===================================================================================
// MOCK - not a real server.
// ===================================================================================
// Emergency substitute for the real 1.20.1 server of tools/mcclient/run.sh. It speaks just enough
// of protocol 763 (offline login, Join Game, position, deterministic map_chunk packets) to let a
// client be exercised. It proves NOTHING about compatibility with the real game: the chunk bytes
// are serialized by this file itself, so a client that agrees with it may still be wrong against
// a vanilla server. A test run against this server must never be reported as "tested against a
// real server".
//
//   NODE_PATH=/root/firn/tools/mcserver/node_modules node mock_server.cjs [port=0] [view_distance=4]
//
// Prints `MOCK - not a real server listening on 127.0.0.1:<port>` once ready.
// Chunk content is a pure function of (x, z); the data also covers the cases a decoder must
// handle: single-value containers, indirect 4/5 bit palettes, a direct 15 bit container, and the
// trailing zero padding real servers append to chunkData.
'use strict'

const mc = require('minecraft-protocol')
const nbt = require('prismarine-nbt')
const mcData = require('minecraft-data')('1.20.1')

const port = parseInt(process.argv[2] || '0', 10)
const viewDistance = parseInt(process.argv[3] || '4', 10)
const NUM_SECTIONS = 24

function rng (seed) {
  let s = (seed >>> 0) || 1
  return () => {
    s ^= s << 13; s >>>= 0
    s ^= s >>> 17
    s ^= s << 5; s >>>= 0
    return s
  }
}

function varint (v) {
  const out = []
  do {
    let b = v & 0x7f
    v >>>= 7
    if (v) b |= 0x80
    out.push(b)
  } while (v)
  return Buffer.from(out)
}

function packLongs (vals, bits) {
  const per = Math.floor(64 / bits)
  const n = Math.ceil(vals.length / per)
  const b = Buffer.alloc(n * 8)
  for (let l = 0; l < n; l++) {
    let x = 0n
    for (let k = 0; k < per; k++) {
      const i = l * per + k
      if (i >= vals.length) break
      x |= BigInt(vals[i]) << BigInt(k * bits)
    }
    b.writeBigUInt64BE(x, l * 8)
  }
  return b
}

// kind: 'blocks' | 'biomes'; mode: 'single' | 'indirect' | 'direct'
function container (vals, kind, mode) {
  const parts = []
  if (mode === 'single') {
    parts.push(Buffer.from([0]), varint(vals[0]), varint(0))
    return Buffer.concat(parts)
  }
  if (mode === 'indirect') {
    const palette = [...new Set(vals)]
    let bits = Math.max(1, Math.ceil(Math.log2(Math.max(palette.length, 2))))
    if (kind === 'blocks') bits = Math.max(4, bits)
    const idx = vals.map((v) => palette.indexOf(v))
    parts.push(Buffer.from([bits]), varint(palette.length), ...palette.map(varint))
    const longs = packLongs(idx, bits)
    parts.push(varint(longs.length / 8), longs)
    return Buffer.concat(parts)
  }
  const bits = kind === 'blocks' ? 15 : 6
  const longs = packLongs(vals, bits)
  return Buffer.concat([Buffer.from([bits]), varint(longs.length / 8), longs])
}

function buildChunk (cx, cz) {
  const r = rng(((cx * 73856093) ^ (cz * 19349663)) >>> 0)
  const secs = []
  let nonAirTotal = 0
  for (let s = 0; s < NUM_SECTIONS; s++) {
    let blocks; let mode; let count
    if (s >= 6) {
      blocks = new Array(4096).fill(0); mode = 'single'; count = 0
    } else {
      const n = 4 + (r() % 14)
      const pal = []
      for (let i = 0; i < n; i++) pal.push(i === 0 ? 0 : 1 + (r() % 20000))
      blocks = new Array(4096)
      for (let i = 0; i < 4096; i++) blocks[i] = pal[r() % n]
      mode = (s === 1 && ((cx + cz) % 5 === 0)) ? 'direct' : 'indirect'
      if (mode === 'direct') for (let i = 0; i < 4096; i += 7) blocks[i] = 300 + (r() % 15000)
      count = blocks.filter((v) => v !== 0).length
    }
    nonAirTotal += count
    const bio = new Array(64)
    const bmode = s % 3 === 0 ? 'single' : (s % 3 === 1 ? 'indirect' : 'direct')
    for (let i = 0; i < 64; i++) bio[i] = bmode === 'single' ? 5 : (bmode === 'indirect' ? (r() % 3) * 11 : r() % 64)
    const head = Buffer.alloc(2); head.writeInt16BE(count)
    secs.push(Buffer.concat([head, container(blocks, 'blocks', mode), container(bio, 'biomes', bmode)]))
  }
  // real servers pad chunkData with zero bytes
  const chunkData = Buffer.concat([...secs, Buffer.alloc(15 + (r() % 2))])
  const hm = []
  for (let i = 0; i < 256; i++) hm.push(64 + (r() % 40))
  const longs = packLongs(hm, 9)
  const hmLongs = []
  for (let i = 0; i < longs.length; i += 8) hmLongs.push([longs.readInt32BE(i), longs.readInt32BE(i + 4)])
  const heightmaps = nbt.comp({ MOTION_BLOCKING: nbt.longArray(hmLongs), WORLD_SURFACE: nbt.longArray(hmLongs) })
  const lightArr = () => { const a = []; for (let i = 0; i < 2048; i++) a.push(r() & 0xff); return a }
  const skyBits = 0b111; const blockBits = 0b1111
  const skyLight = []; const blockLight = []
  for (let i = 0; i < 3; i++) skyLight.push(lightArr())
  for (let i = 0; i < 4; i++) blockLight.push(lightArr())
  const blockEntities = []
  if ((cx + cz) % 3 === 0) blockEntities.push({ x: 3, z: 4, y: 70, type: 1, nbtData: undefined })
  return {
    x: cx, z: cz, heightmaps, chunkData, blockEntities,
    skyLightMask: [[0, skyBits]], blockLightMask: [[0, blockBits]], emptySkyLightMask: [[0, 0]], emptyBlockLightMask: [[0, 0]],
    skyLight, blockLight
  }
}

const server = mc.createServer({ 'online-mode': false, version: '1.20.1', host: '127.0.0.1', port, maxPlayers: 4, motd: 'MOCK - not a real server', keepAlive: true })

server.on('listening', () => {
  console.log(`MOCK - not a real server listening on 127.0.0.1:${server.socketServer.address().port}`)
})

server.on('login', (client) => {
  console.log(`MOCK - not a real server: ${client.username} joined`)
  client.on('end', () => console.log(`MOCK - not a real server: ${client.username} left`))
  client.on('error', () => {})
  client.write('login', {
    ...mcData.loginPacket,
    entityId: 1,
    isHardcore: false,
    gameMode: 1,
    previousGameMode: -1,
    worldNames: ['minecraft:overworld'],
    worldName: 'minecraft:overworld',
    worldType: 'minecraft:overworld',
    hashedSeed: [0, 0],
    maxPlayers: 4,
    viewDistance,
    simulationDistance: viewDistance,
    reducedDebugInfo: false,
    enableRespawnScreen: true,
    isDebug: false,
    isFlat: false,
    death: undefined
  })
  client.write('position', { x: 8, y: 70, z: 8, yaw: 0, pitch: 0, flags: 0, teleportId: 1 })
  client.write('update_view_position', { chunkX: 0, chunkZ: 0 })
  const list = []
  for (let x = -viewDistance; x <= viewDistance; x++) for (let z = -viewDistance; z <= viewDistance; z++) list.push([x, z])
  list.sort((a, b) => (a[0] ** 2 + a[1] ** 2) - (b[0] ** 2 + b[1] ** 2) || a[0] - b[0] || a[1] - b[1])
  let i = 0
  const timer = setInterval(() => {
    if (i >= list.length || client.state === undefined) { clearInterval(timer); return }
    try { client.write('map_chunk', buildChunk(list[i][0], list[i][1])) } catch (e) { console.error('MOCK write failed: ' + e.message); clearInterval(timer) }
    i++
  }, 2)
  client.on('end', () => clearInterval(timer))
})

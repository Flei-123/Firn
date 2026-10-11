#!/usr/bin/env node
// tools/mc/online/server.cjs -- LOCAL ONLINE-MODE TEST SERVER (node-minecraft-protocol) + MOCK SESSION SERVER.
//
//   NODE_PATH=/root/firn/tools/mcserver/node_modules node server.cjs <version> <ready-file> [mode]
//
// It is a test double, not the real game: node-minecraft-protocol verifies the player through
// yggdrasil.hasJoined; this file points that call to a mock session server on 127.0.0.1 that
//   * POST /session/minecraft/join       checks accessToken + selectedProfile against the fixture
//                                        account (env MOCK_TOKEN, MOCK_UUID, MOCK_NAME), stores the hash
//                                        (204) or answers 403 ForbiddenOperationException "Invalid session"
//   * GET  /session/minecraft/hasJoined  the server computes the hash ITSELF (java style digest of
//                                        serverId + secret + public key, in the yggdrasil module) and the mock
//                                        answers the profile only if the CLIENT posted exactly that hash
// so the hash of lib/mc/mc_auth.fi is checked against an independent implementation. The token value is
// never printed. Modes (3rd argument):
//   normal       online-mode, join works
//   badtoken     server expects another verify token than the one it sent  -> "did not encrypt properly"
//   sessiononly  only the mock session server (for a real vanilla server started with
//                -Dminecraft.api.env=custom -Dminecraft.api.session.host=http://127.0.0.1:<session port>)
//   dropjoin     the mock accepts the join (204) but forgets the hash       -> "Failed to verify username!"
// Chunks (1.20.1 only): deterministic columns of mcclient/mock_server.cjs; other versions: login only, then a kick.
// Writes `port=<game> session=<mock>` into <ready-file> when ready; prints `JOIN-OK <name>` lines.
'use strict'
const http = require('http')
const fs = require('fs')
const ygg = require('yggdrasil')
const mc = require('minecraft-protocol')

const version = process.argv[2] || '1.20.1'
const readyFile = process.argv[3]
const mode = process.argv[4] || 'normal'
const TOKEN = process.env.MOCK_TOKEN || ''
const UUID = (process.env.MOCK_UUID || '').toLowerCase()
const NAME = process.env.MOCK_NAME || ''
const VIEW = 3

const hashes = new Map() // hash -> { id, name }
const log = (s) => console.log(s)

const sessionServer = http.createServer((req, res) => {
  const u = new URL(req.url, 'http://127.0.0.1')
  if (req.method === 'POST' && u.pathname === '/session/minecraft/join') {
    let body = ''
    req.on('data', (d) => { body += d })
    req.on('end', () => {
      let j = {}
      try { j = JSON.parse(body) } catch (e) { /* stays empty */ }
      const okTok = typeof j.accessToken === 'string' && j.accessToken === TOKEN && TOKEN.length > 0
      const okId = typeof j.selectedProfile === 'string' && j.selectedProfile.toLowerCase() === UUID
      const okHash = typeof j.serverId === 'string' && /^-?[0-9a-f]{1,40}$/.test(j.serverId)
      if (okTok && okId && okHash) {
        if (mode !== 'dropjoin') hashes.set(j.serverId, { id: UUID, name: NAME })
        log(`SESSION join accepted hash_len=${j.serverId.length}`)
        res.writeHead(204); res.end()
      } else {
        log(`SESSION join REFUSED token_ok=${okTok} profile_ok=${okId} hash_ok=${okHash}`)
        res.writeHead(403, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'ForbiddenOperationException', errorMessage: 'Invalid session' }))
      }
    })
    return
  }
  if (req.method === 'GET' && u.pathname === '/session/minecraft/hasJoined') {
    const h = u.searchParams.get('serverId')
    const e = hashes.get(h)
    if (e && u.searchParams.get('username') === e.name) {
      log('SESSION hasJoined hit')
      res.writeHead(200, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ id: e.id, name: e.name, properties: [] }))
    } else {
      log('SESSION hasJoined MISS (no matching hash posted by the client)')
      res.writeHead(204); res.end()
    }
    return
  }
  res.writeHead(404); res.end()
})

// column generator copied from tools/mcclient/mock_server.cjs (deterministic, palette/direct containers)
const nbt = require('prismarine-nbt')
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


sessionServer.listen(0, '127.0.0.1', () => {
  const sport = sessionServer.address().port
  if (mode === 'sessiononly') { fs.writeFileSync(readyFile, `port=0 session=${sport}\n`); log('READY'); return }
  const origServer = ygg.server
  ygg.server = (o) => origServer({ ...(o || {}), host: `http://127.0.0.1:${sport}` })
  const server = mc.createServer({ 'online-mode': true, version, host: '127.0.0.1', port: 0, maxPlayers: 4, motd: 'online test', keepAlive: true, hideErrors: true })
  server.on('listening', () => {
    fs.writeFileSync(readyFile, `port=${server.socketServer.address().port} session=${sport}\n`)
    log('READY')
  })
  server.on('connection', (client) => {
    const w = client.write.bind(client)
    client.write = (name, params) => {
      // node-minecraft-protocol never sets the 1.20.5+ flag (it would write false); a vanilla server says true
      if (name === 'encryption_begin') params = { ...params, shouldAuthenticate: true }
      w(name, params)
      if (name === 'encryption_begin' && mode === 'badtoken') client.verifyToken = Buffer.from('zzzz') // expects another token than it sent
    }
  })
  server.on('login', (client) => {
    log(`JOIN-OK ${client.username} uuid_ok=${(client.uuid || '').replace(/-/g, '') === UUID}`)
    client.on('end', () => log(`LEFT ${client.username}`))
    client.on('error', () => {})
    if (version !== '1.20.1') { setTimeout(() => client.end('login-only test'), 500); return }
    const mcData = require('minecraft-data')(version)
    client.write('login', { ...mcData.loginPacket, entityId: 1, isHardcore: false, gameMode: 1, previousGameMode: -1, worldNames: ['minecraft:overworld'], worldName: 'minecraft:overworld', worldType: 'minecraft:overworld', hashedSeed: [0, 0], maxPlayers: 4, viewDistance: VIEW, simulationDistance: VIEW, reducedDebugInfo: false, enableRespawnScreen: true, isDebug: false, isFlat: false, death: undefined })
    client.write('position', { x: 8, y: 70, z: 8, yaw: 0, pitch: 0, flags: 0, teleportId: 1 })
    client.write('update_view_position', { chunkX: 0, chunkZ: 0 })
    const list = []
    for (let x = -VIEW; x <= VIEW; x++) for (let z = -VIEW; z <= VIEW; z++) list.push([x, z])
    let i = 0
    const t = setInterval(() => {
      if (i >= list.length || client.state === undefined) { clearInterval(t); return }
      try { client.write('map_chunk', buildChunk(list[i][0], list[i][1])) } catch (e) { log('CHUNK-WRITE-FAILED ' + e.message); clearInterval(t) }
      i++
    }, 2)
    client.on('end', () => clearInterval(t))
  })
})
process.on('SIGTERM', () => process.exit(0))

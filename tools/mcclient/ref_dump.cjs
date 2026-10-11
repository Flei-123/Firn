#!/usr/bin/env node
// ref_dump.cjs -- reference client for the M0 protocol prototype (MC 1.20.1, protocol 763).
//
//   NODE_PATH=/root/firn/tools/mcserver/node_modules \
//     node ref_dump.cjs <host> <port> <name> <dump_file> [view_distance=4] [idle_ms=2500]
//   node ref_dump.cjs --selftest
//
// Uses node-minecraft-protocol only for the transport / packet parsing (version '1.20.1',
// offline auth). The chunk decoding below is an independent implementation written from the
// public protocol documentation (palette containers, packed long arrays, light arrays); it does
// not share any code with the Firn client. It writes the canonical dump of
// demos/mcclient/DESIGN.md byte for byte and prints `metric key=value` lines.
//
// Dump rules that DESIGN.md leaves open (the Firn client must do the same):
//   * a second map_chunk for the same (x, z) replaces the first one (last wins);
//   * unload_chunk packets are ignored (the dump is "everything that was sent");
//   * sections = number of sections of the dimension (height / 16 from the Join Game codec); the vanilla
//     server pads chunkData with zero octets behind them, so "until the buffer ends" is wrong.
'use strict'

const fs = require('fs')
const crypto = require('crypto')

// ---------------------------------------------------------------- FNV-1a 64 (two 32-bit halves)
class Fnv64 {
  constructor () { this.hi = 0xcbf29ce4; this.lo = 0x84222325 }
  byte (b) {
    // h = (h ^ b) * 0x100000001b3 mod 2^64;  0x100000001b3 = 2^40 + 0x1b3
    const lo = (this.lo ^ b) >>> 0
    const hi = this.hi
    const a = lo * 435
    const newLo = a % 4294967296
    const carry = Math.floor(a / 4294967296)
    const newHi = (hi * 435 + carry + (lo & 0x00ffffff) * 256) % 4294967296
    this.lo = newLo
    this.hi = newHi
  }
  u16le (v) { this.byte(v & 0xff); this.byte((v >>> 8) & 0xff) }
  hex () {
    return this.hi.toString(16).padStart(8, '0') + this.lo.toString(16).padStart(8, '0')
  }
}

function fnvBigInt (bytes) {
  let h = 0xcbf29ce484222325n
  for (const b of bytes) {
    h ^= BigInt(b)
    h = BigInt.asUintN(64, h * 0x100000001b3n)
  }
  return h.toString(16).padStart(16, '0')
}

function selftest () {
  const vec = Buffer.from('a')
  const f = new Fnv64(); for (const b of vec) f.byte(b)
  const ok1 = f.hex() === 'af63dc4c8601ec8c'
  let ok2 = true
  let seed = 12345
  for (let t = 0; t < 200; t++) {
    const n = 1 + (t * 37) % 5000
    const buf = Buffer.alloc(n)
    for (let i = 0; i < n; i++) { seed = (seed * 1103515245 + 12345) & 0x7fffffff; buf[i] = (seed >>> 8) & 0xff }
    const g = new Fnv64(); for (const b of buf) g.byte(b)
    if (g.hex() !== fnvBigInt(buf)) { ok2 = false; break }
  }
  // packed array unpack check: 5 bits per entry, values 0..31 repeated
  const vals = []; for (let i = 0; i < 100; i++) vals.push((i * 7 + 3) % 32)
  const longs = packLongs(vals, 5)
  const back = unpackLongs(longs, 5, 100)
  const ok3 = back.every((v, i) => v === vals[i])
  console.log(`selftest fnv_vector=${ok1} fnv_vs_bigint=${ok2} pack_roundtrip=${ok3}`)
  process.exit(ok1 && ok2 && ok3 ? 0 : 1)
}

// ---------------------------------------------------------------- packed long arrays
// longs: array of [hi, lo] unsigned 32-bit pairs. No entry spans two longs.
function unpackLongs (longs, bits, count) {
  const per = Math.floor(64 / bits)
  const mask = bits >= 32 ? 0xffffffff : (2 ** bits) - 1
  const out = new Array(count)
  for (let i = 0; i < count; i++) {
    const li = Math.floor(i / per)
    if (li >= longs.length) throw new Error('packed array too short')
    const s = (i % per) * bits
    const hi = longs[li][0]; const lo = longs[li][1]
    let v
    if (s + bits <= 32) v = Math.floor(lo / 2 ** s) % (mask + 1)
    else if (s >= 32) v = Math.floor(hi / 2 ** (s - 32)) % (mask + 1)
    else {
      const loPart = Math.floor(lo / 2 ** s) // bits s..31 of lo
      const nLo = 32 - s
      const hiPart = hi % (2 ** (bits - nLo))
      v = loPart + hiPart * 2 ** nLo
    }
    out[i] = v
  }
  return out
}

// test helper (inverse of unpackLongs) -- BigInt is fine here
function packLongs (vals, bits) {
  const per = Math.floor(64 / bits)
  const n = Math.ceil(vals.length / per)
  const res = []
  for (let l = 0; l < n; l++) {
    let x = 0n
    for (let k = 0; k < per; k++) {
      const i = l * per + k
      if (i >= vals.length) break
      x |= BigInt(vals[i]) << BigInt(k * bits)
    }
    res.push([Number(x >> 32n), Number(x & 0xffffffffn)])
  }
  return res
}

// ---------------------------------------------------------------- buffer reader
class Reader {
  constructor (buf) { this.b = buf; this.p = 0 }
  eof () { return this.p >= this.b.length }
  u8 () { if (this.p >= this.b.length) throw new Error('read past end'); return this.b[this.p++] }
  i16 () { const v = this.b.readInt16BE(this.p); this.p += 2; return v }
  varint () {
    let r = 0; let shift = 0
    for (;;) {
      const x = this.u8()
      r += (x & 0x7f) * 2 ** shift
      if (!(x & 0x80)) break
      shift += 7
      if (shift > 35) throw new Error('varint too long')
    }
    return r
  }
  longs (n) {
    if (this.p + n * 8 > this.b.length) throw new Error('long array past end')
    const out = new Array(n)
    for (let i = 0; i < n; i++) {
      out[i] = [this.b.readUInt32BE(this.p), this.b.readUInt32BE(this.p + 4)]
      this.p += 8
    }
    return out
  }
}

// One paletted container. kind 'blocks' (4096, indirect 4..8) or 'biomes' (64, indirect 1..3).
// Returns array of global ids.
function readContainer (r, kind) {
  const count = kind === 'blocks' ? 4096 : 64
  const maxIndirect = kind === 'blocks' ? 8 : 3
  const minIndirect = kind === 'blocks' ? 4 : 1
  let bits = r.u8()
  if (bits === 0) {
    const v = r.varint()
    const len = r.varint()
    if (len !== 0) throw new Error('single-value container with data')
    return new Array(count).fill(v)
  }
  let palette = null
  if (bits <= maxIndirect) {
    if (bits < minIndirect) bits = minIndirect // blocks: servers never send <4, tolerate
    const n = r.varint()
    palette = new Array(n)
    for (let i = 0; i < n; i++) palette[i] = r.varint()
  }
  const nl = r.varint()
  const per = Math.floor(64 / bits)
  if (nl !== Math.ceil(count / per)) throw new Error(`container long count ${nl} != ${Math.ceil(count / per)} (bits ${bits})`)
  const ids = unpackLongs(r.longs(nl), bits, count)
  if (palette) {
    for (let i = 0; i < count; i++) {
      if (ids[i] >= palette.length) throw new Error('palette index out of range')
      ids[i] = palette[ids[i]]
    }
  }
  return ids
}

function hashU16 (arr) {
  const f = new Fnv64()
  for (let i = 0; i < arr.length; i++) f.u16le(arr[i])
  return f.hex()
}

// The server pads chunkData with trailing zero bytes (buffer pre-sized too large), so the
// number of sections is NOT "until the buffer ends": it is world height / 16, taken from the
// dimension type of the Join Game packet (24 for the vanilla overworld). Extra bytes are ignored
// but must be zero (reported otherwise).
let trailingNonZero = 0
function decodeSections (chunkData, numSections) {
  const r = new Reader(chunkData)
  const sections = []
  while (sections.length < numSections) {
    const blockCount = r.i16()
    const blocks = readContainer(r, 'blocks')
    const biomes = readContainer(r, 'biomes')
    sections.push({ blockCount, blocksHash: hashU16(blocks), biomesHash: hashU16(biomes), distinct: new Set(blocks).size })
  }
  for (let i = r.p; i < chunkData.length; i++) if (chunkData[i] !== 0) { trailingNonZero++; break }
  return sections
}

// nmp gives long arrays as [hi, lo] signed int32 pairs
function heightmapValues (arr) {
  const longs = arr.map(([h, l]) => [h >>> 0, l >>> 0])
  return unpackLongs(longs, 9, 256)
}

function decodeChunk (p, numSections) {
  const sections = decodeSections(p.chunkData, numSections)
  const hm = []
  const root = p.heightmaps
  const entries = (root && root.value) || {}
  for (const name of Object.keys(entries || {})) {
    const e = entries[name]
    if (!e || e.type !== 'longArray') continue
    hm.push({ name, hash: hashU16(heightmapValues(e.value)) })
  }
  hm.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
  const f = new Fnv64()
  for (const arr of [...p.skyLight, ...p.blockLight]) for (const b of arr) f.byte(b)
  return {
    x: p.x,
    z: p.z,
    sections,
    blockEntities: p.blockEntities.length,
    hm,
    light: { sky: p.skyLight.length, block: p.blockLight.length, h: f.hex() }
  }
}

function buildDump (chunks) {
  const list = [...chunks.values()].sort((a, b) => a.x - b.x || a.z - b.z)
  let s = 'mcdump 1\nprotocol 763\n'
  s += `chunks ${list.length}\n`
  for (const c of list) {
    s += `C ${c.x} ${c.z} sections=${c.sections.length} blockentities=${c.blockEntities}\n`
    c.sections.forEach((sec, i) => {
      s += `S ${c.x} ${c.z} ${i} count=${sec.blockCount} distinct=${sec.distinct} blocks=${sec.blocksHash} biomes=${sec.biomesHash}\n`
    })
    for (const h of c.hm) s += `H ${c.x} ${c.z} ${h.name} ${h.hash}\n`
    s += `L ${c.x} ${c.z} sky=${c.light.sky} block=${c.light.block} h=${c.light.h}\n`
  }
  const sum = crypto.createHash('sha256').update(s, 'utf8').digest('hex')
  return { text: s + `checksum sha256 ${sum}\n`, sum }
}

// ---------------------------------------------------------------- main
function main () {
  const [host, portS, name, dumpFile, vdS, idleS] = process.argv.slice(2)
  if (!host || !portS || !name || !dumpFile) {
    console.error('usage: ref_dump.cjs <host> <port> <name> <dump_file> [view_distance=4] [idle_ms=2500]')
    process.exit(2)
  }
  const port = parseInt(portS, 10)
  const viewDistance = parseInt(vdS || '4', 10)
  const idleMs = parseInt(idleS || '2500', 10)
  const mc = require('minecraft-protocol')

  const t0 = Date.now()
  const m = { connect_ms: -1, join_ms: -1, first_chunk_ms: -1 }
  let packets = 0; let bytesIn = 0; let bytesDec = 0; let walkOk = 0; let walkFail = 0
  const chunks = new Map()
  let idleTimer = null
  let finished = false
  let chunkErrors = 0
  let numSections = 24

  const client = mc.createClient({ host, port, username: name, version: '1.20.1', auth: 'offline', viewDistance: String(viewDistance), hideErrors: false })

  const finish = (code, msg) => {
    if (finished) return
    finished = true
    if (idleTimer) clearTimeout(idleTimer)
    const total = Date.now() - t0
    try { client.end('done') } catch (e) { /* ignore */ }
    if (msg) console.error(msg)
    if (code !== 0) process.exit(code)
    const { text, sum } = buildDump(chunks)
    fs.writeFileSync(dumpFile, text)
    const secs = Math.max(total, 1) / 1000
    const out = {
      connect_ms: m.connect_ms,
      join_ms: m.join_ms,
      first_chunk_ms: m.first_chunk_ms,
      total_ms: total,
      packets,
      bytes_in: bytesIn,
      bytes_decompressed: bytesDec,
      packets_per_s: (packets / secs).toFixed(1),
      chunks: chunks.size,
      walk_ok: walkOk,
      walk_fail: walkFail + chunkErrors,
      num_sections: numSections,
      trailing_nonzero_chunks: trailingNonZero,
      checksum: sum
    }
    for (const k of Object.keys(out)) console.log(`metric ${k}=${out[k]}`)
    setTimeout(() => process.exit(0), 50)
  }

  const armIdle = () => {
    if (idleTimer) clearTimeout(idleTimer)
    idleTimer = setTimeout(() => finish(0), idleMs)
  }

  client.on('connect', () => { m.connect_ms = Date.now() - t0 })
  client.on('error', (e) => { walkFail++; console.error('client error: ' + (e && e.message)) })
  client.on('end', (r) => { if (!finished) finish(3, 'disconnected before done: ' + r) })
  client.on('kick_disconnect', (p) => console.error('kicked: ' + JSON.stringify(p)))
  client.on('disconnect', (p) => console.error('disconnect: ' + JSON.stringify(p)))

  // count wire bytes (compressed, encrypted-free offline) at the socket
  client.on('connect', () => {
    if (client.socket) client.socket.on('data', (d) => { bytesIn += d.length })
  })
  client.on('packet', (data, meta, buffer, full) => {
    packets++; walkOk++
    bytesDec += (full || buffer || Buffer.alloc(0)).length
  })

  client.on('login', (p) => {
    m.join_ms = Date.now() - t0
    try {
      const nbt = require('prismarine-nbt')
      const codec = nbt.simplify(p.dimensionCodec)
      const list = codec['minecraft:dimension_type'].value
      const e = list.find((x) => x.name === p.worldType)
      if (e && e.element && e.element.height > 0) numSections = e.element.height / 16
    } catch (err) { console.error('dimension height lookup failed, assuming 24 sections: ' + err.message) }
    client.write('settings', {
      locale: 'en_US', viewDistance, chatFlags: 0, chatColors: true,
      skinParts: 0x7f, mainHand: 1, enableTextFiltering: false, enableServerListing: true
    })
  })
  client.on('position', (p) => { client.write('teleport_confirm', { teleportId: p.teleportId }) })

  client.on('map_chunk', (p) => {
    if (m.first_chunk_ms < 0) m.first_chunk_ms = Date.now() - t0
    try {
      const c = decodeChunk(p, numSections)
      chunks.set(`${c.x},${c.z}`, c)
    } catch (e) {
      chunkErrors++
      console.error(`chunk decode failed at ${p.x},${p.z}: ${e.message}`)
    }
    armIdle()
  })

  // overall safety net
  setTimeout(() => finish(4, 'timeout waiting for chunks'), 120000).unref()
}

if (process.argv[2] === '--selftest') selftest()
else main()

#!/usr/bin/env node
// ref_dump.cjs -- reference client for the M0 protocol prototype (MC 1.20.1, protocol 763; also 1.20.2 and 1.12.2
// through MC_NMP / MC_PROTOCOL, see tools/mcclient/run.sh).
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

const NMP_VERSION = process.env.MC_NMP || '1.20.1'
const PROTOCOL = parseInt(process.env.MC_PROTOCOL || '763', 10)
const LEGACY_SECTIONS = PROTOCOL <= 340 && PROTOCOL >= 107 // 1.9 - 1.12.2: CF_SECTIONS_19

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
  bytes (n) {
    if (this.p + n > this.b.length) throw new Error('byte run past end')
    const out = this.b.subarray(this.p, this.p + n)
    this.p += n
    return out
  }
  longsBig (n) {
    if (this.p + n * 8 > this.b.length) throw new Error('long array past end')
    const out = new Array(n)
    for (let i = 0; i < n; i++) { out[i] = this.b.readBigUInt64BE(this.p); this.p += 8 }
    return out
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

// 1.21.5+ (protocol 770): a paletted container has no data array length any more (derived from the bits per entry).
const NO_LEN = Number(process.env.MC_PROTOCOL || 0) >= 770

// One paletted container. kind 'blocks' (4096, indirect 4..8) or 'biomes' (64, indirect 1..3).
// Returns array of global ids.
function readContainer (r, kind) {
  const count = kind === 'blocks' ? 4096 : 64
  const maxIndirect = kind === 'blocks' ? 8 : 3
  const minIndirect = kind === 'blocks' ? 4 : 1
  let bits = r.u8()
  if (bits === 0) {
    const v = r.varint()
    const len = NO_LEN ? 0 : r.varint()
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
  const per = Math.floor(64 / bits)
  const nl = NO_LEN ? Math.ceil(count / per) : r.varint()
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


// ---------------------------------------------------------------- 1.9 - 1.12.2 (CF_SECTIONS_19)
// Section: bits per block (u8), palette length (varint; 0 = direct ids), palette entries (varint),
// data array length (varint, in longs), the longs (an entry MAY span two longs in this format),
// 2048 octets block light, 2048 octets sky light (only when the dimension has a sky).
// Block value on the wire = id << 4 | metadata. A ground-up column ends with 256 biome octets.
function unpackSpanning (longs, bits, count) {
  const out = new Array(count)
  const mask = (1n << BigInt(bits)) - 1n
  for (let i = 0; i < count; i++) {
    const bit = i * bits
    const w = Math.floor(bit / 64)
    const off = BigInt(bit % 64)
    if (w >= longs.length) throw new Error('packed array too short')
    let v = longs[w] >> off
    if (off + BigInt(bits) > 64n) {
      if (w + 1 >= longs.length) throw new Error('packed array too short')
      v |= longs[w + 1] << (64n - off)
    }
    out[i] = Number(v & mask)
  }
  return out
}

function decodeLegacyColumn (p, hasSky) {
  const r = new Reader(p.chunkData)
  const sections = []
  const f = new Fnv64()
  let skyN = 0; let blockN = 0
  for (let i = 0; i < 16; i++) {
    if (!(p.bitMap & (1 << i))) {
      sections.push({ blockCount: 0, blocksHash: hashU16(new Array(4096).fill(0)), distinct: 1, ids: null })
      continue
    }
    let bits = r.u8()
    const pn = r.varint()
    const palette = []
    for (let k = 0; k < pn; k++) palette.push(r.varint())
    const nl = r.varint()
    if (bits < 4) bits = 4
    if (nl !== Math.ceil(4096 * bits / 64)) throw new Error(`legacy long count ${nl} for ${bits} bits`)
    const ids = unpackSpanning(r.longsBig(nl), bits, 4096)
    if (pn > 0) {
      for (let k = 0; k < 4096; k++) {
        if (ids[k] >= pn) throw new Error('legacy palette index out of range')
        ids[k] = palette[ids[k]]
      }
    }
    for (const b of r.bytes(2048)) f.byte(b)
    blockN++
    if (hasSky) { for (const b of r.bytes(2048)) f.byte(b); skyN++ }
    let nonAir = 0
    for (const v of ids) if (v !== 0) nonAir++
    sections.push({ blockCount: nonAir, blocksHash: hashU16(ids), distinct: new Set(ids).size, ids })
  }
  let biomes = null
  if (p.groundUp) {
    biomes = Array.from(r.bytes(256))
  }
  for (let i = r.p; i < p.chunkData.length; i++) if (p.chunkData[i] !== 0) { trailingNonZero++; break }
  return {
    x: p.x,
    z: p.z,
    sections,
    legacy: true,
    biomesHash: biomes ? hashU16(biomes) : null,
    blockEntities: p.blockEntities.length,
    hm: [],
    light: { sky: skyN, block: blockN, h: f.hex() }
  }
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
  if (Array.isArray(root)) {
    // 1.21.5+: heightmaps = [{ type: 'motion_blocking', data: [[hi, lo], ...] }, ...] (protodef mapper names)
    for (const e of root) hm.push({ name: String(e.type).toUpperCase(), hash: hashU16(heightmapValues(e.data)) })
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
  let s = `mcdump 1\nprotocol ${PROTOCOL}\n`
  s += `chunks ${list.length}\n`
  for (const c of list) {
    s += `C ${c.x} ${c.z} sections=${c.sections.length} blockentities=${c.blockEntities}\n`
    if (c.legacy && c.biomesHash) s += `B ${c.x} ${c.z} ${c.biomesHash}\n`
    c.sections.forEach((sec, i) => {
      if (c.legacy) s += `S ${c.x} ${c.z} ${i} count=${sec.blockCount} distinct=${sec.distinct} blocks=${sec.blocksHash}\n`
      else s += `S ${c.x} ${c.z} ${i} count=${sec.blockCount} distinct=${sec.distinct} blocks=${sec.blocksHash} biomes=${sec.biomesHash}\n`
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
  // lpVec3 (1.21.9+ low precision vector) is unknown to node-minecraft-protocol 1.54: a reader written from the
  // public format description (u8 a; a == 0 -> zero vector; else u8 + u32 + varint when bit 2 of a is set)
  // is registered in its protodef compiler. Same type as tools/mcgen/gen_fixtures.cjs; the value is not used.
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

  const t0 = Date.now()
  const m = { connect_ms: -1, join_ms: -1, first_chunk_ms: -1 }
  let packets = 0; let bytesIn = 0; let bytesDec = 0; let walkOk = 0; let walkFail = 0
  const chunks = new Map()
  let idleTimer = null
  let finished = false
  let chunkErrors = 0
  let numSections = 24

  const client = mc.createClient({ host, port, username: name, version: NMP_VERSION, auth: 'offline', viewDistance: String(viewDistance), hideErrors: false })

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
      partial_chunks: partialChunks,
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

  let hasSky = true
  let partialChunks = 0
  client.on('login', (p) => {
    m.join_ms = Date.now() - t0
    if (LEGACY_SECTIONS) {
      hasSky = p.dimension === 0 // 1.12.2: -1 nether, 0 overworld, 1 end (only the overworld has a sky)
      client.write('settings', {
        locale: 'en_US', viewDistance, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1
      })
      return
    }
    if (p.dimensionCodec) {
      try {
        const nbt = require('prismarine-nbt')
        const codec = nbt.simplify(p.dimensionCodec)
        const list = codec['minecraft:dimension_type'].value
        const e = list.find((x) => x.name === p.worldType)
        if (e && e.element && e.element.height > 0) numSections = e.element.height / 16
      } catch (err) { console.error('dimension height lookup failed, assuming 24 sections: ' + err.message) }
    } // 1.20.2+: the registries arrive in the configuration phase; the vanilla overworld has 24 sections
    client.write('settings', {
      locale: 'en_US', viewDistance, chatFlags: 0, chatColors: true,
      skinParts: 0x7f, mainHand: 1, enableTextFiltering: false, enableServerListing: true
    })
  })
  client.on('chunk_batch_finished', () => { client.write('chunk_batch_received', { chunksPerTick: 10 }) })
  client.on('position', (p) => { client.write('teleport_confirm', { teleportId: p.teleportId }) })

  client.on('map_chunk', (p) => {
    if (m.first_chunk_ms < 0) m.first_chunk_ms = Date.now() - t0
    try {
      if (LEGACY_SECTIONS && !p.groundUp) { partialChunks++; armIdle(); return } // section-only update: ignored (documented)
      const c = LEGACY_SECTIONS ? decodeLegacyColumn(p, hasSky) : decodeChunk(p, numSections)
      chunks.set(`${c.x},${c.z}`, c)
    } catch (e) {
      chunkErrors++
      console.error(`chunk decode failed at ${p.x},${p.z}: ${e.message}`)
      if (process.env.MC_REF_DEBUG) console.error(`  debug chunkData(${p.chunkData.length}) head=${Buffer.from(p.chunkData).subarray(0, 96).toString('hex')}`)
    }
    armIdle()
  })

  // overall safety net
  setTimeout(() => finish(4, 'timeout waiting for chunks'), 120000).unref()
}

if (process.argv[2] === '--selftest') selftest()
else main()

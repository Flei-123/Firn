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

function teleportV21 (buf) {
  let o = 0
  const vi = () => { let r = 0; let sh = 0; let c; do { c = buf[o++]; r |= (c & 0x7f) << sh; sh += 7 } while (c & 0x80); return r }
  vi() // packet id
  const entityId = vi()
  const x = buf.readDoubleBE(o); const y = buf.readDoubleBE(o + 8); const z = buf.readDoubleBE(o + 16)
  o += 48 // position + delta movement
  const yaw = buf.readFloatBE(o); const pitch = buf.readFloatBE(o + 4)
  o += 8 + 4 // + relative flags
  return { entityId, x, y, z, yaw, pitch, onGround: buf[o] !== 0, v21: true }
}
const LEGACY_SECTIONS = PROTOCOL <= 340 && PROTOCOL >= 107 // 1.9 - 1.12.2: CF_SECTIONS_19
const LEGACY8 = PROTOCOL <= 47 // 1.8 - 1.8.9: CF_LEGACY_8 (map_chunk + map_chunk_bulk, unpaletted u16 LE blocks)
const SECTIONS_114 = PROTOCOL >= 751 && PROTOCOL <= 754 // 1.16.2 - 1.16.5: CF_SECTIONS_114

const fs = require('fs')
const crypto = require('crypto')
const { EntityTracker } = require('./ref_entities.cjs')
const RAW_NAMES = process.env.MC_RAW_NAMES ? new Set(process.env.MC_RAW_NAMES.split(',')) : null

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
const SEC_FLUID = Number(process.env.MC_PROTOCOL || 0) >= 775
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
    if (SEC_FLUID) r.i16() // 26.1 (775): non-empty fluid count follows the block count (found by raw bytes of the real server)
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

// ---------------------------------------------------------------- flattening (MC_FLATTEN=1, 1.12.2 only)
// The legacy table (id << 4 | meta -> 1.20.1 global state id) comes from the INDEPENDENT Node implementation
// tools/mc/flat/ref_flat.cjs (rules: tools/mcgen/legacy_flat.json), not from the Firn generator output.
let flatTable = null
function loadFlatTable () {
  const cp = require('child_process')
  const path = require('path')
  const root = path.join(__dirname, '..')
  const out = cp.execFileSync(process.execPath, [path.join(root, 'mc', 'flat', 'ref_flat.cjs'), path.join(root, 'mcgen', 'legacy_flat.json')],
    { encoding: 'utf8', env: process.env, maxBuffer: 1 << 24 })
  const t = out.split('\n').filter((l) => l.trim() !== '').map((l) => parseInt(l, 10))
  if (t.length !== 4096 || t.some((v) => !Number.isInteger(v))) throw new Error(`flat table has ${t.length} entries`)
  return t
}
if (process.env.MC_FLATTEN && PROTOCOL === 340) flatTable = loadFlatTable()

// A section update (groundUp = false) is MERGED into the stored column (the Firn client does the same with
// mc_world.world_apply): the sections named in bitMap replace the stored ones; biomes, heightmaps, block
// entities and the light hash stay those of the stored column; the light section counters add up.
// An update for a column that is not stored is dropped (counted in partial_chunks).
function mergeUpdate (stored, upd, bitMap) {
  for (let i = 0; i < 16; i++) {
    if (!(bitMap & (1 << i))) continue
    const old = stored.sections[i]
    const ns = upd.sections[i]
    if (old.biomesHash !== undefined && ns.biomesHash !== undefined) ns.biomesHash = old.biomesHash
    stored.sections[i] = ns
  }
  stored.light.sky += upd.light.sky
  stored.light.block += upd.light.block
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
    if (flatTable) { for (let k = 0; k < 4096; k++) ids[k] = ids[k] < 4096 ? flatTable[ids[k]] : 0 }
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

// ---------------------------------------------------------------- 1.16.2 - 1.16.5 (CF_SECTIONS_114)
// map_chunk: x, z, groundUp, bitMap (section i present), heightmaps (NBT), biomes (int[1024], ground-up only),
// chunkData, blockEntities. Per present section: blockCount (i16), bits per block (u8), palette (bits <= 8),
// packed longs that NEVER span two longs. A 1.16 column is 256 high = 16 sections; an absent one is all air.
// The biome array is ordered (y*4+z)*4+x over 64 cells of 4 blocks, so section i owns entries i*64 .. i*64+63.
// Light is a separate packet (update_light) and not part of the dump: sky=0 block=0, h = FNV offset basis.
function decodeChunk114 (p) {
  const r = new Reader(p.chunkData)
  const sections = []
  for (let i = 0; i < 16; i++) {
    let blockCount = 0
    let ids = new Array(4096).fill(0)
    if (p.bitMap & (1 << i)) {
      blockCount = r.i16()
      ids = readContainer(r, 'blocks')
    }
    const bio = p.groundUp ? p.biomes.slice(i * 64, i * 64 + 64) : new Array(64).fill(0)
    if (bio.length !== 64) throw new Error('biome array too short')
    sections.push({ blockCount, blocksHash: hashU16(ids), biomesHash: hashU16(bio), distinct: new Set(ids).size })
  }
  if (p.groundUp && p.biomes.length !== 1024) throw new Error(`biome count ${p.biomes.length}`)
  if (!r.eof()) trailingNonZero++ // 1.16 pads nothing: any leftover is reported
  const hm = []
  const entries = (p.heightmaps && p.heightmaps.value) || {}
  for (const name of Object.keys(entries)) {
    const e = entries[name]
    if (!e || e.type !== 'longArray') continue
    hm.push({ name, hash: hashU16(heightmapValues(e.value)) })
  }
  hm.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
  return {
    x: p.x,
    z: p.z,
    sections,
    blockEntities: p.blockEntities.length,
    hm,
    light: { sky: 0, block: 0, h: new Fnv64().hex() }
  }
}

// ---------------------------------------------------------------- 1.8 - 1.8.9 (CF_LEGACY_8)
// Column data (map_chunk chunkData, or one slice of map_chunk_bulk): for every present section 4096 blocks
// as u16 LITTLE endian (id << 4 | meta); then for every present section 2048 octets block light; then (only
// with sky light) for every present section 2048 octets sky light; a ground-up column ends with 256 biome
// octets. The light is hashed in wire order. Independent of the Firn decoder (own offsets, no shared code).
function legacy8Len (mask, hasSky, groundUp) {
  let n = 0
  for (let i = 0; i < 16; i++) if (mask & (1 << i)) n++
  return n * (8192 + 2048 + (hasSky ? 2048 : 0)) + (groundUp ? 256 : 0)
}

function decodeLegacy8Column (x, z, mask, groundUp, hasSky, data) {
  if (data.length !== legacy8Len(mask, hasSky, groundUp)) {
    throw new Error(`legacy8 data length ${data.length}, expected ${legacy8Len(mask, hasSky, groundUp)}`)
  }
  const present = []
  for (let i = 0; i < 16; i++) if (mask & (1 << i)) present.push(i)
  const n = present.length
  const f = new Fnv64()
  const lightOff = n * 8192
  for (let k = 0; k < n * 2048; k++) f.byte(data[lightOff + k])
  let skyN = 0
  if (hasSky) {
    for (let k = 0; k < n * 2048; k++) f.byte(data[lightOff + n * 2048 + k])
    skyN = n
  }
  const sections = []
  for (let i = 0; i < 16; i++) {
    const k = present.indexOf(i)
    if (k < 0) {
      sections.push({ blockCount: 0, blocksHash: hashU16(new Array(4096).fill(0)), distinct: 1, ids: null })
      continue
    }
    const ids = new Array(4096)
    for (let q = 0; q < 4096; q++) ids[q] = data.readUInt16LE(k * 8192 + q * 2)
    let nonAir = 0
    for (const v of ids) if (v !== 0) nonAir++
    sections.push({ blockCount: nonAir, blocksHash: hashU16(ids), distinct: new Set(ids).size, ids })
  }
  let biomes = null
  if (groundUp) {
    const off = n * (8192 + 2048 + (hasSky ? 2048 : 0))
    biomes = Array.from(data.subarray(off, off + 256))
  }
  return {
    x,
    z,
    sections,
    legacy: true,
    biomesHash: biomes ? hashU16(biomes) : null,
    blockEntities: 0,
    hm: [],
    light: { sky: skyN, block: n, h: f.hex() }
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
  let bulkPackets = 0
  let bulkColumns = 0 // 1.8: map_chunk_bulk packets / columns they carried
  let entityErrors = 0
  let numSections = 24
  const tracker = new EntityTracker(PROTOCOL)

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
    fs.writeFileSync(dumpFile + '.entities', tracker.dump()) // entity side file; the chunk dump stays as it was
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
      walk_fail: walkFail + chunkErrors + lightBad,
      num_sections: numSections,
      partial_chunks: partialChunks,
      merged_chunks: mergedChunks,
      update_light: { applied: lightApplied, pending: lightPending, orphan: lightOrphans, bad: lightBad },
      bulk_packets: bulkPackets,
      bulk_columns: bulkColumns,
      trailing_nonzero_chunks: trailingNonZero,
      entities: tracker.ents.size,
      entity_spawns: tracker.count.spawn,
      entity_errors: entityErrors,
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
    if (RAW_NAMES && RAW_NAMES.has(meta.name) && buffer) fs.appendFileSync(dumpFile + '.raw', `${meta.name} ${buffer.toString('hex')}\n`) // MC_RAW_NAMES=a,b: raw bodies for layout checks
    // minecraft-data lists the pre-1.21.2 entity_teleport layout for 1.21.2+: decode the real one from the raw body
    // (packet id varint, entity id varint, pos f64x3, delta f64x3, yaw f32, pitch f32, flags i32, on ground bool)
    if (meta.name === 'entity_teleport' && PROTOCOL >= 768 && buffer) data = teleportV21(buffer)
    try { tracker.onPacket(meta.name, data) } catch (e) { entityErrors++; console.error('entity packet ' + meta.name + ': ' + e.message) }
    bytesDec += (full || buffer || Buffer.alloc(0)).length
  })

  let hasSky = true
  let partialChunks = 0
  let mergedChunks = 0
  let lightOrphans = 0; let lightPending = 0; let lightApplied = 0; let lightBad = 0
  client.on('login', (p) => {
    m.join_ms = Date.now() - t0
    if (LEGACY_SECTIONS || LEGACY8) {
      hasSky = p.dimension === 0 // 1.12.2: -1 nether, 0 overworld, 1 end (only the overworld has a sky)
      client.write('settings', {
        locale: 'en_US', viewDistance, chatFlags: 0, chatColors: true, skinParts: 0x7f, mainHand: 1
      })
      return
    }
    if (p.dimensionCodec && !SECTIONS_114) {
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
  client.on('position', (p) => { if (!LEGACY8) client.write('teleport_confirm', { teleportId: p.teleportId }) }) // 1.8 has no teleport confirm

  client.on('map_chunk', (p) => {
    if (m.first_chunk_ms < 0) m.first_chunk_ms = Date.now() - t0
    if (LEGACY8) {
      try {
        if (p.groundUp && p.bitMap === 0 && p.chunkData.length === 0) { chunks.delete(`${p.x},${p.z}`); armIdle(); return } // 1.8 unload
        if (!p.groundUp) { partialChunks++; armIdle(); return } // section-only update: ignored (documented)
        const c = decodeLegacy8Column(p.x, p.z, p.bitMap, true, hasSky, p.chunkData)
        chunks.set(`${c.x},${c.z}`, c)
      } catch (e) {
        chunkErrors++
        console.error(`chunk decode failed at ${p.x},${p.z}: ${e.message}`)
      }
      armIdle()
      return
    }
    try {
      const c = LEGACY_SECTIONS ? decodeLegacyColumn(p, hasSky) : SECTIONS_114 ? decodeChunk114(p) : decodeChunk(p, numSections)
      if ((LEGACY_SECTIONS || SECTIONS_114) && !p.groundUp) {
        // section-only update: merged into the stored column (like mc_world.world_apply); unknown column = dropped
        const stored = chunks.get(`${c.x},${c.z}`)
        if (!stored) partialChunks++
        else { mergeUpdate(stored, c, p.bitMap); mergedChunks++ }
        armIdle()
        return
      }
      chunks.set(`${c.x},${c.z}`, c)
      if (SECTIONS_114 && process.env.MC_KEEP_LIGHT) attachPending(c)
    } catch (e) {
      chunkErrors++
      console.error(`chunk decode failed at ${p.x},${p.z}: ${e.message}`)
      if (process.env.MC_REF_DEBUG_FILE) fs.writeFileSync(process.env.MC_REF_DEBUG_FILE, Buffer.from(p.chunkData).toString('hex'))
      if (process.env.MC_REF_DEBUG) console.error(`  debug chunkData(${p.chunkData.length}) head=${Buffer.from(p.chunkData).subarray(0, 96).toString('hex')}`)
    }
    armIdle()
  })

  // 1.14 - 1.16.5: light arrives in its own (partial) update_light packets. MC_KEEP_LIGHT=1 merges them per column (an
  // array replaces the one of the same section bit, an "empty" bit drops it) and hashes sky arrays then block arrays in
  // ascending bit order. Own parser (restBuffer), independent of the Firn decoder. A packet for an unknown column is
  // counted as orphan (the Firn client drops it too).
  // Light for a column that is not stored yet (1.14 - 1.16.5 send update_light BEFORE the chunk) is kept per column and
  // attached when the column arrives (counted as pending, then applied). A chunk sent again starts without light.
  const pendingLight = new Map()
  const lightHash = (st) => {
    const f = new Fnv64()
    for (const map of [st.sky, st.block]) {
      for (const bit of [...map.keys()].sort((a, b) => a - b)) for (const b of map.get(bit)) f.byte(b)
    }
    return { sky: st.sky.size, block: st.block.size, h: f.hex() }
  }
  const attachPending = (c) => {
    const k = `${c.x},${c.z}`
    const st = pendingLight.get(k)
    if (!st) return
    pendingLight.delete(k)
    c.lightState = st
    c.light = lightHash(st)
    lightApplied++
  }
  client.on('update_light', (p) => {
    if (!SECTIONS_114 || !process.env.MC_KEEP_LIGHT) return
    const key = `${p.chunkX},${p.chunkZ}`
    const c = chunks.get(key)
    let st
    if (c) st = c.lightState || (c.lightState = { sky: new Map(), block: new Map() })
    else { st = pendingLight.get(key); if (!st) { st = { sky: new Map(), block: new Map() }; pendingLight.set(key, st) } }
    try {
      const r = new Reader(p.data)
      const u32 = (v) => v >>> 0
      const readSet = (mask, empty, map) => {
        for (let bit = 0; bit < 32; bit++) {
          if (u32(mask) & (2 ** bit)) {
            const n = r.varint()
            if (n !== 2048) throw new Error(`light array length ${n}`)
            map.set(bit, Buffer.from(r.bytes(2048)))
          }
        }
        for (let bit = 0; bit < 32; bit++) if (u32(empty) & (2 ** bit) && !(u32(mask) & (2 ** bit))) map.delete(bit)
      }
      // the arrays of one kind are consecutive: all sky first, then all block
      readSet(p.skyLightMask, p.emptySkyLightMask, st.sky)
      readSet(p.blockLightMask, p.emptyBlockLightMask, st.block)
      if (!r.eof()) throw new Error('update_light: trailing octets')
      if (c) { c.light = lightHash(st); lightApplied++ } else lightPending++
    } catch (e) { lightBad++; console.error(`update_light failed at ${p.chunkX},${p.chunkZ}: ${e.message}`) }
    armIdle()
  })

  client.on('map_chunk_bulk', (p) => {
    if (m.first_chunk_ms < 0) m.first_chunk_ms = Date.now() - t0
    bulkPackets++; bulkColumns += p.meta.length
    try {
      let off = 0
      for (const e of p.meta) {
        const len = legacy8Len(e.bitMap, p.skyLightSent, true)
        const c = decodeLegacy8Column(e.x, e.z, e.bitMap, true, p.skyLightSent, p.data.subarray(off, off + len))
        off += len
        chunks.set(`${c.x},${c.z}`, c)
      }
      if (off !== p.data.length) throw new Error(`bulk data ${p.data.length} octets, columns use ${off}`)
    } catch (e) {
      chunkErrors++
      console.error(`bulk decode failed: ${e.message}`)
    }
    armIdle()
  })

  // overall safety net
  setTimeout(() => finish(4, 'timeout waiting for chunks'), 120000).unref()
}

if (process.argv[2] === '--selftest') selftest()
else main()

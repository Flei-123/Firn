// tools/mc/packet/gen.cjs -- reference side of the mc_packet cross-check.
//
// Serialises random 1.20.1 packets with the protodef serializer of
// node-minecraft-protocol (minecraft-data protocol.json) and writes
//   <out>/packets.bin   records: [u8 tag][u32 BE length][packet bytes]
//   <out>/expect.txt    one canonical text line per record, built from the INPUT
//                       VALUES (not from the bytes), so the Firn reader has to
//                       reach the same values independently.
// Usage: NODE_PATH=<mcserver node_modules> node gen.cjs <out-dir> [count] [seed]
'use strict'
const fs = require('fs')
const path = require('path')
const { createSerializer, createDeserializer } = require('minecraft-protocol/src/transforms/serializer')
const nbt = require('prismarine-nbt')
const mcdata = require('minecraft-data')('1.20.1')

const out = process.argv[2]
const count = parseInt(process.argv[3] || '600', 10)
let seed = parseInt(process.argv[4] || '20261011', 10) >>> 0
function rnd () { // mulberry32
  seed = (seed + 0x6D2B79F5) >>> 0
  let t = seed
  t = Math.imul(t ^ (t >>> 15), t | 1)
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296
}
const ri = (n) => Math.floor(rnd() * n)
const pick = (a) => a[ri(a.length)]

const ser = { toClient: createSerializer({ state: 'play', isServer: true, version: '1.20.1' }),
  toServer: createSerializer({ state: 'play', isServer: false, version: '1.20.1' }) }
const des = { toClient: createDeserializer({ state: 'play', isServer: false, version: '1.20.1', noErrorLogging: true }),
  toServer: createDeserializer({ state: 'play', isServer: true, version: '1.20.1', noErrorLogging: true }) }

function pktId (dir, name) {
  const m = mcdata.protocol.play[dir].types.packet[1][0].type[1].mappings
  for (const k of Object.keys(m)) if (m[k] === name) return parseInt(k, 16)
  throw new Error('no packet ' + name)
}

const ALPHA = ['a', 'b', 'Z', '0', ' ', '_', 'ä', 'ß', 'é', 'я', '中', '€', '😀', '𝄞', '\u0000', '§', '"']
function text (maxChars) {
  const n = ri(maxChars + 1)
  let s = ''
  for (let i = 0; i < n; i++) s += pick(ALPHA)
  return s
}
const hex = (b) => Buffer.from(b).toString('hex')
const hexs = (s) => hex(Buffer.from(s, 'utf8'))
function bitsF64 (v) { const b = Buffer.alloc(8); b.writeDoubleBE(v); return b.readBigUInt64BE().toString(16).padStart(16, '0') }
function bitsF32 (v) { const b = Buffer.alloc(4); b.writeFloatBE(v); return b.readUInt32BE().toString(16).padStart(8, '0') }
const f64 = () => (ri(2000000) - 1000000) / 16
const f32 = () => (ri(200000) - 100000) / 8
function big64 () { return (BigInt(ri(4294967296)) << 32n | BigInt(ri(4294967296))) - (1n << 63n) }
function varint () { return ri(4) === 0 ? ri(4294967296) - 2147483648 : ri(3000) }
function uuid () { let s = ''; for (let i = 0; i < 32; i++) s += '0123456789abcdef'[ri(16)]; return s }

function randNbt (depth) {
  const v = {}
  const n = ri(5)
  for (let i = 0; i < n; i++) {
    const name = 'k' + i + text(3).replace(/\u0000/g, '')
    const t = ri(depth > 0 ? 9 : 7)
    if (t === 0) v[name] = nbt.byte(ri(256) - 128)
    else if (t === 1) v[name] = nbt.short(ri(65536) - 32768)
    else if (t === 2) v[name] = nbt.int(ri(4294967296) - 2147483648)
    else if (t === 3) v[name] = nbt.long([ri(4294967296) - 2147483648, ri(4294967296) - 2147483648])
    else if (t === 4) v[name] = nbt.string(text(12).replace(/\u0000/g, ''))
    else if (t === 5) v[name] = nbt.intArray(Array.from({ length: ri(6) }, () => ri(100000) - 50000))
    else if (t === 6) v[name] = nbt.list(nbt.int(Array.from({ length: ri(5) }, () => ri(1000))))
    else if (t === 7) v[name] = nbt.comp(randNbt(depth - 1).value)
    else v[name] = nbt.list(nbt.comp(Array.from({ length: ri(3) }, () => randNbt(depth - 1).value)))
  }
  return nbt.comp(v, '')
}

function slot () {
  if (ri(3) === 0) return { value: { present: false }, line: '-' }
  const itemId = ri(1200)
  const itemCount = ri(64) + 1
  if (ri(2) === 0) {
    return { value: { present: true, itemId, itemCount, nbtData: undefined }, line: `${itemId}:${itemCount}:` }
  }
  const tag = randNbt(2)
  const raw = nbt.writeUncompressed(tag, 'big')
  return { value: { present: true, itemId, itemCount, nbtData: tag }, line: `${itemId}:${itemCount}:${hex(raw)}` }
}

const gens = {
  1: () => { const c = text(ri(20) === 0 ? 5000 : 60); const a = ri(2) === 1
    return ['toClient', 'system_chat', { content: c, isActionBar: a }, `content=${hexs(c)} action=${a ? 1 : 0}`] },
  2: () => { const p = { x: f64(), y: f64(), z: f64(), yaw: f32(), pitch: f32(), flags: ri(256) - 128, teleportId: varint() }
    return ['toClient', 'position', p, `x=${bitsF64(p.x)} y=${bitsF64(p.y)} z=${bitsF64(p.z)} yaw=${bitsF32(p.yaw)} pitch=${bitsF32(p.pitch)} flags=${p.flags} tp=${p.teleportId}`] },
  3: () => { const l = { x: ri(67108864) - 33554432, y: ri(4096) - 2048, z: ri(67108864) - 33554432 }; const t = ri(30000)
    return ['toClient', 'block_change', { location: l, type: t }, `pos=${l.x},${l.y},${l.z} type=${t}`] },
  4: () => { const p = { entityId: varint(), playerUUID: '', x: f64(), y: f64(), z: f64(), yaw: ri(256) - 128, pitch: ri(256) - 128 }
    const u = uuid(); p.playerUUID = `${u.slice(0, 8)}-${u.slice(8, 12)}-${u.slice(12, 16)}-${u.slice(16, 20)}-${u.slice(20)}`
    return ['toClient', 'named_entity_spawn', p, `eid=${p.entityId} uuid=${u} x=${bitsF64(p.x)} y=${bitsF64(p.y)} z=${bitsF64(p.z)} yaw=${p.yaw} pitch=${p.pitch}`] },
  5: () => { const a = big64(); const t = big64()
    return ['toClient', 'update_time', { age: a, time: t }, `age=${a} time=${t}`] },
  6: () => { const ch = pick(['minecraft:brand', 'a:b/c', 'fml:handshake', 'x']); const d = Buffer.from(Array.from({ length: ri(40) }, () => ri(256)))
    return ['toClient', 'custom_payload', { channel: ch, data: d }, `channel=${hexs(ch)} data=${hex(d)}`] },
  7: () => { const p = { locale: pick(['en_us', 'de_de', '', 'ru_ru']), viewDistance: ri(32), chatFlags: ri(3), chatColors: ri(2) === 1, skinParts: ri(256), mainHand: ri(2), enableTextFiltering: ri(2) === 1, enableServerListing: ri(2) === 1 }
    return ['toServer', 'settings', p, `locale=${hexs(p.locale)} vd=${p.viewDistance} cf=${p.chatFlags} cc=${p.chatColors ? 1 : 0} sp=${p.skinParts} mh=${p.mainHand} tf=${p.enableTextFiltering ? 1 : 0} sl=${p.enableServerListing ? 1 : 0}`] },
  8: () => { const items = Array.from({ length: ri(8) }, slot); const carried = slot(); const wid = ri(256); const sid = varint()
    return ['toClient', 'window_items', { windowId: wid, stateId: sid, items: items.map(s => s.value), carriedItem: carried.value },
      `win=${wid} state=${sid} n=${items.length} items=${items.map(s => s.line).join(',')} carried=${carried.line}`] },
  9: () => { const m = Array.from({ length: ri(6) }, () => { const t = ri(2) === 0 ? undefined : text(10); return { match: text(8), tooltip: t } })
    const p = { transactionId: varint(), start: ri(100), length: ri(100), matches: m }
    return ['toClient', 'tab_complete', p, `tx=${p.transactionId} start=${p.start} len=${p.length} n=${m.length} m=${m.map(x => hexs(x.match) + '/' + (x.tooltip === undefined ? '-' : hexs(x.tooltip))).join(',')}`] },
  10: () => { const sig = ri(2) === 0 ? undefined : Buffer.from(Array.from({ length: 256 }, () => ri(256))); const ack = Buffer.from([ri(256), ri(256), ri(256)])
    const p = { message: text(40), timestamp: big64(), salt: big64(), signature: sig, offset: ri(20), acknowledged: ack }
    return ['toServer', 'chat_message', p, `msg=${hexs(p.message)} ts=${p.timestamp} salt=${p.salt} sig=${sig ? hex(sig) : '-'} off=${p.offset} ack=${hex(ack)}`] },
  11: () => { const cc = { x: ri(4194304) - 2097152, z: ri(4194304) - 2097152, y: ri(1048576) - 524288 }; const rec = Array.from({ length: ri(6) }, () => ri(2000000000))
    return ['toClient', 'multi_block_change', { chunkCoordinates: cc, records: rec }, `cc=${cc.x},${cc.y},${cc.z} n=${rec.length} rec=${rec.join(',')}`] },
  12: () => { const e = Array.from({ length: ri(5) }, () => text(12)); const a = ri(3)
    return ['toClient', 'chat_suggestions', { action: a, entries: e }, `action=${a} n=${e.length} e=${e.map(hexs).join(',')}`] }
}

const recs = []
const lines = []
for (let i = 0; i < count; i++) {
  const tag = 1 + (i % 12)
  const [dir, name, params, line] = gens[tag]()
  const buf = ser[dir].createPacketBuffer({ name, params })
  // sanity: node can parse its own bytes and re-serialise to the same bytes
  const parsed = des[dir].parsePacketBuffer(buf).data
  const again = ser[dir].createPacketBuffer({ name: parsed.name, params: parsed.params })
  if (!again.equals(buf)) throw new Error('node roundtrip mismatch for ' + name)
  const hdr = Buffer.alloc(5)
  hdr[0] = tag
  hdr.writeUInt32BE(buf.length, 1)
  recs.push(hdr, buf)
  lines.push(`${tag} ${name} id=${pktId(dir, name)} ${line}`)
}
fs.mkdirSync(out, { recursive: true })
fs.writeFileSync(path.join(out, 'packets.bin'), Buffer.concat(recs))
fs.writeFileSync(path.join(out, 'expect.txt'), lines.join('\n') + '\n')
console.log(`gen: ${count} packets, ${Buffer.concat(recs).length} bytes`)

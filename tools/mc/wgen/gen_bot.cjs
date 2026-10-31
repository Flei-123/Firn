'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const mcData = require(path.join(root, 'minecraft-data'))('1.20.1')

const [host, portArg, seedArg, expectArg] = process.argv.slice(2)
const expect = parseInt(expectArg, 10)
const CODES = { stone: 'S', air: 'A', water: 'W', lava: 'L', copper_ore: 'C', raw_copper_block: 'c', granite: 'G', deepslate_iron_ore: 'I', raw_iron_block: 'i', tuff: 'T' }

function varint(buf, o) {
  let r = 0, s = 0, b
  do { b = buf[o.p++]; r |= (b & 127) << s; s += 7 } while (b & 128)
  return r
}

function container(buf, o, maxIndirect) {
  const bits = buf[o.p++]
  let pal = null, single = 0
  if (bits === 0) single = varint(buf, o)
  else if (bits <= maxIndirect) {
    const n = varint(buf, o)
    pal = []
    for (let i = 0; i < n; i++) pal.push(varint(buf, o))
  }
  const n = varint(buf, o)
  const longs = []
  for (let i = 0; i < n; i++) { longs.push(buf.readBigUInt64BE(o.p)); o.p += 8 }
  return { bits, pal, single, longs }
}

function entry(c, idx) {
  if (c.bits === 0) return c.single
  const per = Math.floor(64 / c.bits)
  const l = c.longs[Math.floor(idx / per)]
  const v = Number((l >> BigInt((idx % per) * c.bits)) & ((1n << BigInt(c.bits)) - 1n))
  return c.pal ? c.pal[v] : v
}

function rows(data, cx, cz) {
  const buf = data.chunkData
  const o = { p: 0 }
  const out = []
  for (let sec = 0; sec < 24; sec++) {
    o.p += 2
    const blocks = container(buf, o, 8)
    container(buf, o, 3)
    for (let ly = 0; ly < 16; ly++) {
      let row = ''
      for (let i = 0; i < 256; i++) {
        const b = mcData.blocksByStateId[entry(blocks, ly * 256 + i)]
        row += (b && CODES[b.name]) || '?'
      }
      out.push(`${seedArg} ${cx} ${cz} ${-64 + sec * 16 + ly} ${row}`)
    }
  }
  return out.reverse()
}

const client = mc.createClient({ host, port: parseInt(portArg, 10), username: 'GenBot', version: '1.20.1', auth: 'offline' })
const t0 = Date.now()
let count = 0
let origin = null
client.on('map_chunk', (p) => {
  count++
  if (p.x === 0 && p.z === 0) origin = rows(p, 0, 0)
  if (count >= expect) finish(true)
})
client.on('error', (e) => { console.error('error', String(e)); process.exit(1) })
client.on('kick_disconnect', (p) => { console.error('kick', JSON.stringify(p)); process.exit(1) })
setTimeout(() => finish(false), parseInt(process.env.GEN_BOT_TIMEOUT_MS || '120000', 10))

function finish(ok) {
  console.error(`chunks=${count} ms=${Date.now() - t0}`)
  if (origin) console.log(origin.join('\n'))
  client.end('bye')
  setTimeout(() => process.exit(ok ? 0 : 2), 200)
}

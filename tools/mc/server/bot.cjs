'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))

const [host, portArg, scenario, nameArg] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const name = nameArg || 'FirnBot'
const VERSION = '1.20.1'

function varint(buf, o) {
  let r = 0, s = 0, b
  do { b = buf[o.p++]; r |= (b & 127) << s; s += 7 } while (b & 128)
  return r
}

function container(buf, o, maxIndirect, directBits) {
  const bits = buf[o.p++]
  let pal = null, single = 0
  if (bits === 0) {
    single = varint(buf, o)
  } else if (bits <= maxIndirect) {
    const n = varint(buf, o)
    pal = []
    for (let i = 0; i < n; i++) pal.push(varint(buf, o))
  }
  const n = varint(buf, o)
  const longs = []
  for (let i = 0; i < n; i++) { longs.push(buf.readBigUInt64BE(o.p)); o.p += 8 }
  return { bits, pal, single, longs, direct: bits > maxIndirect ? directBits : 0 }
}

function entry(c, idx, cells) {
  if (c.bits === 0) return c.single
  const bits = c.direct || c.bits
  const per = Math.floor(64 / c.bits)
  const w = idx % per
  const l = c.longs[Math.floor(idx / per)]
  const v = Number((l >> BigInt(w * c.bits)) & ((1n << BigInt(c.bits)) - 1n))
  return c.pal ? c.pal[v] : v
}

function parseColumn(data) {
  const buf = data.chunkData
  const o = { p: 0 }
  const hist = {}
  const column = []
  let sections = 0
  while (o.p < buf.length && sections < 24) {
    const count = buf.readInt16BE(o.p); o.p += 2
    const blocks = container(buf, o, 8, 15)
    container(buf, o, 3, 6)
    for (let i = 0; i < 4096; i++) {
      const id = entry(blocks, i, 4096)
      if (id !== 0) hist[id] = (hist[id] || 0) + 1
      if ((i & 15) === 0 && ((i >> 4) & 15) === 0) column.push(id)
    }
    sections++
  }
  let pad = 0
  while (o.p + pad < buf.length && buf[o.p + pad] === 0) pad++
  return { sections, consumed: o.p + pad === buf.length, pad, hist }
}

function out(obj) { console.log(JSON.stringify(obj)) }

if (scenario === 'status') {
  const t0 = Date.now()
  mc.ping({ host, port, version: VERSION }, (err, r) => {
    if (err) { out({ ok: false, error: String(err.message) }); process.exit(1) }
    out({ ok: true, protocol: r.version.protocol, max: r.players.max, online: r.players.online, ping_ms: Date.now() - t0 })
    process.exit(0)
  })
} else {
  const opts = { host, port, username: name, auth: 'offline', version: VERSION, hideErrors: true }
  if (scenario === 'silent') opts.keepAlive = false
  const client = mc.createClient(opts)
  const res = { order: [], keepalive: [], times: [], chunks: 0, columns: {}, hist: null, max_pad: 0, parse_ok: true, sections_ok: true,
    center: [], system_chat: [], kicked: null, closed: false, position: null, errors: [], hists: {}, bad_columns: [] }
  const t0 = Date.now()
  let moved = false
  let chatSent = false
  const seen = new Set()
  client.on('error', (e) => res.errors.push(String(e.message)))
  client.on('end', (r) => { res.closed = true; res.end_reason = String(r) })
  client.on('kick_disconnect', (p) => { res.kicked = p.reason })
  client.on('disconnect', (p) => { res.kicked = p.reason })
  let finished = false
  client.on('packet', (data, meta) => {
    if (meta.state !== 'play' || finished) return
    const now = Date.now() - t0
    if (!seen.has(meta.name) && res.order.length < 40) { seen.add(meta.name); res.order.push(meta.name) }
    if (meta.name === 'position') {
      res.position = { x: data.x, y: data.y, z: data.z, teleportId: data.teleportId }
      client.write('teleport_confirm', { teleportId: data.teleportId })
      client.write('position_look', { x: data.x, y: data.y, z: data.z, yaw: 0, pitch: 0, onGround: true })
    } else if (meta.name === 'keep_alive') {
      res.keepalive.push(now)
    } else if (meta.name === 'update_time') {
      res.times.push({ age: Number(data.age), wall: now })
    } else if (meta.name === 'update_view_position') {
      res.center.push([data.chunkX, data.chunkZ])
    } else if (meta.name === 'map_chunk') {
      res.chunks++
      const key = data.x + ',' + data.z
      if (!res.columns[key]) {
        res.columns[key] = true
        try {
          const c = parseColumn(data)
          if (c.pad > res.max_pad) res.max_pad = c.pad
          if (c.sections !== 24 || !c.consumed) { res.sections_ok = false; res.bad_columns.push({ key, sections: c.sections, consumed: c.consumed, len: data.chunkData.length }) }
          if (process.env.BOT_HISTS) res.hists[key] = c.hist
          if (data.x === 0 && data.z === 0) res.hist = c.hist
        } catch (e) { res.parse_ok = false; res.errors.push('parse ' + e.message) }
      }
    } else if (meta.name === 'system_chat' || meta.name === 'player_chat' || meta.name === 'profileless_chat') {
      res.system_chat.push(meta.name + ':' + JSON.stringify(data).slice(0, 300))
    }
  })
  const finish = (code) => { finished = true; if (!process.env.BOT_HISTS) delete res.hists; res.column_count = Object.keys(res.columns).length; delete res.columns; out(res); try { client.end('bye') } catch (e) {} ; setTimeout(() => process.exit(code), 100) }
  if (scenario === 'join') {
    const total = parseInt(process.env.BOT_SECONDS || '18', 10) * 1000
    setTimeout(() => {
      if (res.position && !moved) {
        moved = true
        client.write('position_look', { x: res.position.x + 160, y: res.position.y, z: res.position.z, yaw: 90, pitch: 0, onGround: true })
      }
    }, 4000)
    setTimeout(() => {
      if (!chatSent) {
        chatSent = true
        if (client.chat) client.chat('hello firn')
        else client.write('chat_message', { message: 'hello firn', timestamp: BigInt(Date.now()), salt: 0n, offset: 0, acknowledged: Buffer.alloc(3) })
      }
    }, 6000)
    setTimeout(() => finish(0), total)
  } else if (scenario === 'silent') {
    setTimeout(() => finish(0), parseInt(process.env.BOT_SECONDS || '55', 10) * 1000)
  } else if (scenario === 'login_only') {
    client.on('login', () => setTimeout(() => finish(0), 1500))
    setTimeout(() => finish(1), 15000)
  }
}

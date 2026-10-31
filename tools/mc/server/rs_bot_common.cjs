'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const data = require(path.join(root, 'minecraft-data'))('1.20.1')

function describe(id) {
  const b = data.blocksByStateId[id]
  if (!b) return `state${id}`
  let rest = id - b.minStateId
  const parts = []
  const states = b.states || []
  const radix = states.map((s) => s.num_values)
  for (let i = states.length - 1; i >= 0; i--) {
    const v = rest % radix[i]
    rest = Math.floor(rest / radix[i])
    const s = states[i]
    const val = s.type === 'bool' ? (v === 0 ? 'true' : 'false') : s.values ? s.values[v] : String(v)
    parts.unshift(`${s.name}=${val}`)
  }
  return parts.length ? `${b.name}[${parts.join(',')}]` : b.name
}

function startBot(scenario, opts) {
  const gap = (opts && opts.gap) || 1800
  const [host, portArg] = process.argv.slice(2)
  const port = parseInt(portArg, 10)
  const res = { pos: null, events: [], marks: [], errors: [], kicked: null }
  let seq = 0
  let done = false
  let t0 = 0
  const client = mc.createClient({ host, port, username: 'RsBot', auth: 'offline', version: '1.20.1', hideErrors: true })
  client.on('error', (e) => res.errors.push(String(e.message)))
  client.on('kick_disconnect', (p) => { res.kicked = p.reason })

  client.on('packet', (d, meta) => {
    if (meta.state !== 'play' || done) return
    if (meta.name === 'position') {
      if (!res.pos) res.pos = { x: d.x, y: d.y, z: d.z }
      client.write('teleport_confirm', { teleportId: d.teleportId })
      client.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
      if (!res.started) { res.started = true; setTimeout(run, 2500) }
    } else if (meta.name === 'block_change' && res.base) {
      res.events.push({ t: Date.now() - t0, x: d.location.x - res.base.x, y: d.location.y - res.base.y, z: d.location.z - res.base.z, state: describe(d.type) })
    } else if (meta.name === 'multi_block_change' && res.base) {
      const c = d.chunkCoordinates
      for (const r of d.records) {
        const v = BigInt(r)
        const pos = Number(v & 0xfffn)
        const state = Number(v >> 12n)
        res.events.push({ t: Date.now() - t0, x: c.x * 16 + ((pos >> 8) & 15) - res.base.x, y: c.y * 16 + (pos & 15) - res.base.y, z: c.z * 16 + ((pos >> 4) & 15) - res.base.z, state: describe(state) })
      }
    }
  })

  function place(x, y, z, face) {
    const b = res.base
    client.write('block_place', { hand: 0, location: { x: b.x + x, y: b.y + y, z: b.z + z }, direction: face, cursorX: 0.5, cursorY: 0.5, cursorZ: 0.5, insideBlock: false, sequence: ++seq })
  }
  const held = (slot) => client.write('held_item_slot', { slotId: slot })
  const give = (slot, n) => client.write('set_creative_slot', { slot, item: { present: true, itemId: data.itemsByName[n].id, itemCount: 1, nbtData: undefined } })
  const look = (yaw, pitch) => client.write('position_look', { x: res.pos.x, y: res.pos.y, z: res.pos.z, yaw, pitch: pitch || 0, onGround: true })

  function run() {
    res.base = { x: Math.floor(res.pos.x), y: Math.floor(res.pos.y), z: Math.floor(res.pos.z) }
    t0 = Date.now()
    const slots = {}
    const steps = []
    const actions = []
    const api = {
      put(n, x, y, z, face) {
        if (!(n in slots)) { slots[n] = Object.keys(slots).length; give(36 + slots[n], n) }
        steps.push(() => held(slots[n]))
        steps.push(() => place(x, y, z, face))
      },
      look(yaw, pitch) { steps.push(() => look(yaw, pitch)) },
      act(markName, n, x, y, z) {
        if (!(n in slots)) { slots[n] = Object.keys(slots).length; give(36 + slots[n], n) }
        actions.push(() => { res.marks.push({ name: markName, t: Date.now() - t0 }); held(slots[n]); place(x, y, z, 1) })
      },
    }
    scenario(api)
    let at = 300
    steps.forEach((f) => { setTimeout(f, at); at += 250 })
    at += 1000
    actions.forEach((f) => { setTimeout(f, at); at += gap })
    setTimeout(finish, at + 500)
  }

  function finish() {
    if (done) return
    done = true
    console.log(JSON.stringify(res))
    try { client.end('bye') } catch (e) {}
    setTimeout(() => process.exit(0), 300)
  }
  setTimeout(finish, 90000)
}

module.exports = { startBot }

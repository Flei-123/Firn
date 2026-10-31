'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))

const [host, portArg] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const res = { dig: { eid: null, pos: null, slots: [], collects: [], items: [] }, obs: { eid: null, named: [], spawns: [], metadata: [], moves: [], collects: [], destroys: [], infos: 0 }, target: null, errors: [] }

function connect(name) {
  const c = mc.createClient({ host, port, username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
  c.on('error', (e) => res.errors.push(name + ': ' + String(e.message)))
  return c
}

function track(c, who) {
  c.on('packet', (d, meta) => {
    if (meta.state !== 'play') return
    if (meta.name === 'login') res[who].eid = d.entityId
    if (meta.name === 'position') {
      if (!res[who].pos) res[who].pos = { x: d.x, y: d.y, z: d.z }
      c.write('teleport_confirm', { teleportId: d.teleportId })
      c.write('position_look', { x: d.x, y: d.y, z: d.z, yaw: 0, pitch: 0, onGround: true })
    }
  })
}

const dig = connect('DigBot')
const obs = connect('ObsBot')
track(dig, 'dig')
track(obs, 'obs')
const t0 = Date.now()
const stamp = () => Date.now() - t0

obs.on('packet', (d, meta) => {
  if (meta.state !== 'play') return
  const o = res.obs
  if (meta.name === 'named_entity_spawn') o.named.push({ eid: d.entityId, x: d.x, y: d.y, z: d.z })
  else if (meta.name === 'player_info') o.infos++
  else if (meta.name === 'spawn_entity') o.spawns.push({ eid: d.entityId, type: d.type, x: d.x, y: d.y, z: d.z, objectData: d.objectData, t: stamp() })
  else if (meta.name === 'entity_metadata') {
    const it = (d.metadata || []).find((m) => m.key === 8)
    o.metadata.push({ eid: d.entityId, item: it && it.value && it.value.present ? { id: it.value.itemId, n: it.value.itemCount } : null, t: stamp() })
  } else if (meta.name === 'rel_entity_move' || meta.name === 'entity_move_look') o.moves.push({ eid: d.entityId, dx: d.dX / 4096, dy: d.dY / 4096, dz: d.dZ / 4096, t: stamp() })
  else if (meta.name === 'entity_teleport') o.moves.push({ eid: d.entityId, tx: d.x, ty: d.y, tz: d.z, t: stamp() })
  else if (meta.name === 'collect') o.collects.push({ item: d.collectedEntityId, by: d.collectorEntityId, n: d.pickupItemCount, t: stamp() })
  else if (meta.name === 'entity_destroy') o.destroys.push({ ids: d.entityIds, t: stamp() })
})

dig.on('packet', (d, meta) => {
  if (meta.state !== 'play') return
  const g = res.dig
  if (meta.name === 'set_slot') g.slots.push({ w: d.windowId, slot: d.slot, id: d.item.present ? d.item.itemId : null, n: d.item.present ? d.item.itemCount : 0 })
  else if (meta.name === 'collect') g.collects.push({ item: d.collectedEntityId, by: d.collectorEntityId, n: d.pickupItemCount })
  else if (meta.name === 'spawn_entity') g.items.push({ eid: d.entityId, x: d.x, y: d.y, z: d.z })
  else if (meta.name === 'entity_move_look' || meta.name === 'rel_entity_move') {
    const it = g.items.find((i) => i.eid === d.entityId)
    if (it) { it.x += d.dX / 4096; it.y += d.dY / 4096; it.z += d.dZ / 4096 }
  } else if (meta.name === 'entity_teleport') {
    const it = g.items.find((i) => i.eid === d.entityId)
    if (it) { it.x = d.x; it.y = d.y; it.z = d.z }
  }
})

function finish() {
  console.log(JSON.stringify(res))
  try { dig.end() } catch (e) {}
  try { obs.end() } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}

function waitFor(cond, ms, next) {
  const s = Date.now()
  const iv = setInterval(() => {
    if (cond() || Date.now() - s > ms) { clearInterval(iv); next() }
  }, 100)
}

waitFor(() => res.obs.pos && res.dig.pos, 20000, () => {
  setTimeout(() => {
    const p = res.dig.pos
    const bx = Math.floor(p.x) + 1
    const by = Math.floor(p.y) - 1
    const bz = Math.floor(p.z)
    res.target = { x: bx, y: by, z: bz }
    dig.write('block_dig', { status: 0, location: { x: bx, y: by, z: bz }, face: 1, sequence: 1 })
    setTimeout(() => {
      dig.write('block_dig', { status: 2, location: { x: bx, y: by, z: bz }, face: 1, sequence: 2 })
      setTimeout(() => {
        dig.write('position_look', { x: bx + 0.5, y: by, z: bz + 0.5, yaw: 0, pitch: 0, onGround: true })
        setTimeout(finish, 2500)
      }, 2500)
    }, 1600)
  }, 2500)
})
setTimeout(finish, 40000)

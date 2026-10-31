'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const data = require(path.join(root, 'minecraft-data'))('1.20.1')
const fs = require('fs')
const [file, wantItem] = process.argv.slice(2)
const r = JSON.parse(fs.readFileSync(file, 'utf8'))
const itemId = data.itemsByName[wantItem || 'dirt'].id
const typeId = data.entitiesByName.item.id
let bad = 0
const ok = (c, m) => { console.log((c ? '  ok    ' : '  FAIL  ') + m); if (!c) bad++ }
const o = r.obs
const sp = o.spawns[0]
ok(r.errors.length === 0, 'no bot errors ' + r.errors.join(';'))
ok(o.infos >= 1 && o.named.some((n) => n.eid === r.dig.eid), 'observer sees the other player (player_info + named_entity_spawn)')
ok(o.spawns.length === 1, 'exactly one item spawn seen by observer (got ' + o.spawns.length + ')')
if (sp) {
  ok(sp.type === typeId, 'spawned entity is an item (type ' + sp.type + ')')
  const t = r.target
  ok(Math.abs(sp.x - (t.x + 0.5)) <= 0.3 && Math.abs(sp.z - (t.z + 0.5)) <= 0.3, 'spawn x/z inside the broken block')
  ok(sp.y >= t.y + 0.1 && sp.y <= t.y + 0.65, 'spawn y inside the broken block (' + sp.y.toFixed(3) + ')')
  const md = o.metadata.find((m) => m.eid === sp.eid)
  ok(md && md.item && md.item.id === itemId && md.item.n === 1, 'item metadata ' + wantItem + ' x1 ' + JSON.stringify(md && md.item))
  ok(md && o.spawns[0].t <= md.t, 'metadata after spawn')
  let y = sp.y, x = sp.x, z = sp.z
  for (const m of o.moves.filter((m) => m.eid === sp.eid)) { if (m.tx !== undefined) { x = m.tx; y = m.ty; z = m.tz } else { x += m.dx; y += m.dy; z += m.dz } }
  ok(Math.abs(y - t.y) < 0.06, 'item rests on the block floor y=' + t.y + ' (got ' + y.toFixed(3) + ')')
  ok(Math.abs(x - (t.x + 0.5)) < 0.8 && Math.abs(z - (t.z + 0.5)) < 0.8, 'item rests within one block of the break')
  const col = o.collects.find((c) => c.item === sp.eid)
  ok(col && col.by === r.dig.eid && col.n === 1, 'observer sees collect by the digger ' + JSON.stringify(col))
  const de = o.destroys.find((d) => d.ids.includes(sp.eid))
  ok(de && col && de.t >= col.t, 'item destroyed after collect')
}
const g = r.dig
ok(g.collects.length >= 1 && g.collects[0].n === 1, 'digger receives collect')
ok(g.slots.some((s) => s.slot === 36 && s.id === itemId && s.n === 1), 'digger gets set_slot 36 = ' + wantItem + ' x1')
process.exit(bad ? 1 : 0)

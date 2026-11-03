'use strict'
const fs = require('fs')
const [vf, ff] = process.argv.slice(2)
const IGNORE = new Set((process.env.ORDER_IGNORE || 'update_time,time,entity_metadata,entity_velocity,entity_teleport,rel_entity_move,entity_move_look,entity_look,entity_head_rotation,entity_update_attributes,update_attributes,entity_equipment,player_info,player_remove,named_entity_spawn,spawn_entity,entity_destroy,declare_recipes,tags,declare_commands,advancements,unlock_recipes,sound_effect,named_sound_effect,block_change,multi_block_change,acknowledge_player_digging,block_break_animation,world_particles,statistics,update_light,tile_entity_data,map_chunk,chunk_biomes,unload_chunk,system_chat,chunk_batch_start,chunk_batch_finished,boss_bar,bundle_delimiter,experience,set_ticking_state,step_tick,post_effects,set_time,level_chunk_with_light,light_update,forget_level_chunk').split(','))
function norm(packets, loose) {
  const res = []
  for (let p of packets) {
    if (loose) p = p.replace(/^(position|update_view_position) .*/, '$1')
    const name = p.split(' ')[0]
    if (IGNORE.has(name)) continue
    res.push(p.replace(/"entityId":\d+/, '"entityId":0').replace(/^spawn_position .*/, 'spawn_position'))
  }
  const merged = []
  for (const p of res) {
    const name = p.split(' ')[0]
    const last = merged[merged.length - 1]
    if (last && last.name === name && (name === 'set_slot' || name === 'position' || name === 'window_items')) { last.n++; continue }
    merged.push({ name, text: p, n: 1 })
  }
  return merged.map((m) => (m.name === 'set_slot' || m.name === 'window_items' ? 'inventory' : m.text))
}
function chunkRuns(packets) {
  const names = packets.map((p) => p.split(' ')[0])
  return names.filter((n) => n === 'map_chunk').length
}
const v = JSON.parse(fs.readFileSync(vf, 'utf8'))
const f = JSON.parse(fs.readFileSync(ff, 'utf8'))
let bad = 0
if (v.errors.length || f.errors.length) { console.log('errors', JSON.stringify(v.errors), JSON.stringify(f.errors)); bad++ }
const n = Math.max(v.marks.length, f.marks.length)
for (let i = 0; i < n; i++) {
  const vm = v.marks[i], fm = f.marks[i]
  if (vm && !/^(execute in|raw:enter|raw:leave|raw:end)/.test(vm.cmd)) continue
  if (!vm || !fm) { console.log('mark count differs'); bad++; break }
  const loose = vm.cmd.startsWith('raw:') && !vm.cmd.startsWith('raw:end')
  const a = norm(vm.packets, loose), b = norm(fm.packets, loose)
  const same = JSON.stringify(a) === JSON.stringify(b)
  console.log(`${same ? '  ok   ' : '  DIFF '} ${vm.cmd}  (vanilla ${a.length}, firn ${b.length}, chunks ${chunkRuns(vm.packets)}/${chunkRuns(fm.packets)})`)
  if (!same || process.env.ORDER_VERBOSE) {
    const max = Math.max(a.length, b.length)
    for (let k = 0; k < max; k++) {
      const x = a[k] === undefined ? '-' : a[k], y = b[k] === undefined ? '-' : b[k]
      console.log(`    ${x === y ? '=' : '!'} ${x.slice(0, 150)}${x === y ? '' : '   |   ' + y.slice(0, 150)}`)
    }
  }
  if (!same) bad++
}
process.exit(bad ? 1 : 0)

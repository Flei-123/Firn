'use strict'
const fs = require('fs')

function same(a, b) { return JSON.stringify(a) === JSON.stringify(b) }
function pos(order, name) { return order.indexOf(name) }

function checkJoin(label, r, ref) {
  const lines = []
  const reference = label === 'vanilla' || label === 'firn-no-opt'
  const add = (ok, msg, soft) => lines.push((ok ? '  ok    ' : (reference && soft ? '  info  ' : '  FAIL  ')) + label + ': ' + msg)
  const o = r.order
  add(pos(o, 'login') === 0, 'login is the first play packet (' + o.slice(0, 4).join(',') + ')')
  add(pos(o, 'position') > pos(o, 'login') && pos(o, 'map_chunk') > pos(o, 'position'), 'order login < position < map_chunk')
  add(pos(o, 'update_view_position') !== -1 && pos(o, 'update_view_position') < pos(o, 'map_chunk'), 'view position sent before the first chunk')
  add(r.parse_ok && r.sections_ok, 'every column parses: 24 sections, data fully consumed', true)
  add(r.column_count >= 289, 'at least 289 distinct columns streamed (' + r.column_count + ')')
  add(r.hist && ref.hist && same(r.hist, ref.hist), 'block histogram of column 0,0 equals vanilla ' + JSON.stringify(r.hist))
  const t = r.times
  if (t.length >= 2) {
    const dt = t[t.length - 1].wall - t[0].wall
    const da = t[t.length - 1].age - t[0].age
    const tps = da * 1000 / dt
    add(da >= 200 && tps > 18.0 && tps < 21.0, 'TPS over ' + da + ' ticks = ' + tps.toFixed(2), true)
  } else add(false, 'no update_time packets')
  const ka = r.keepalive[0]
  add(ka !== undefined && ka > 14500 && ka < 16800, 'first keep alive after ' + ka + ' ms (15 s)')
  const px = r.position ? r.position.x : 0
  const lastc = r.center[r.center.length - 1]
  add(lastc && lastc[0] === Math.floor((px + 160) / 16), 'view center follows the move: ' + JSON.stringify(lastc), true)
  add(r.system_chat.some((m) => m.indexOf('hello firn') !== -1), 'chat message echoed back')
  add(!r.kicked && r.errors.length === 0, 'no kick, no errors ' + JSON.stringify(r.errors.slice(0, 2)))
  return lines
}

function checkSilent(label, r) {
  const closed = r.closed
  const reason = JSON.stringify(r.kicked || r.end_reason || '')
  return ['  ' + (closed && /imed|timeout/i.test(reason) ? 'ok    ' : 'FAIL  ') + label + ': silent client dropped by keep alive timeout ' + reason.slice(0, 80)]
}

function total(h) { return Object.values(h).reduce((a, b) => a + b, 0) }

function checkWorld(label, f, v) {
  const lines = []
  const add = (ok, msg) => lines.push((ok ? '  ok    ' : '  FAIL  ') + label + ': ' + msg)
  const common = Object.keys(f.hists).filter((k) => v.hists[k] && total(f.hists[k]) > 0)
  const same = common.filter((k) => same_h(f.hists[k], v.hists[k]))
  const drift = common.filter((k) => !same.includes(k) && Math.abs(total(f.hists[k]) - total(v.hists[k])) > Math.max(8, total(v.hists[k]) * 0.005))
  add(f.parse_ok && f.sections_ok && f.max_pad === 0, 'every column parses, data fully consumed, no padding (pad ' + f.max_pad + ')')
  add(v.parse_ok && v.sections_ok, 'vanilla columns parse (padding up to ' + v.max_pad + ' bytes tolerated)')
  add(common.length >= 100, common.length + ' saved columns present in both')
  add(same.length >= common.length * 0.9, same.length + ' of ' + common.length + ' column histograms equal vanilla (rest differ by fluid/vegetation ticks)')
  add(drift.length === 0, 'no column differs by more than 0.5% of its blocks (' + drift.slice(0, 3).join(' ') + ')')
  add(f.position && Math.abs(f.position.x - 0.5) < 1e9 && f.position.y > 0, 'spawn from level.dat ' + JSON.stringify(f.position))
  return lines
}

function checkExtras(label, f, v) {
  const lines = []
  const add = (ok, msg) => lines.push((ok ? '  ok    ' : '  FAIL  ') + label + ': ' + msg)
  const common = Object.keys(f.extras).filter((k) => v.extras[k] && f.hists[k] && v.hists[k] && total(f.hists[k]) > 0 && total(v.hists[k]) > 0)
  const eq = (k, get) => JSON.stringify(get(f.extras[k])) === JSON.stringify(get(v.extras[k]))
  const bad = (get) => common.filter((k) => !eq(k, get))
  const kinds = [['biome cells', (x) => x.biomes], ['light masks', (x) => x.masks], ['sky light arrays', (x) => x.sky], ['block light arrays', (x) => x.blk], ['block entity list', (x) => x.bes]]
  add(common.length >= 100, common.length + ' saved columns with extras present in both')
  for (const [name, get] of kinds) {
    const b = bad(get)
    add(b.length === 0, name + ' equal to vanilla in ' + (common.length - b.length) + '/' + common.length + ' columns' + (b.length ? ' (first ' + b.slice(0, 3).join(' ') + ')' : ''))
  }
  const withBe = common.filter((k) => v.extras[k].bes.length > 0).length
  const withBiomes = common.filter((k) => new Set(v.extras[k].biomes).size > 1).length
  add(withBe >= 10, withBe + ' columns carry block entities')
  add(withBiomes >= 10, withBiomes + ' columns carry more than one biome')
  return lines
}

function same_h(a, b) { return JSON.stringify(Object.entries(a).sort()) === JSON.stringify(Object.entries(b).sort()) }

if (process.argv[2] === 'extras' || process.argv[2] === 'extras-mutate') {
  const f = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'))
  const v = JSON.parse(fs.readFileSync(process.argv[4], 'utf8'))
  if (process.argv[2] === 'extras-mutate') {
    for (const k of Object.keys(f.extras)) {
      const b = f.extras[k].biomes
      f.extras[k].biomes = (b.charAt(0) === '0' ? '1' : '0') + b.slice(1)
      f.extras[k].bes.push('0|0|0|0|null')
    }
    const lines = checkExtras('counter-check', f, v)
    process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 0 : 1)
  }
  const lines = checkExtras(process.argv[5], f, v)
  console.log(lines.join('\n'))
  process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 1 : 0)
}

if (process.argv[2] === 'world' || process.argv[2] === 'world-mutate') {
  const f = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'))
  const v = JSON.parse(fs.readFileSync(process.argv[4], 'utf8'))
  if (process.argv[2] === 'world-mutate') {
    for (const k of Object.keys(f.hists)) {
      if (!v.hists[k]) continue
      const id = Object.keys(f.hists[k])[0]
      f.hists[k][id] += 100000
    }
    const lines = checkWorld('counter-check', f, v)
    process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 0 : 1)
  }
  const lines = checkWorld(process.argv[5], f, v)
  console.log(lines.join('\n'))
  process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 1 : 0)
}

if (process.argv[2] === 'join') {
  const r = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'))
  const ref = JSON.parse(fs.readFileSync(process.argv[4], 'utf8'))
  const lines = checkJoin(process.argv[5], r, ref)
  console.log(lines.join('\n'))
  process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 1 : 0)
} else if (process.argv[2] === 'silent') {
  const r = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'))
  const lines = checkSilent(process.argv[4], r)
  console.log(lines.join('\n'))
  process.exit(lines.some((l) => l.indexOf('FAIL') !== -1) ? 1 : 0)
} else if (process.argv[2] === 'mutate') {
  const r = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'))
  const ref = JSON.parse(fs.readFileSync(process.argv[4], 'utf8'))
  const k = Object.keys(ref.hist)[0]
  ref.hist[k] = ref.hist[k] + 1
  const lines = checkJoin('counter-check', r, ref)
  process.exit(lines.some((l) => l.indexOf('FAIL') !== -1 && l.indexOf('histogram') !== -1) ? 0 : 1)
}

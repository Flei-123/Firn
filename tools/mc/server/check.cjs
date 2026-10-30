'use strict'
const fs = require('fs')

function same(a, b) { return JSON.stringify(a) === JSON.stringify(b) }
function pos(order, name) { return order.indexOf(name) }

function checkJoin(label, r, ref) {
  const lines = []
  const reference = label === 'vanilla'
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
    add(da >= 200 && tps > 18.0 && tps < 21.0, 'TPS over ' + da + ' ticks = ' + tps.toFixed(2))
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

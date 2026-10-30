'use strict'
const fs = require('fs')
const [kind, aPath, bPath] = process.argv.slice(2)
const a = JSON.parse(fs.readFileSync(aPath, 'utf8'))
const fail = (m) => { console.log('  FAIL  ' + m); process.exit(1) }
if (kind === 'firn') {
  if (a.errors.length) fail('bot errors ' + a.errors.join(';'))
  if (a.expect.length !== 7) fail('no expectations')
  if (a.acks.length < 9) fail('acks ' + a.acks.length + ' < 9')
  const stone = a.changes.filter((c) => c.x === a.expect[3].x && c.z === a.expect[3].z && c.y === a.expect[3].y)
  if (stone.length === 0) fail('no block_change for the placed stone')
  console.log('  ok    firn edits: ' + a.changes.length + ' block_change, ' + a.acks.length + ' acks')
} else if (kind === 'player') {
  const b = JSON.parse(fs.readFileSync(bPath, 'utf8'))
  const f = b.final_pos
  const dx = Math.abs(a.pos.x - f.x), dy = Math.abs(a.pos.y - f.y), dz = Math.abs(a.pos.z - f.z)
  if (dx > 0.01 || dy > 0.01 || dz > 0.01) fail('player position differs: saved ' + JSON.stringify(f) + ' vs vanilla rejoin ' + JSON.stringify(a.pos))
  console.log('  ok    vanilla rejoin restores the Firn-saved position ' + JSON.stringify(a.pos))
  const w = a.window
  if (!w[36] || !w[37]) fail('hotbar items missing after vanilla rejoin: ' + JSON.stringify(w))
  console.log('  ok    vanilla rejoin restores the hotbar: ' + JSON.stringify(w))
}

'use strict'
const fs = require('fs')
const { lastState, ticks, compareRuns } = require('./rs_check_common.cjs')

const [aFile, bFile, wrong] = process.argv.slice(2)
const load = (f) => JSON.parse(fs.readFileSync(f, 'utf8'))
const a = load(aFile)
const b = bFile && bFile !== '-' ? load(bFile) : null
let bad = 0
const fail = (m) => { console.log('  FAIL  ' + m); bad++ }
const ok = (m) => console.log('  ok    ' + m)

if (a.errors.length || a.kicked) fail('errors ' + JSON.stringify(a.errors) + ' kicked ' + a.kicked)
const lamps = { A: '4,0,2', B: '3,0,4', C: '2,1,-2' }
const expectLit = { A0: true, A1: false, B0: true, B1: false, C0: false, C1: true }
const expectedFinal = { A: 'redstone_lamp[lit=false]', B: 'redstone_lamp[lit=false]', C: 'redstone_lamp[lit=true]' }
if (wrong) expectedFinal.A = 'redstone_lamp[lit=true]'
for (const k of Object.keys(lamps)) {
  const key = lamps[k]
  const got = lastState(a, key)
  if (got === expectedFinal[k]) ok(`lamp ${k} final ${got}`)
  else fail(`lamp ${k} final ${got} expected ${expectedFinal[k]}`)
}
const t = ticks(a)
for (const ph of Object.keys(expectLit)) {
  const key = lamps[ph[0]]
  const evs = (t[ph] || []).filter((e) => e.key === key)
  const want = `redstone_lamp[lit=${expectLit[ph]}]`
  const hit = evs.find((e) => e.state === want)
  if (hit) ok(`phase ${ph}: lamp -> ${want} after ${hit.dt} ticks`)
  else fail(`phase ${ph}: lamp never became ${want}`)
}

if (b) compareRuns(a, b, fail)
process.exit(bad ? 1 : 0)

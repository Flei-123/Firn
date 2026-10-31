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
const lamps = { stone: '3,0,3', wood: '3,0,-3', observer: '-3,0,3', comparator: '-2,0,-3' }
const lit = (on) => `redstone_lamp[lit=${on}]`
const finals = { stone: lit(false), wood: lit(false), observer: lit(false), comparator: lit(wrong ? false : true) }
for (const k of Object.keys(lamps)) {
  const got = lastState(a, lamps[k])
  if (got === finals[k]) ok(`${k} lamp final ${got}`)
  else fail(`${k} lamp final ${got} expected ${finals[k]}`)
}
const t = ticks(a)
const phases = {
  B0: ['stone', [true, false]],
  B1: ['wood', [true, false]],
  O0: ['observer', [true, false]],
  O1: ['observer', [true, false]],
  C0: ['comparator', [true]],
  C2: ['comparator', [false]],
  C3: ['comparator', [true]],
}
for (const ph of Object.keys(phases)) {
  const [lamp, seq] = phases[ph]
  const evs = (t[ph] || []).filter((e) => e.key === lamps[lamp])
  const got = evs.map((e) => e.state)
  const want = seq.map(lit)
  if (JSON.stringify(got) === JSON.stringify(want)) ok(`phase ${ph}: ${lamp} lamp ${got.join(' -> ')} (dt ${evs.map((e) => e.dt).join(',')})`)
  else fail(`phase ${ph}: ${lamp} lamp events ${JSON.stringify(got)} expected ${JSON.stringify(want)}`)
}
if (b) compareRuns(a, b, fail)
process.exit(bad ? 1 : 0)

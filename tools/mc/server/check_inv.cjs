'use strict'
const fs = require('fs')
const [refPath, gotPath, label] = process.argv.slice(2)
const ref = JSON.parse(fs.readFileSync(refPath, 'utf8'))
const got = JSON.parse(fs.readFileSync(gotPath, 'utf8'))
const dump = (r) => r.slots.map((s, i) => (s ? i + ':' + s : null)).filter(Boolean).join(' ') + ' | cursor ' + r.cursor
let fail = 0
let total = 0
for (const mode of Object.keys(ref.modes)) {
  const g = got.modes[mode] || []
  ref.modes[mode].forEach((r, i) => {
    total++
    const x = g[i]
    const same = x && x.name === r.name && JSON.stringify(x.slots) === JSON.stringify(r.slots) && x.cursor === r.cursor && JSON.stringify(x.start) === JSON.stringify(r.start)
    if (!same) {
      fail++
      console.log('  FAIL  ' + mode + '/' + r.name)
      console.log('        vanilla: ' + dump(r))
      console.log('        ' + label + ': ' + (x ? dump(x) : 'missing'))
    }
  })
}
if (ref.errors.length) { console.log('  FAIL  reference errors: ' + ref.errors.join('; ')); fail++ }
if (got.errors.length) { console.log('  FAIL  ' + label + ' errors: ' + got.errors.join('; ')); fail++ }
if (process.argv[5] === 'expect-diff') {
  if (fail === 0) { console.log('  FAIL  negative control: mutated reference not detected'); process.exit(1) }
  process.exit(0)
}
if (fail === 0) console.log('  ok    ' + label + ': ' + total + ' click scenarios identical to vanilla')
process.exit(fail === 0 ? 0 : 1)

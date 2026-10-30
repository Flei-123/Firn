const fs = require('fs')
const [mode, firnPath, vanPath] = process.argv.slice(2)
const load = (p) => JSON.parse(fs.readFileSync(p, 'utf8'))
const van = load(vanPath)
let fir = load(firnPath)
const NUM_TOLERANCE = 250
const norm = (s, name) => s.split(name).join('NAME')
const round = (v) => (typeof v === 'number' ? Math.round(v * 1e4) / 1e4 : v)
const numbers = (s) => (s.match(/-?\d+/g) || []).map(Number)
const shape = (s) => s.replace(/\d+/g, '#')

function compareEvent(f, v, fname, vname) {
  const problems = []
  const timeCmd = /^time (query|add|set)/.test(v.cmd)
  const fc = f.chat.map((s) => norm(s, fname))
  const vc = v.chat.map((s) => norm(s, vname))
  if (fc.length !== vc.length) problems.push(`chat count firn=${fc.length} vanilla=${vc.length}: ${JSON.stringify(fc)} vs ${JSON.stringify(vc)}`)
  else {
    for (let i = 0; i < fc.length; i++) {
      if (timeCmd && /^The time is|^Set the time/.test(vc[i])) {
        const a = numbers(fc[i])
        const b = numbers(vc[i])
        const close = a.length === b.length && a.every((x, k) => Math.abs(x - b[k]) <= NUM_TOLERANCE)
        if (shape(fc[i]) !== shape(vc[i]) || !close) problems.push(`chat[${i}] firn='${fc[i]}' vanilla='${vc[i]}'`)
      } else if (fc[i] !== vc[i]) problems.push(`chat[${i}] firn='${fc[i]}' vanilla='${vc[i]}'`)
    }
  }
  const fp = JSON.stringify(f.pos.map((p) => p.map(round)))
  const vp = JSON.stringify(v.pos.map((p) => p.map(round)))
  if (fp !== vp) problems.push(`position firn=${fp} vanilla=${vp}`)
  if (JSON.stringify(f.modes) !== JSON.stringify(v.modes)) problems.push(`game_state firn=${JSON.stringify(f.modes)} vanilla=${JSON.stringify(v.modes)}`)
  return problems
}

function timeSeen(events, i, expected) {
  const times = events[i].times.concat(events[i + 1] ? events[i + 1].times : [])
  const circular = (x) => Math.abs((((x - expected) % 24000) + 36000) % 24000 - 12000)
  return times.some((t) => circular(t) <= NUM_TOLERANCE)
}

function compareTimes(fir, van, i) {
  const message = van.events[i].chat[0] || ''
  const expected = /^Set the time/.test(message) ? numbers(message)[0] : undefined
  if (expected === undefined) return []
  const problems = []
  if (!timeSeen(van.events, i, expected)) problems.push(`vanilla sent no update_time near ${expected}`)
  if (!timeSeen(fir.events, i, expected)) problems.push(`firn sent no update_time near ${expected}`)
  return problems
}

function mutate(doc) {
  const copy = JSON.parse(JSON.stringify(doc))
  const e = copy.events.find((x) => x.chat.length > 0)
  e.chat[0] = e.chat[0] + ' X'
  return copy
}

if (mode === 'mutate') fir = mutate(fir)
let bad = 0
for (let i = 0; i < van.events.length; i++) {
  const problems = compareEvent(fir.events[i], van.events[i], fir.name, van.name)
  if (/^time (set|add)/.test(van.events[i].cmd)) problems.push(...compareTimes(fir, van, i))
  if (problems.length) {
    bad++
    if (mode !== 'mutate') console.log(`  FAIL  cmd '${van.events[i].cmd}': ${problems.join('; ')}`)
  }
}
if (mode === 'mutate') process.exit(bad > 0 ? 0 : 1)
const total = van.events.length
if (bad === 0) console.log(`  ok    commands: ${total} commands, chat text, position, game_state and update_time equal to vanilla`)
else console.log(`  FAIL  commands: ${bad}/${total} differ from vanilla`)
process.exit(bad === 0 ? 0 : 1)

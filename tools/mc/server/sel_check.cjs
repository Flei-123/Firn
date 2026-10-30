const fs = require('fs')
const [mode, firnDir, vanDir] = process.argv.slice(2)
const load = (d, n) => JSON.parse(fs.readFileSync(`${d}/sel_${n}.json`, 'utf8'))
const SUBSET = new Set(['teleport', 'tp', 'gamemode', 'time', 'say'])
const round = (v) => (typeof v === 'number' ? Math.round(v * 1e4) / 1e4 : v)
const bots = ['OpBot', 'NoBot']

function tabView(t, vanilla) {
  if (!t) return null
  let matches = t.matches
  if (vanilla && t.start === 1) matches = matches.filter((m) => SUBSET.has(m))
  return { start: t.start, length: t.length, matches }
}

function compareBot(f, v) {
  const problems = []
  f.events.forEach((fe, i) => {
    const ve = v.events[i]
    const p = []
    if (JSON.stringify(fe.chat) !== JSON.stringify(ve.chat)) p.push(`chat firn=${JSON.stringify(fe.chat)} vanilla=${JSON.stringify(ve.chat)}`)
    const fp = JSON.stringify(fe.pos.map((x) => x.map(round)))
    const vp = JSON.stringify(ve.pos.map((x) => x.map(round)))
    if (fp !== vp) p.push(`position firn=${fp} vanilla=${vp}`)
    if (JSON.stringify(fe.modes) !== JSON.stringify(ve.modes)) p.push(`game_state firn=${JSON.stringify(fe.modes)} vanilla=${JSON.stringify(ve.modes)}`)
    if (ve.kind === 'tab') {
      const ft = JSON.stringify(tabView(fe.tab, false))
      const vt = JSON.stringify(tabView(ve.tab, true))
      if (ft !== vt) p.push(`tab firn=${ft} vanilla=${vt}`)
    }
    if (p.length) problems.push(`${v.name} '${ve.who}|${ve.kind}|${ve.text}': ${p.join('; ')}`)
  })
  return problems
}

function mutate(doc) {
  const copy = JSON.parse(JSON.stringify(doc))
  const e = copy.events.find((x) => x.kind === 'tab' && x.tab && x.tab.matches.length > 0)
  e.tab.matches.push('zz_mutated')
  return copy
}

let problems = []
for (const b of bots) {
  let f = load(firnDir, b)
  const v = load(vanDir, b)
  if (mode === 'mutate' && b === 'OpBot') f = mutate(f)
  problems = problems.concat(compareBot(f, v))
}
if (mode === 'mutate') process.exit(problems.length > 0 ? 0 : 1)
const total = load(vanDir, 'OpBot').events.length * bots.length
problems.forEach((p) => console.log('  FAIL  ' + p))
if (problems.length === 0) console.log(`  ok    selectors+tab: ${total} bot slots (chat, position, game_state, tab_complete) equal to vanilla`)
else console.log(`  FAIL  selectors+tab: ${problems.length}/${total} slots differ from vanilla`)
process.exit(problems.length === 0 ? 0 : 1)

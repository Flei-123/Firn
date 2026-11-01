'use strict'

const ok = (m) => console.log('  ok    ' + m)

function perPos(r) {
  const m = {}
  const lastT = {}
  for (const e of r.events) {
    if (e.y < 0) continue
    const key = `${e.x},${e.y},${e.z}`
    const l = (m[key] ||= [])
    if (lastT[key] !== undefined && e.t - lastT[key] < 20 && l.length) l[l.length - 1] = e.state
    else if (l[l.length - 1] !== e.state) l.push(e.state)
    lastT[key] = e.t
  }
  for (const k of Object.keys(m)) m[k] = m[k].filter((s, i, a) => i === 0 || s !== a[i - 1])
  for (const k of Object.keys(m)) if (m[k].every((s) => s === 'air')) delete m[k]
  return m
}
function lastState(r, key) { const l = perPos(r)[key]; return l ? l[l.length - 1] : null }
function phaseOf(r, t) {
  let name = null
  for (const m of r.marks) if (m.t <= t) name = m
  return name
}
function ticks(r) {
  const out = {}
  const seen = {}
  for (const e of r.events) {
    const m = phaseOf(r, e.t)
    if (!m || e.y < 0 || e.state.startsWith('redstone_wire') || e.state === 'air') continue
    const key = `${e.x},${e.y},${e.z}`
    if (seen[key] === e.state) continue
    seen[key] = e.state
    ;(out[`${m.name}`] ||= []).push({ key: `${e.x},${e.y},${e.z}`, state: e.state, dt: Math.round((e.t - m.t) / 50) })
  }
  return out
}


function compareRuns(a, b, fail) {
  const pa = perPos(a)
  const pb = perPos(b)
  const keys = new Set([...Object.keys(pa), ...Object.keys(pb)])
  let seqDiff = 0
  for (const k of keys) {
    const x = JSON.stringify(pa[k] || [])
    const y = JSON.stringify(pb[k] || [])
    if (x !== y) { seqDiff++; fail(`state sequence at ${k}\n      A: ${x}\n      B: ${y}`) }
  }
  if (!seqDiff) ok(`state sequences identical at ${keys.size} positions`)
  const ta = ticks(a)
  const tb = ticks(b)
  let tDiff = 0
  for (const ph of Object.keys(ta)) {
    const la = ta[ph]
    const lb = tb[ph] || []
    for (const e of lb) {
      const m = la.find((f) => f.key === e.key && f.state === e.state)
      if (!m || Math.abs(m.dt - e.dt) > 1) { tDiff++; fail(`timing ${ph} ${e.key} ${e.state}: A ${m ? m.dt : 'missing'} B ${e.dt}`) }
    }
  }
  if (!tDiff) ok('tick timing within 1 tick')
}

module.exports = { perPos, lastState, ticks, compareRuns }

// tools/mc/flat/ref_flat.cjs -- independent reference of the flattening table (Node, no Firn, no Python).
//
// Reads tools/mcgen/legacy_flat.json (the rules) and minecraft-data through its npm API and prints
// the 4096 modern state ids of the legacy states `id << 4 | meta`, one per line.
// It follows the text in the header of tools/mcgen/gen_flat.py but is written differently on purpose:
// the modern state is FOUND by decoding every state of the target block (mixed radix, last
// property fastest) and matching the wanted property values, not computed with strides.
//
// usage: NODE_PATH=<...>/node_modules node ref_flat.cjs <rules.json>
'use strict'
const fs = require('fs')
const mcdata = require('minecraft-data')

const rules = JSON.parse(fs.readFileSync(process.argv[2], 'utf8')).rules
const oldv = mcdata('1.12.2')
const newv = mcdata('1.20.1')

function propValues(p) {
  if (p.type === 'bool') return ['true', 'false']
  return p.values.map(String)
}

// all (stateId, {prop: valueText}) of one modern block
function enumerate(b) {
  const props = b.states || []
  const out = []
  const total = b.maxStateId - b.minStateId + 1
  for (let n = 0; n < total; n++) {
    let rest = n
    const obj = {}
    for (let i = props.length - 1; i >= 0; i--) {
      const vals = propValues(props[i])
      obj[props[i].name] = vals[rest % vals.length]
      rest = Math.floor(rest / vals.length)
    }
    out.push([b.minStateId + n, obj])
  }
  return out
}

const cache = new Map()
function find(name, wanted) {
  // wanted: property -> text; absent properties take the default state's value. null = illegal value
  const b = newv.blocksByName[name]
  if (!b) throw new Error('no modern block ' + name)
  if (!cache.has(name)) cache.set(name, enumerate(b))
  const all = cache.get(name)
  const def = all.find(([id]) => id === b.defaultState)[1]
  const want = Object.assign({}, def)
  for (const [k, v] of Object.entries(wanted)) {
    if (!(k in def)) throw new Error(`block ${name} has no property ${k}`)
    const legal = propValues(b.states.find((s) => s.name === k))
    if (!legal.includes(v)) return null
    want[k] = v
  }
  for (const [id, obj] of all) {
    if (Object.keys(want).every((k) => obj[k] === want[k])) return id
  }
  throw new Error('no state found for ' + name)
}

const text = (v) => (v === true ? 'true' : v === false ? 'false' : String(v))

// returns the state id, or null for an unknown combination (strict) / never null (lenient)
function conv(rule, name, meta, lenient) {
  if (!rule) return find(name, {})
  let target
  if (rule.to !== undefined) target = rule.to
  else if (rule.by_meta) target = rule.by_meta[meta]
  else if (rule.names) {
    const i = (meta >> (rule.shift || 0)) & rule.mask
    if (i >= rule.names.length) {
      if (!lenient) return null
      target = rule.names[0]
    } else target = rule.names[i]
  } else target = name
  const want = {}
  for (const [k, v] of Object.entries(rule.set || {})) want[k] = text(v)
  for (const [k, p] of Object.entries(rule.props || {})) {
    const bits = (meta >> (p.shift || 0)) & p.mask
    let v
    if (p.map) {
      if (bits >= p.map.length) {
        if (!lenient) return null
        continue
      }
      v = p.map[bits]
      if (v === null) continue
    } else v = bits + (p.offset || 0)
    want[k] = text(v)
  }
  if (rule.to === undefined) {
    const have = new Set((newv.blocksByName[target].states || []).map((s) => s.name))
    for (const k of Object.keys(want)) if (!have.has(k)) delete want[k]
  }
  let id = find(target, want)
  if (id === null) {
    if (!lenient) return null
    const ok = {}
    for (const [k, v] of Object.entries(want)) if (find(target, { [k]: v }) !== null) ok[k] = v
    id = find(target, ok)
  }
  return id
}

const table = new Array(4096).fill(0)
for (const b of oldv.blocksArray) {
  const rule = rules[b.name]
  const base = conv(rule, b.name, 0, true)
  for (let meta = 0; meta < 16; meta++) {
    const id = conv(rule, b.name, meta, false)
    table[(b.id << 4) | meta] = id === null ? base : id
  }
}
process.stdout.write(table.join('\n') + '\n')

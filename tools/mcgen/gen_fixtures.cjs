#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// gen_fixtures.cjs -- example packet bodies from the REFERENCE serializer.
//
// usage: NODE_PATH=<tools/mcserver/node_modules> node gen_fixtures.cjs <outdir> [version=1.20.1] [per-packet=4]
//
// For EVERY packet of EVERY state and direction this builds example values from
// the protocol.json type tree (deterministic seed, boundary values mixed with
// random ones, every switch case reachable), serializes them with the
// node-minecraft-protocol / protodef serializer, parses the result back with the
// reference deserializer (a fixture the reference cannot parse exactly is
// dropped and reported, so a bad fixture can never blame the Firn walker) and
// writes the packet BODY (without the packet id varint) to <outdir>/<n>.bin.
//
// <outdir>/manifest.txt, one line per fixture:
//   <state 0..3> <dir 0=C2S 1=S2C> <id> <name> <file> <flags>
// flags: "-" or "rest" (the packet ends in a restBuffer: prefix/extension
// checks do not apply, any tail is a valid tail).
'use strict'
const fs = require('fs')
const path = require('path')
const mcData = require('minecraft-data')
const nbt = require('prismarine-nbt')
const { createSerializer, createDeserializer } = require('minecraft-protocol/src/transforms/serializer')

const outDir = process.argv[2]
const version = process.argv[3] || '1.20.1'
const perPacket = parseInt(process.argv[4] || '4', 10)
if (!outDir) { console.error('usage: gen_fixtures.cjs <outdir> [version] [per-packet]'); process.exit(2) }
fs.mkdirSync(outDir, { recursive: true })

const data = mcData(version)
const proto = data.protocol
const STATES = ['handshaking', 'status', 'login', 'play', 'configuration']
const DIRS = ['toServer', 'toClient'] // index = the dir number used by the Firn side

// ---------------------------------------------------------------- random
function mulberry32 (a) {
  return function () {
    a |= 0; a = (a + 0x6D2B79F5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}
function hashStr (s) {
  let h = 2166136261
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619) }
  return h >>> 0
}

class Gen {
  constructor (seed, level, localTypes) {
    this.r = mulberry32(seed)
    this.level = level // 0 = smallest values, 1.. = bigger
    this.local = localTypes
  }

  int (lo, hi) { return lo + Math.floor(this.r() * (hi - lo + 1)) }
  pick (arr) { return arr[this.int(0, arr.length - 1)] }
  chance (p) { return this.r() < p }

  lookup (name) {
    if (this.local && this.local[name] !== undefined) return this.local[name]
    return proto.types[name]
  }

  // ---- hints: values for a field that later siblings switch on / use as count
  keysOfSwitchOn (t, fieldName, out) {
    if (!Array.isArray(t)) return
    const [h, a] = t
    if (h === 'switch') {
      if (a.compareTo === fieldName) for (const k of Object.keys(a.fields)) out.push(k)
      for (const v of Object.values(a.fields)) this.keysOfSwitchOn(v, fieldName, out)
      if (a.default) this.keysOfSwitchOn(a.default, fieldName, out)
    } else if (h === 'container') {
      for (const f of a) this.keysOfSwitchOn(f.type, fieldName, out)
    } else if (h === 'array') {
      if (a.count === fieldName) out.push('#count')
      this.keysOfSwitchOn(a.type, fieldName, out)
    } else if (h === 'buffer') {
      if (a.count === fieldName) out.push('#count')
    } else if (h === 'option' || h === 'topBitSetTerminatedArray' || h === 'entityMetadataLoop') {
      this.keysOfSwitchOn(h === 'option' ? a : a.type, fieldName, out)
    } else if (a && a.compareTo === fieldName) {
      // a parametrized named type: its own switch keys are the hints
      const def = this.lookup(h)
      if (Array.isArray(def) && def[0] === 'switch') for (const k of Object.keys(def[1].fields)) out.push(k)
    }
  }

  // ---- value generation. `ctx` is the scope chain: [root, ..., current container object]
  value (t, ctx, hint) {
    if (typeof t === 'string') return this.named(t, ctx, null, hint)
    const [h, a] = t
    switch (h) {
      case 'container': return this.container(a, ctx)
      case 'switch': {
        const v = this.resolveCmp(a.compareTo, ctx, null)
        let key = String(v)
        let tt
        if (Object.prototype.hasOwnProperty.call(a.fields, key)) tt = a.fields[key]
        else tt = a.default || 'void'
        return this.value(tt, ctx)
      }
      case 'array': {
        let n
        if (a.countType) n = this.level === 0 ? this.pick([0, 1]) : this.int(0, 3)
        else if (typeof a.count === 'number') n = a.count
        else n = Number(this.resolveCmp(a.count, ctx, null))
        const arr = []
        for (let i = 0; i < n; i++) arr.push(this.value(a.type, ctx))
        return arr
      }
      case 'option': return this.chance(0.5) ? undefined : this.value(a, ctx)
      case 'mapper': {
        const names = Object.values(a.mappings)
        return this.pick(names)
      }
      case 'bitfield': {
        const o = {}
        for (const f of a) {
          const bits = f.size > 20 ? 20 : f.size
          if (f.signed) o[f.name] = this.int(-(1 << Math.min(bits - 1, 12)) + 1, (1 << Math.min(bits - 1, 12)) - 1)
          else o[f.name] = this.int(0, (1 << Math.min(bits, 12)) - 1)
        }
        return o
      }
      case 'bitflags': {
        const o = { _value: 0 }
        for (const f of a.flags) o[f] = this.chance(0.5)
        return o
      }
      case 'pstring': return this.str()
      case 'buffer': {
        let n
        if (a.countType) n = this.level === 0 ? this.pick([0, 1, 5]) : this.int(0, 40)
        else n = a.count
        return this.buf(n)
      }
      case 'topBitSetTerminatedArray': {
        const n = this.int(1, 3)
        const arr = []
        for (let i = 0; i < n; i++) {
          const el = this.value(a.type, ctx)
          // bit 7 of the first octet is the "more" flag: the first field must not use it
          const k0 = el && typeof el === 'object' ? Object.keys(el)[0] : undefined
          if (k0 !== undefined && typeof el[k0] === 'number') el[k0] = Math.abs(el[k0]) & 127
          arr.push(el)
        }
        return arr
      }
      case 'entityMetadataLoop': {
        const n = this.level === 0 ? this.pick([0, 1]) : this.int(0, 4)
        const arr = []
        for (let i = 0; i < n; i++) arr.push(this.value(a.type, ctx))
        return arr
      }
      case 'registryEntryHolder': {
        if (this.chance(0.5)) return { [a.baseName]: this.int(1, 300) }
        return { [a.otherwise.name]: this.value(a.otherwise.type, ctx) }
      }
      default: {
        // a parametrized named type, e.g. ["particleData", {compareTo: "particleId"}]
        return this.named(h, ctx, a, hint)
      }
    }
  }

  resolveCmp (p, ctx, argValue) {
    if (p.startsWith('$')) return argValue
    const parts = p.split('/')
    let idx = ctx.length - 1
    let i = 0
    while (parts[i] === '..') { idx--; i++ }
    let cur = ctx[idx]
    for (; i < parts.length; i++) {
      if (cur === undefined) throw new Error('compareTo ' + p + ': missing ' + parts[i])
      cur = cur[parts[i]]
    }
    return cur
  }

  named (name, ctx, params, hint) {
    switch (name) {
      case 'varint': case 'optvarint':
        if (hint === '#count') return this.int(0, 3)
        if (hint !== undefined && hint !== null) {
          const n = Number(hint)
          if (!Number.isNaN(n)) return n
        }
        return this.pick([0, 1, 127, 128, 300, 16384, 2097151, 268435455, this.int(0, 100000)])
      case 'varlong': return this.pick([0, 1, 127, 128, 70000, 4294967296, this.int(0, 100000)])
      case 'u8': return this.pick([0, 1, 127, 128, 254, this.int(0, 255)])
      case 'i8': return this.pick([0, 1, -1, 127, -128, this.int(-128, 127)])
      case 'u16': return this.pick([0, 1, 255, 256, 65535, this.int(0, 65535)])
      case 'i16': return this.pick([0, -1, 32767, -32768, this.int(-32768, 32767)])
      case 'u32': return this.pick([0, 1, 4294967295, this.int(0, 2147483647)])
      case 'i32': return this.pick([0, -1, 2147483647, -2147483648, this.int(-1000000, 1000000)])
      case 'u64': return BigInt(this.pick([0, 1, 4294967296, this.int(0, 2147483647)]))
      case 'i64': return BigInt(this.pick([0, -1, 4294967296, -4294967296, this.int(-2147483648, 2147483647)]))
      case 'f32': return this.pick([0, 1, -1.5, 3.25, 1000000.5, this.r() * 100])
      case 'f64': return this.pick([0, 1, -1.5, 3.14159, 1e10, this.r() * 100])
      case 'bool': return hint !== undefined && hint !== null && (hint === 'true' || hint === 'false') ? hint === 'true' : this.chance(0.5)
      case 'UUID': {
        let s = ''
        for (let i = 0; i < 32; i++) s += '0123456789abcdef'[this.int(0, 15)]
        return s
      }
      case 'void': return undefined
      case 'restBuffer': return this.buf(this.level === 0 ? 0 : this.int(0, 12))
      case 'nbt': return this.nbtRoot()
      case 'optionalNbt': return this.chance(0.5) ? undefined : this.nbtRoot()
      case 'anonymousNbt': return this.nbtRoot()
      case 'anonOptionalNbt': return this.chance(0.5) ? undefined : this.nbtRoot()
      default: break
    }
    const def = this.lookup(name)
    if (def === 'native') throw new Error('no generator for native type ' + name)
    if (typeof def === 'string') return this.named(def, ctx, params, hint)
    // A parametrized switch: the compare value is the value of the field named in params.compareTo
    if (params && params.compareTo !== undefined && def[0] === 'switch') {
      const cmp = this.resolveCmp(params.compareTo, ctx, null)
      const key = String(cmp)
      const sw = def[1]
      const tt = Object.prototype.hasOwnProperty.call(sw.fields, key) ? sw.fields[key] : (sw.default || 'void')
      // a "$compareTo" inside the case types would need the argument; none occur in 1.20.1 cases
      return this.value(tt, ctx)
    }
    // a named type is a fresh context (compareTo paths inside it are relative to it)
    if (def[0] === 'container') return this.container(def[1], [undefined])
    return this.value(def, ctx, hint)
  }

  container (fields, ctx) {
    const obj = {}
    const sc = ctx.concat([obj])
    for (let i = 0; i < fields.length; i++) {
      const f = fields[i]
      let hint
      if (f.name !== undefined) {
        const keys = []
        for (let j = i + 1; j < fields.length; j++) this.keysOfSwitchOn(fields[j].type, f.name, keys)
        if (keys.length > 0) {
          const k = this.pick(keys)
          hint = k
        }
      }
      const v = this.value(f.type, sc, hint)
      if (f.anon) { if (v && typeof v === 'object') Object.assign(obj, v) } else obj[f.name] = v
    }
    return obj
  }

  str () {
    const n = this.level === 0 ? this.pick([0, 1, 3]) : this.int(0, 12)
    let s = ''
    for (let i = 0; i < n; i++) s += String.fromCharCode(this.int(97, 122))
    return s
  }

  buf (n) {
    const b = Buffer.alloc(n)
    for (let i = 0; i < n; i++) b[i] = this.int(0, 255)
    return b
  }

  nbtRoot () {
    const depth = this.level === 0 ? 0 : 2
    return nbt.comp(this.nbtCompound(depth), '')
  }

  nbtCompound (depth) {
    const o = {}
    const n = this.level === 0 ? this.int(0, 1) : this.int(0, 4)
    for (let i = 0; i < n; i++) {
      const name = 'k' + i
      const kind = this.int(0, depth > 0 ? 9 : 7)
      switch (kind) {
        case 0: o[name] = nbt.byte(this.int(-128, 127)); break
        case 1: o[name] = nbt.short(this.int(-32768, 32767)); break
        case 2: o[name] = nbt.int(this.int(-100000, 100000)); break
        case 3: o[name] = nbt.long(BigInt(this.int(-100000, 100000))); break
        case 4: o[name] = nbt.float(this.r() * 10); break
        case 5: o[name] = nbt.double(this.r() * 10); break
        case 6: o[name] = nbt.string(this.str()); break
        case 7: o[name] = nbt.byteArray(Array.from(this.buf(this.int(0, 5)))); break
        case 8: o[name] = nbt.list(nbt.int([this.int(0, 9), this.int(0, 9)])); break
        default: o[name] = nbt.comp(this.nbtCompound(depth - 1)); break
      }
    }
    return o
  }
}

// does the packet type end in a restBuffer (any tail is valid then)?
function endsInRest (t, local) {
  if (typeof t === 'string') {
    if (t === 'restBuffer') return true
    const def = (local && local[t]) || proto.types[t]
    if (def && def !== 'native') return endsInRest(def, local)
    return false
  }
  if (t[0] === 'container') return t[1].length > 0 && endsInRest(t[1][t[1].length - 1].type, local)
  if (t[0] === 'option') return endsInRest(t[1], local)
  if (t[0] === 'switch') return Object.values(t[1].fields).some(x => endsInRest(x, local)) || (t[1].default ? endsInRest(t[1].default, local) : false)
  return false
}

// ---------------------------------------------------------------- main
const manifest = []
const dropped = []
let n = 0
let total = 0
for (let si = 0; si < STATES.length; si++) {
  if (!proto[STATES[si]]) continue
  for (let di = 0; di < DIRS.length; di++) {
    const dirTypes = proto[STATES[si]][DIRS[di]].types
    const pk = dirTypes.packet
    const mapping = pk[1][0].type[1].mappings
    const isServerSerializer = di === 1 // the serializer of toClient packets is the server's
    const ser = createSerializer({ state: STATES[si], isServer: isServerSerializer, version })
    const des = createDeserializer({ state: STATES[si], isServer: di === 0, version, noErrorLogging: true })
    for (const [idStr, pname] of Object.entries(mapping)) {
      const id = Number(idStr)
      const tname = pk[1][1].type[1].fields[pname]
      const rest = endsInRest(tname, dirTypes)
      let okCount = 0
      for (let k = 0; k < perPacket * 3 && okCount < perPacket; k++) {
        const level = k % 2 === 0 ? 0 : 1
        const g = new Gen(hashStr(STATES[si] + DIRS[di] + pname) + k * 7919, level, dirTypes)
        let params
        try {
          params = g.named(tname, [undefined], null)
        } catch (e) { dropped.push(`${STATES[si]}/${DIRS[di]}/${pname}: generator: ${e.message}`); break }
        total++
        let buf
        try {
          buf = ser.createPacketBuffer({ name: pname, params })
        } catch (e) { dropped.push(`${STATES[si]}/${DIRS[di]}/${pname} #${k}: serialize: ${e.message}`); continue }
        // strip the id varint
        let p = 0
        while (buf[p] & 0x80) p++
        p++
        // copy: the reference parser clears the top bits IN PLACE (topBitSetTerminatedArray)
        const body = Buffer.from(buf.subarray(p))
        const whole = Buffer.from(buf)
        // the reference must read it back EXACTLY
        try {
          const r = des.parsePacketBuffer(whole)
          if (!r || r.metadata.size !== buf.length || r.data.name !== pname) {
            dropped.push(`${STATES[si]}/${DIRS[di]}/${pname} #${k}: reference read ${r && r.metadata.size} of ${buf.length}`)
            continue
          }
        } catch (e) { dropped.push(`${STATES[si]}/${DIRS[di]}/${pname} #${k}: reference parse: ${e.message}`); continue }
        const file = `${n++}.bin`
        fs.writeFileSync(path.join(outDir, file), body)
        manifest.push(`${si} ${di} ${id} ${pname} ${file} ${rest ? 'rest' : '-'}`)
        okCount++
      }
    }
  }
}
fs.writeFileSync(path.join(outDir, 'manifest.txt'), manifest.join('\n') + '\n')
if (dropped.length) fs.writeFileSync(path.join(outDir, 'dropped.txt'), dropped.join('\n') + '\n')
console.log(`fixtures: ${manifest.length} written, ${dropped.length} dropped (of ${total} tries), version ${version}`)

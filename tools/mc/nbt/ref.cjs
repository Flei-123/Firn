// tools/mc/nbt/ref.cjs -- the reference for mc_nbt: random NBT trees written by prismarine-nbt (an
// independent implementation) and the dump lines computed from the GENERATED tree, not from any parser.
//
//   NODE_PATH=<node_modules> node ref.cjs <outdir> <count> <seed>
//
// writes <outdir>/t<i>.nbt (named root) and <outdir>/t<i>.anon.nbt (the nameless root of 1.20.2+, made by
// removing the root name from the same bytes) plus <outdir>/t<i>.txt with the expected dump lines.
const nbt = require('prismarine-nbt')
const fs = require('fs')
const outDir = process.argv[2]
const count = parseInt(process.argv[3] || '40', 10)
let state = parseInt(process.argv[4] || '1', 10)
fs.mkdirSync(outDir, { recursive: true })

function rnd () { state = (state * 1103515245 + 12345) % 2147483648; return state >> 8 }
function pick (n) { return rnd() % n }

function fnvBytes (buf) {
  let h = 0xcbf29ce484222325n
  for (const b of buf) { h = BigInt.asUintN(64, (h ^ BigInt(b)) * 0x100000001b3n) }
  return h.toString(16).padStart(16, '0')
}

const NAMES = ['alpha', 'b', 'c d', 'ünï', 'x/y', 'long_name_' + 'z'.repeat(40), '', 'minecraft:thing', 'k9']
function name () { return NAMES[pick(NAMES.length)] + (pick(3) === 0 ? String(pick(100)) : '') }
function str () {
  const parts = ['', 'a', 'hello world', 'ünïcödé', '日本語', 'emoji-free text with spaces', 'x'.repeat(300)]
  return parts[pick(parts.length)] + (pick(2) ? String(pick(1000)) : '')
}
function i32 () { return [0, 1, -1, 127, -128, 32767, -32768, 2147483647, -2147483648, pick(1000000) - 500000][pick(10)] }
function f32 () { return Math.fround([0, 1.5, -2.25, 3.4e38, 1e-30, pick(1000) / 7][pick(6)]) }
function f64 () { return [0, 1.5, -2.25, 1e300, 1e-300, pick(1000) / 7][pick(6)] }
function i64pair () { return [i32(), i32()] }

const KINDS = ['byte', 'short', 'int', 'long', 'float', 'double', 'string', 'byteArray', 'intArray', 'longArray', 'list', 'compound']
function make (t, depth) {
  switch (t) {
    case 'byte': return { type: 'byte', value: [0, 1, -1, 127, -128][pick(5)] }
    case 'short': return { type: 'short', value: [0, 1, -1, 32767, -32768, pick(20000)][pick(6)] }
    case 'int': return { type: 'int', value: i32() }
    case 'long': return { type: 'long', value: i64pair() }
    case 'float': return { type: 'float', value: f32() }
    case 'double': return { type: 'double', value: f64() }
    case 'string': return { type: 'string', value: str() }
    case 'byteArray': return { type: 'byteArray', value: Array.from({ length: pick(40) }, () => pick(256) - 128) }
    case 'intArray': return { type: 'intArray', value: Array.from({ length: pick(30) }, () => i32()) }
    case 'longArray': return { type: 'longArray', value: Array.from({ length: pick(20) }, () => i64pair()) }
    case 'compound': return { type: 'compound', value: makeCompound(depth + 1) }
    case 'list': {
      if (depth > 6 || pick(6) === 0) return { type: 'list', value: { type: 'end', value: [] } }
      const et = KINDS[pick(KINDS.length)]
      const n = pick(5)
      const vals = []
      for (let i = 0; i < n; i++) vals.push(make(et, depth + 1).value)
      return { type: 'list', value: { type: et, value: vals } }
    }
  }
}
function makeCompound (depth) {
  const o = {}
  const n = depth > 8 ? 0 : (depth === 0 ? 3 + pick(6) : pick(6))
  for (let i = 0; i < n; i++) {
    let k = 'k' + name()
    while (k in o) k += '_'
    o[k] = make(KINDS[pick(KINDS.length)], depth)
  }
  return o
}

const L = []
const nameHex = (s) => Buffer.from(s, 'utf8').toString('hex')
function f32bits (v) { const b = Buffer.alloc(4); b.writeFloatBE(v); return b.readUInt32BE().toString(16).padStart(8, '0') }
function f64bits (v) { const b = Buffer.alloc(8); b.writeDoubleBE(v); return b.readBigUInt64BE().toString(16).padStart(16, '0') }
function i64str (p) { return BigInt.asIntN(64, (BigInt(p[0]) << 32n) | BigInt.asUintN(32, BigInt(p[1]))).toString() }
function dumpValue (path, type, value) {
  switch (type) {
    case 'byte': case 'short': case 'int': L.push(`${path} ${type} ${value}`); break
    case 'long': L.push(`${path} long ${i64str(value)}`); break
    case 'float': L.push(`${path} float ${f32bits(value)}`); break
    case 'double': L.push(`${path} double ${f64bits(value)}`); break
    case 'string': { const b = Buffer.from(value, 'utf8'); L.push(`${path} string len=${b.length} hex=${b.toString('hex')}`); break }
    case 'byteArray': { const b = Buffer.from(Int8Array.from(value).buffer); L.push(`${path} byte_array n=${value.length} fnv=${fnvBytes(b)}`); break }
    case 'intArray': { const b = Buffer.alloc(value.length * 4); value.forEach((v, i) => b.writeInt32BE(v, i * 4)); L.push(`${path} int_array n=${value.length} fnv=${fnvBytes(b)}`); break }
    case 'longArray': { const b = Buffer.alloc(value.length * 8); value.forEach((p, i) => b.writeBigInt64BE(BigInt(i64str(p)), i * 8)); L.push(`${path} long_array n=${value.length} fnv=${fnvBytes(b)}`); break }
    case 'list': {
      const t = value.type
      const snake = { byteArray: 'byte_array', intArray: 'int_array', longArray: 'long_array' }
      L.push(`${path} list elem=${snake[t] || t} n=${value.value.length}`)
      value.value.forEach((v, i) => dumpValue(`${path}[${i}]`, t, v))
      break
    }
    case 'compound': {
      const keys = Object.keys(value)
      L.push(`${path} compound n=${keys.length}`)
      for (const k of keys) dumpValue(`${path}/${nameHex(k)}`, value[k].type, value[k].value)
      break
    }
  }
}

for (let i = 0; i < count; i++) {
  const rootName = pick(4) === 0 ? '' : name()
  const comp = makeCompound(0)
  const tree = { type: 'compound', name: rootName, value: comp }
  const buf = nbt.writeUncompressed(tree, 'big')
  const nl = buf.readUInt16BE(1)
  const anon = Buffer.concat([buf.subarray(0, 1), buf.subarray(3 + nl)])
  L.length = 0
  L.push(`root-name hex=${nameHex(rootName)}`)
  dumpValue('', 'compound', comp)
  fs.writeFileSync(`${outDir}/t${i}.nbt`, buf)
  fs.writeFileSync(`${outDir}/t${i}.anon.nbt`, anon)
  fs.writeFileSync(`${outDir}/t${i}.txt`, L.join('\n') + '\n')
  const back = nbt.writeUncompressed(nbt.parseUncompressed(buf), 'big')
  if (!back.equals(buf)) { console.error('generator self-check failed for tree ' + i); process.exit(1) }
}
console.log(`ref: ${count} trees written to ${outDir}`)

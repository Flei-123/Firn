// SPDX-License-Identifier: MPL-2.0
// codec.cjs -- reference packet serializer for the entity cross-check: node-minecraft-protocol's
// serializer where it supports the version, else the protodef compiler on minecraft-data's
// protocol.json (same approach and same two helper types as tools/mcgen/gen_fixtures.cjs).
'use strict'
const mcData = require('minecraft-data')
const nbt = require('prismarine-nbt')
const { createSerializer } = require('minecraft-protocol/src/transforms/serializer')
const { ProtoDefCompiler } = require('protodef').Compiler
const { Serializer } = require('protodef')
const nmpVersions = require('minecraft-protocol/src/version').supportedVersions

function writeVarint (v, buffer, offset) {
  v >>>= 0
  while (v >= 0x80) { buffer[offset++] = (v & 0x7f) | 0x80; v >>>= 7 }
  buffer[offset++] = v
  return offset
}
function sizeVarint (v) { v >>>= 0; let n = 1; while (v >= 0x80) { n++; v >>>= 7 } return n }
const LPVEC3 = {
  Read: {
    lpVec3: ['native', (buffer, offset) => {
      const lowest = buffer.readUInt8(offset)
      if (lowest === 0) return { value: { lowest, middle: 0, highest: 0, ext: 0 }, size: 1 }
      const middle = buffer.readUInt8(offset + 1)
      const highest = buffer.readUInt32BE(offset + 2)
      let size = 6
      let ext = 0
      if ((lowest & 4) !== 0) {
        let shift = 0
        let b
        do { b = buffer.readUInt8(offset + size); size++; ext |= (b & 0x7f) << shift; shift += 7 } while (b & 0x80)
        ext >>>= 0
      }
      return { value: { lowest, middle, highest, ext }, size }
    }]
  },
  Write: {
    lpVec3: ['native', (value, buffer, offset) => {
      buffer.writeUInt8(value.lowest, offset++)
      if (value.lowest === 0) return offset
      buffer.writeUInt8(value.middle, offset++)
      buffer.writeUInt32BE(value.highest, offset); offset += 4
      if ((value.lowest & 4) !== 0) offset = writeVarint(value.ext, buffer, offset)
      return offset
    }]
  },
  SizeOf: {
    lpVec3: ['native', (value) => {
      if (value.lowest === 0) return 1
      return 6 + ((value.lowest & 4) !== 0 ? sizeVarint(value.ext) : 0)
    }]
  }
}
// node-minecraft-protocol's registryEntryHolder is wrong in all three directions for the
// INLINE form (varint 0, then the value): the reader calls the inline type at the wrong offset
// and returns `holder.data`, the writer and sizeOf forget the leading 0. It cannot read its
// own output, and the bytes it writes are not the protocol. Corrected versions replace them
// here (the registry-id form is fine and unchanged in behaviour).
const holderParts = (opts) => ({
  base: `value.${opts.baseName}`,
  other: `value.${opts.otherwise.name}`
})
const FIXED_HOLDER = {
  Read: {
    registryEntryHolder: ['parametrizable', (compiler, opts) => compiler.wrapCode(`
const { value: n, size: nSize } = ${compiler.callType('varint')}
if (n !== 0) {
  return { value: { ${opts.baseName}: n - 1 }, size: nSize }
}
const holder = ${compiler.callType(opts.otherwise.type, 'offset + nSize')}
return { value: { ${opts.otherwise.name}: holder.value }, size: nSize + holder.size }
`.trim())]
  },
  Write: {
    registryEntryHolder: ['parametrizable', (compiler, opts) => {
      const h = holderParts(opts)
      return compiler.wrapCode(`
if (${h.base} !== undefined) {
  offset = ${compiler.callType(`${h.base} + 1`, 'varint')}
} else if (${h.other} !== undefined) {
  offset = ${compiler.callType(0, 'varint')}
  offset = ${compiler.callType(h.other, opts.otherwise.type)}
} else {
  throw new Error('registryEntryHolder needs ${opts.baseName} or ${opts.otherwise.name}')
}
return offset
`.trim())
    }]
  },
  SizeOf: {
    registryEntryHolder: ['parametrizable', (compiler, opts) => {
      const h = holderParts(opts)
      return compiler.wrapCode(`
let size = 0
if (${h.base} !== undefined) {
  size += ${compiler.callType(`${h.base} + 1`, 'varint')}
} else if (${h.other} !== undefined) {
  size += ${compiler.callType(0, 'varint')}
  size += ${compiler.callType(h.other, opts.otherwise.type)}
} else {
  throw new Error('registryEntryHolder needs ${opts.baseName} or ${opts.otherwise.name}')
}
return size
`.trim())
    }]
  }
}

const customProtocols = {}
function customProtocol (proto, state, direction) {
  const key = state + ';' + direction
  if (customProtocols[key]) return customProtocols[key]
  const compiler = new ProtoDefCompiler()
  compiler.addTypes(require('minecraft-protocol/src/datatypes/compiler-minecraft'))
  compiler.addTypes(LPVEC3)
  compiler.addTypes(FIXED_HOLDER)
  compiler.addProtocol(proto, [state, direction])
  nbt.addTypesToCompiler('big', compiler)
  customProtocols[key] = compiler.compileProtoDefSync()
  return customProtocols[key]
}

// serializer of the play packets the SERVER sends (toClient); returns { data, serialize(name, params) -> body without id }
function playToClient (version) {
  const data = mcData(version)
  const proto = data.protocol
  const viaNmp = nmpVersions.includes(version) && !JSON.stringify(proto).includes('"registryEntryHolder",')
  const ser = viaNmp
    ? createSerializer({ state: 'play', isServer: true, version })
    : new Serializer(customProtocol(proto, 'play', 'toClient'), 'packet')
  return {
    data,
    viaNmp,
    serialize (name, params) {
      const buf = ser.createPacketBuffer({ name, params })
      // strip the leading packet id varint
      let i = 0
      while (buf[i] & 0x80) i++
      return buf.subarray(i + 1)
    }
  }
}
module.exports = { playToClient }

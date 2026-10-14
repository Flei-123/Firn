// tools/mc/registry/ref.cjs -- the reference for dump.fi: the same text from minecraft-data
// and prismarine-block (an independent implementation of the state decoding).
//   NODE_PATH=<node_modules> node ref.cjs <minecraft-version> <protocol>
const mcData = require('minecraft-data')
const version = process.argv[2]
const protocol = process.argv[3]
const d = mcData(version)
const Block = require('prismarine-block')(version)
const out = []
const legacy = d.blocksArray.length > 0 && d.blocksArray[0].minStateId === undefined
out.push(`version ${protocol}`)
let blocks = d.blocksArray.map(b => Object.assign({}, b))
if (legacy) {
  for (const b of blocks) { b.minStateId = b.id * 16; b.maxStateId = b.id * 16 + 15; b.defaultState = b.id * 16; b.states = [] }
}
blocks.sort((a, b) => a.minStateId - b.minStateId)
const flags = b => (b.boundingBox === 'block' ? 1 : 0) | (b.transparent ? 2 : 0) | (b.diggable ? 4 : 0) |
  (((b.filterLight || 0) & 15) << 4) | (((b.emitLight || 0) & 15) << 8)
// minecraft-data 1.16.x lists these blocks as hardness 0 (instant) but the game says unbreakable (-1)
const UNBREAKABLE = new Set(['bedrock', 'barrier', 'light', 'command_block', 'chain_command_block', 'repeating_command_block', 'structure_block',
  'jigsaw', 'end_portal', 'end_portal_frame', 'end_gateway', 'nether_portal', 'moving_piston'])
blocks.forEach((b, i) => {
  if (b.hardness === 0 && UNBREAKABLE.has(b.name)) b.hardness = null
  const hard = (b.hardness === null || b.hardness === undefined || b.hardness < 0) ? -1 : Math.round(b.hardness * 1000)
  out.push(`B ${i} ${b.name} ${b.minStateId} ${b.maxStateId} ${b.defaultState} flags=${flags(b)} hard=${hard} stack=${b.stackSize === undefined ? 64 : b.stackSize} props=${(b.states || []).length}`)
})
const nstates = Math.max(...blocks.map(b => b.maxStateId)) + 1
const owner = new Array(nstates).fill(null)
for (const b of blocks) for (let s = b.minStateId; s <= b.maxStateId; s++) owner[s] = b
for (let s = 0; s < nstates; s++) {
  const b = owner[s]
  if (!b) { out.push(`S ${s} -`); continue }
  if (legacy || !b.states || b.states.length === 0) { out.push(`S ${s} ${b.name}`); continue }
  // prismarine-block decodes the state id into properties (last property fastest; bool 0 = true)
  const props = Block.fromStateId(s, 0).getProperties()
  const parts = b.states.map(p => `${p.name}=${props[p.name]}`)
  out.push(`S ${s} ${b.name}[${parts.join(',')}]`)
}
const items = d.itemsArray.slice().sort((a, b) => a.id - b.id)
for (const it of items) out.push(`I ${it.id} ${it.name} ${it.stackSize === undefined ? 64 : it.stackSize}`)
const cmp = (x, y) => (x < y ? -1 : x > y ? 1 : 0)
const ents = d.entitiesArray.slice().sort((a, b) => (a.id - b.id) || cmp(a.type || '', b.type || '') || cmp(a.name, b.name))
for (const e of ents) out.push(`E ${e.id} ${e.name} ${Math.round((e.width || 0) * 1000)} ${Math.round((e.height || 0) * 1000)} ${e.type || ''}`)
const bios = d.biomesArray.slice().sort((a, b) => (a.id - b.id) || cmp(a.name, b.name))
for (const b of bios) out.push(`M ${b.id} ${b.name} ${b.category || ''} ${b.color || 0}`)
process.stdout.write(out.join('\n') + '\n')

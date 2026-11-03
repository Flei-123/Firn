'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))
const mcData = require(path.join(root, 'minecraft-data'))('1.20.1')
const scenarios = require('./inv_scenarios.cjs')

const [host, portArg, name, outPath] = process.argv.slice(2)
const only = process.env.INV_ONLY ? process.env.INV_ONLY.split(',') : null
const modes = (process.env.INV_MODES || 'creative,survival').split(',')
const SLOTS = 46
const step = 12

const c = mc.createClient({ host, port: parseInt(portArg, 10), username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
let slots = new Array(SLOTS).fill(null)
let cursor = null
let stateId = 0
let fulls = 0
let gm = -1
let gmWaiters = []
let pos = null
let teleported = false
const errors = []

const toStack = (it) => (it && it.present ? { id: it.itemId, n: it.itemCount } : null)
const label = (s) => (s ? mcData.items[s.id].name + '*' + s.n : null)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const wire = (nm, n) => (n && n > 0 ? { present: true, itemId: mcData.itemsByName[nm].id, itemCount: n, nbtData: undefined } : { present: false })

c.on('error', (e) => errors.push(String(e.message)))
c.on('packet', (d, meta) => {
  if (meta.state !== 'play') return
  if (meta.name === 'position') {
    if (!pos) pos = { x: d.x, y: d.y, z: d.z }
    c.write('teleport_confirm', { teleportId: d.teleportId })
    c.write('position_look', { x: pos.x, y: pos.y, z: pos.z, yaw: 0, pitch: 0, onGround: true })
    teleported = true
  } else if (meta.name === 'window_items' && d.windowId === 0) {
    stateId = d.stateId
    slots = d.items.map(toStack)
    cursor = toStack(d.carriedItem)
    fulls++
  } else if (meta.name === 'set_slot') {
    if (d.windowId === -1 && d.slot === -1) { cursor = toStack(d.item); fulls++ }
    else if (d.windowId === 0) { stateId = d.stateId; slots[d.slot] = toStack(d.item) }
  } else if (meta.name === 'game_state_change' && d.reason === 3) {
    gm = d.gameMode
    const w = gmWaiters
    gmWaiters = []
    w.forEach((f) => f())
  }
})

async function setMode(m) {
  const want = m === 'creative' ? 1 : 0
  if (gm === want) return
  await new Promise((res) => {
    gmWaiters.push(res)
    c.chat('/gamemode ' + m)
    setTimeout(res, 3000)
  })
  await sleep(100)
}

async function sync() {
  const before = fulls
  c.write('window_click', { windowId: 0, stateId: stateId + 77, slot: -1, mouseButton: 0, mode: 0, changedSlots: [], cursorItem: { present: false } })
  for (let i = 0; i < 100 && fulls === before; i++) await sleep(30)
  await sleep(120)
}

function click(slot, button, mode) {
  c.write('window_click', { windowId: 0, stateId, slot, mouseButton: button, mode, changedSlots: [], cursorItem: { present: false } })
}

async function clearAll() {
  for (let k = 1; k < SLOTS; k++) c.write('set_creative_slot', { slot: k, item: { present: false } })
  await sleep(60)
}

async function hop() {
  for (let i = 0; i < 2; i++) {
    pos.x += step / 2
    c.write('position', { x: pos.x, y: pos.y, z: pos.z, onGround: true })
    await sleep(60)
  }
}

async function runScenario(sc, mode) {
  await setMode('creative')
  await clearAll()
  for (const [k, [nm, n]] of Object.entries(sc.seed)) c.write('set_creative_slot', { slot: parseInt(k, 10), item: wire(nm, n) })
  await sleep(80)
  if (mode === 'survival') await setMode('survival')
  await sync()
  const start = slots.map(label)
  for (const [slot, button, m] of sc.steps) {
    click(slot, button, m)
    await sleep(8)
  }
  await sleep(100)
  await sync()
  const res = { name: sc.name, start, slots: slots.map(label), cursor: label(cursor) }
  await setMode('creative')
  click(-999, 0, 0)
  await sleep(60)
  await hop()
  return res
}

async function main() {
  for (let i = 0; i < 300 && !teleported; i++) await sleep(50)
  await sleep(800)
  const out = { name, modes: {}, errors }
  for (const mode of modes) {
    out.modes[mode] = []
    for (const sc of scenarios) {
      if (only && !only.includes(sc.name)) continue
      out.modes[mode].push(await runScenario(sc, mode))
    }
  }
  require('fs').writeFileSync(outPath, JSON.stringify(out))
  try { c.end() } catch (e) {}
  setTimeout(() => process.exit(0), 300)
}

main().catch((e) => { console.error(e); process.exit(1) })
setTimeout(() => { console.error('timeout'); process.exit(2) }, parseInt(process.env.INV_TIMEOUT_MS || '600000', 10))

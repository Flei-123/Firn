const fs = require('fs')
const path = require('path')
const mc = require('/root/firn/tools/mcserver/node_modules/minecraft-protocol')
const [host, port, name, listPath, outPath, barrier] = process.argv.slice(2)
const slots = fs.readFileSync(listPath, 'utf8').split('\n').filter((l) => l.length > 0).map((l) => {
  const [who, kind, ...rest] = l.split('|')
  return { who: who === 'Op' ? 'OpBot' : 'NoBot', kind, text: rest.join('|') }
})
const intervalMs = parseInt(process.env.SEL_INTERVAL_MS || '600', 10)
const leadMs = 1500
const events = slots.map((s) => ({ who: s.who, kind: s.kind, text: s.text, chat: [], pos: [], modes: [], tab: null }))
const flat = require('./lang_text.cjs').chatFlattener(process.env.CMD_LANG)
const slotFile = path.join(barrier, 'slot')
const current = () => {
  let i = -1
  try { i = parseInt(fs.readFileSync(slotFile, 'utf8'), 10) } catch (e) { return null }
  return i >= 0 && i < events.length ? events[i] : null
}
const c = mc.createClient({ host, port: parseInt(port, 10), username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
c.on('system_chat', (p) => { const s = current(); if (s) s.chat.push(flat(p.content)) })
c.on('player_chat', (p) => { const s = current(); if (s) s.chat.push('[' + flat(p.networkName) + '] ' + (p.plainMessage || '')) })
c.on('position', (p) => { const s = current(); if (s) s.pos.push([p.x, p.y, p.z, p.yaw, p.pitch, p.flags]); c.write('teleport_confirm', { teleportId: p.teleportId }) })
c.on('game_state_change', (p) => { const s = current(); if (s && p.reason === 3) s.modes.push(p.gameMode) })
c.on('tab_complete', (p) => {
  const e = events[p.transactionId - 1]
  if (e) e.tab = { start: p.start, length: p.length, matches: p.matches.map((m) => m.match) }
})
c.on('error', (e) => console.error(String(e.message).slice(0, 300)))
const done = () => { fs.writeFileSync(outPath, JSON.stringify({ name, events })); process.exit(0) }
const bothReady = () => ['OpBot', 'NoBot'].every((n) => fs.existsSync(path.join(barrier, 'ready_' + n)))
c.on('login', () => {
  setTimeout(() => {
    fs.writeFileSync(path.join(barrier, 'ready_' + name), '1')
    const wait = setInterval(() => {
      if (!bothReady()) return
      clearInterval(wait)
      slots.forEach((s, i) => {
        if (s.who !== name) return
        setTimeout(() => {
          fs.writeFileSync(slotFile, String(i))
          if (s.kind === 'cmd') c.chat('/' + s.text)
          else c.write('tab_complete', { transactionId: i + 1, text: s.text })
        }, leadMs + i * intervalMs)
      })
      setTimeout(done, leadMs + slots.length * intervalMs + 2500)
    }, 50)
  }, 1500)
})
setTimeout(() => { console.error('timeout'); done() }, 120000)

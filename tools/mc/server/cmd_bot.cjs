const fs = require('fs')
const mc = require('/root/firn/tools/mcserver/node_modules/minecraft-protocol')
const [host, port, name, listPath, outPath] = process.argv.slice(2)
const cmds = fs.readFileSync(listPath, 'utf8').split('\n').filter((l) => l.length > 0)
const intervalMs = parseInt(process.env.CMD_INTERVAL_MS || '500', 10)
const events = cmds.map((cmd) => ({ cmd, chat: [], pos: [], modes: [], times: [] }))
let cur = -1
const slot = () => (cur >= 0 ? events[cur] : null)
const c = mc.createClient({ host, port: parseInt(port, 10), username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
const flat = require('./lang_text.cjs').chatFlattener(process.env.CMD_LANG)
c.on('system_chat', (p) => { const s = slot(); if (s) s.chat.push(flat(p.content)) })
c.on('player_chat', (p) => { const s = slot(); if (s) s.chat.push('[' + flat(p.networkName) + '] ' + (p.plainMessage || '')) })
c.on('position', (p) => { const s = slot(); if (s) s.pos.push([p.x, p.y, p.z, p.yaw, p.pitch, p.flags]); c.write('teleport_confirm', { teleportId: p.teleportId }) })
c.on('game_state_change', (p) => { const s = slot(); if (s && p.reason === 3) s.modes.push(p.gameMode) })
c.on('update_time', (p) => { const s = slot(); if (s) s.times.push(Number(p.time)) })
c.on('error', (e) => console.error(String(e.message).slice(0, 300)))
const done = () => { fs.writeFileSync(outPath, JSON.stringify({ name, events })); process.exit(0) }
c.on('login', () => {
  setTimeout(() => {
    const t = setInterval(() => {
      if (cur + 1 >= cmds.length) { clearInterval(t); setTimeout(done, 1500); return }
      cur++
      c.chat('/' + cmds[cur])
    }, intervalMs)
  }, 1500)
})
setTimeout(() => { console.error('timeout'); done() }, 90000)

const mc = require('/root/firn/tools/mcserver/node_modules/minecraft-protocol')
const [host, port, name, ...cmds] = process.argv.slice(2)
const out = { nodes: 0, names: [], chat: [], pos: [], modes: [], times: [] }
const c = mc.createClient({ host, port: parseInt(port, 10), username: name, auth: 'offline', version: '1.20.1', hideErrors: false })
const flat = (j) => { try { j = typeof j === 'string' ? JSON.parse(j) : j } catch (e) { return String(j) } if (typeof j === 'string') return j; return (j.text || '') + (j.extra || []).map(flat).join('') }
c.on('declare_commands', (p) => { out.nodes = p.nodes.length; out.names = p.nodes.map((n) => (n.extraNodeData && n.extraNodeData.name ? n.extraNodeData.name : '') + ':' + (n.extraNodeData && n.extraNodeData.parser ? n.extraNodeData.parser : '')) })
c.on('system_chat', (p) => out.chat.push(flat(p.content)))
c.on('player_chat', (p) => out.chat.push('[' + flat(p.networkName) + '] ' + (p.plainMessage || '')))
c.on('position', (p) => { out.pos.push([p.x, p.y, p.z]); c.write('teleport_confirm', { teleportId: p.teleportId }) })
c.on('game_state_change', (p) => { if (p.reason === 3) out.modes.push(p.gameMode) })
c.on('update_time', (p) => out.times.push(Number(p.time)))

c.on('error', (e) => console.error(String(e.message).slice(0, 300)))
c.on('login', () => { let i = 0; const t = setInterval(() => { if (i >= cmds.length) { clearInterval(t); setTimeout(() => { console.log(JSON.stringify(out)); process.exit(0) }, 1500); return } c.chat('/' + cmds[i++]) }, 700) })
setTimeout(() => { console.log(JSON.stringify(out)); process.exit(1) }, 25000)

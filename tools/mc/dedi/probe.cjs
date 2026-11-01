'use strict'
const path = require('path')
const root = process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules'
const mc = require(path.join(root, 'minecraft-protocol'))

const [mode, host, portArg, nameArg] = process.argv.slice(2)
const port = parseInt(portArg, 10)
const VERSION = '1.20.1'

function done(obj) {
  console.log(JSON.stringify(obj))
  process.exit(0)
}

setTimeout(() => done({ error: 'timeout' }), 15000)

if (mode === 'ping') {
  mc.ping({ host, port, version: VERSION }, (err, res) => {
    if (err) return done({ error: String(err) })
    const desc = typeof res.description === 'string' ? res.description : (res.description.text || JSON.stringify(res.description))
    done({ motd: desc, max: res.players.max, online: res.players.online, version: res.version.name, protocol: res.version.protocol })
  })
} else {
  const client = mc.createClient({ host, port, username: nameArg || 'Probe', version: VERSION, auth: 'offline' })
  let joined = false
  client.on('login', () => {
    joined = true
    setTimeout(() => { client.end('bye'); setTimeout(() => done({ joined: true }), 300) }, parseInt(process.env.HOLD_MS || '800', 10))
  })
  client.on('kick_disconnect', (p) => { if (!joined) done({ joined: false, kick: p.reason }) })
  client.on('disconnect', (p) => { if (!joined) done({ joined: false, kick: p.reason }) })
  client.on('error', (e) => { if (!joined) done({ joined: false, error: String(e) }) })
}

// tools/mcserver/nmp_client.cjs -- a THIRD implementation of the client.
//
// The extension is `.cjs` and not `.js` on purpose: a `package.json` with
// `"type": "module"` ANYWHERE above this directory would otherwise turn the
// file into an ES module and `require` would stop existing. `.cjs` says
// CommonJS regardless of what stands above.
//
// `tools/mcserver/harness.py` and the server share an author, and that is a
// weakness of every self written test: two sides can be wrong in the same
// way. `node-minecraft-protocol` (PrismarineJS) was not: it is the client
// library thousands of bots run on, it has its own packet definitions out
// of `minecraft-data`, and it validates every field against them. If it
// reads a packet of this server differently than the server meant it, it
// throws -- it does not shrug.
//
// What is checked here is the one thing that decides whether a human player
// would be in the world: the `login` event carries the Join Game packet,
// and `position` carries the Synchronize Player Position that ends the
// loading screen.
//
//     node nmp_client.cjs <host> <port> <name>
//
// exit 0 = the client is in the world, everything else is a failure.
const mc = require('minecraft-protocol')

const host = process.argv[2] || '127.0.0.1'
const port = parseInt(process.argv[3] || '25565', 10)
const username = process.argv[4] || 'NmpBot'

const seen = { login: false, position: false, chunks: 0, keepalive: 0 }
let done = false

function finish (code, why) {
  if (done) return
  done = true
  console.log(why)
  process.exit(code)
}

const client = mc.createClient({
  host,
  port,
  username,
  auth: 'offline',
  version: '1.20.4',
  hideErrors: false
})

client.on('login', (packet) => {
  seen.login = true
  console.log('nmp: login  entityId=' + packet.entityId +
    ' dimension=' + packet.worldName +
    ' gameMode=' + packet.gameMode +
    ' viewDistance=' + packet.viewDistance +
    ' isFlat=' + packet.isFlat)
})

client.on('position', (packet) => {
  seen.position = true
  console.log('nmp: position x=' + packet.x + ' y=' + packet.y + ' z=' + packet.z +
    ' teleportId=' + packet.teleportId)
  client.write('teleport_confirm', { teleportId: packet.teleportId })
})

client.on('map_chunk', () => { seen.chunks++ })
client.on('keep_alive', (p) => {
  seen.keepalive++
  client.write('keep_alive', { keepAliveId: p.keepAliveId })
})
client.on('chunk_batch_finished', (p) => {
  console.log('nmp: chunk batch finished, ' + p.batchSize + ' announced, ' +
    seen.chunks + ' received')
  client.write('chunk_batch_received', { chunksPerTick: 16.0 })
})

client.on('kick_disconnect', (p) => finish(1, 'nmp: KICKED ' + JSON.stringify(p)))
client.on('disconnect', (p) => finish(1, 'nmp: DISCONNECTED ' + JSON.stringify(p)))
client.on('error', (e) => finish(1, 'nmp: ERROR ' + e.message))
client.on('end', () => {
  if (!done) finish(seen.login && seen.position ? 0 : 1, 'nmp: connection ended')
})

setInterval(() => {
  if (seen.login && seen.position && seen.chunks > 0 && seen.keepalive > 0) {
    console.log('nmp: chunks=' + seen.chunks + ' keepAlives=' + seen.keepalive)
    finish(0, 'OK nmp: the client is in the world')
  }
}, 200)

setTimeout(() => {
  finish(1, 'nmp: TIMEOUT -- login=' + seen.login + ' position=' + seen.position +
    ' chunks=' + seen.chunks + ' keepalive=' + seen.keepalive)
}, 30000)

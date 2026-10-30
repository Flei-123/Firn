const fs = require('fs')
const mc = require('/root/firn/tools/mcserver/node_modules/minecraft-protocol')
const [host, port, name, outPath, seconds] = process.argv.slice(2)
const frames = []
const c = mc.createClient({ host, port: parseInt(port, 10), username: name, auth: 'offline', version: '1.20.1', hideErrors: true })
const hook = () => {
  const d = c.deserializer
  if (d.__hooked) return
  d.__hooked = true
  const origTransform = d._transform.bind(d)
  d._transform = (chunk, enc, cb) => {
    if (chunk.length > 0 && chunk[0] === 0x10) frames.push(chunk.toString('hex'))
    return origTransform(chunk, enc, cb)
  }
}
c.on('state', hook)
hook()
c.on('error', (e) => console.error('err', e.message.slice(0, 80)))
setTimeout(() => { fs.writeFileSync(outPath, JSON.stringify(frames)); process.exit(0) }, parseInt(seconds || '8', 10) * 1000)

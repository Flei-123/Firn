const net = require('net')
const zlib = require('zlib')
const fs = require('fs')

const host = process.argv[2]
const port = parseInt(process.argv[3], 10)
const protocol = parseInt(process.argv[4], 10)
const out = process.argv[5]
const playLimit = parseInt(process.argv[6] || '40', 10)

function varint(n) {
  const b = []
  let v = n >>> 0
  do {
    let x = v & 127
    v >>>= 7
    if (v) x |= 128
    b.push(x)
  } while (v)
  return Buffer.from(b)
}

function readVarint(buf, at) {
  let r = 0
  let shift = 0
  for (;;) {
    if (at >= buf.length) return null
    const b = buf[at++]
    r |= (b & 127) << shift
    if (!(b & 128)) return [r >>> 0, at]
    shift += 7
  }
}

function str(s) {
  const b = Buffer.from(s, 'utf8')
  return Buffer.concat([varint(b.length), b])
}

let threshold = -1
const sock = net.connect(port, host)
let acc = Buffer.alloc(0)
let state = 'login'
const config = []
const play = []
let done = false

function send(id, body) {
  const payload = Buffer.concat([varint(id), body])
  let frame
  if (threshold < 0) {
    frame = Buffer.concat([varint(payload.length), payload])
  } else if (payload.length < threshold) {
    frame = Buffer.concat([varint(payload.length + 1), Buffer.from([0]), payload])
  } else {
    const z = zlib.deflateSync(payload)
    const head = varint(payload.length)
    frame = Buffer.concat([varint(head.length + z.length), head, z])
  }
  sock.write(frame)
}

function finish() {
  if (done) return
  done = true
  fs.writeFileSync(out, JSON.stringify({ protocol, config, play }))
  sock.destroy()
  process.exit(0)
}

function onPacket(id, body) {
  if (state === 'login') {
    if (id === 3) {
      threshold = readVarint(body, 0)[0]
    } else if (id === 2) {
      send(3, Buffer.alloc(0))
      state = 'config'
    } else if (id === 0) {
      console.error('login disconnect', body.toString('utf8'))
      process.exit(2)
    }
    return
  }
  if (state === 'config') {
    config.push({ id, body: body.toString('hex') })
    if (id === 0x0f) send(7, varint(0))
    else if (id === 0x04) send(4, body)
    else if (id === 0x03) {
      send(3, Buffer.alloc(0))
      state = 'play'
    } else if (id === 0x02) {
      console.error('config disconnect')
      process.exit(2)
    }
    return
  }
  play.push({ id, body: body.toString('hex') })
  if (play.length >= playLimit) finish()
}

sock.on('connect', () => {
  const hs = Buffer.concat([varint(protocol), str(host), Buffer.from([port >> 8, port & 255]), varint(2)])
  send(0, hs)
  const uuid = Buffer.alloc(16, 7)
  send(0, Buffer.concat([str('Capture'), uuid]))
})

sock.on('data', (d) => {
  acc = Buffer.concat([acc, d])
  for (;;) {
    const l = readVarint(acc, 0)
    if (!l || acc.length < l[1] + l[0]) break
    let payload = acc.subarray(l[1], l[1] + l[0])
    acc = acc.subarray(l[1] + l[0])
    if (threshold >= 0) {
      const u = readVarint(payload, 0)
      payload = u[0] === 0 ? payload.subarray(u[1]) : zlib.inflateSync(payload.subarray(u[1]))
    }
    const idr = readVarint(payload, 0)
    onPacket(idr[0], payload.subarray(idr[1]))
  }
})

sock.on('error', (e) => {
  console.error('socket error', e.message)
  finish()
})
sock.on('close', finish)
setTimeout(finish, 20000)

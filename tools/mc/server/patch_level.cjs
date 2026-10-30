'use strict'
const fs = require('fs')
const path = require('path')
const zlib = require('zlib')
const nbt = require(path.join(process.env.MCBOT_MODULES || '/root/firn/tools/mcserver/node_modules', 'prismarine-nbt'))

const file = process.argv[2]
const rules = process.argv.slice(3)
nbt.parse(zlib.gunzipSync(fs.readFileSync(file))).then(({ parsed }) => {
  const data = parsed.value.Data.value
  if (!data.GameRules) data.GameRules = { type: 'compound', value: {} }
  for (const rule of rules) {
    const [name, value] = rule.split('=')
    data.GameRules.value[name] = { type: 'string', value }
  }
  fs.writeFileSync(file, zlib.gzipSync(nbt.writeUncompressed(parsed)))
})

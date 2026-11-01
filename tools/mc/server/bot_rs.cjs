'use strict'
const { startBot } = require('./rs_bot_common.cjs')

startBot((b) => {
  const g = -1
  b.look(0)
  b.put('redstone_lamp', 4, g, 2, 1)
  b.put('redstone', 1, g, 2, 1)
  b.put('redstone', 2, g, 2, 1)
  b.put('redstone', 3, g, 2, 1)
  b.put('lever', 0, g, 2, 1)
  b.look(-90)
  b.put('redstone_lamp', 3, g, 4, 1)
  b.put('redstone', 2, g, 4, 1)
  b.put('repeater', 1, g, 4, 1)
  b.put('lever', 0, g, 4, 1)
  b.look(0)
  b.put('stone', 1, g, -2, 1)
  b.put('stone', 2, g, -2, 1)
  b.put('lever', 1, 0, -2, 4)
  b.put('redstone_torch', 1, 0, -2, 1)
  b.put('redstone_lamp', 2, 0, -2, 1)
  const toggles = [['A', 0, 0, 2], ['A', 0, 0, 2], ['B', 0, 0, 4], ['B', 0, 0, 4], ['C', 0, 0, -2], ['C', 0, 0, -2]]
  toggles.forEach(([n, x, y, z], i) => b.act(`${n}${i % 2}`, 'lever', x, y, z))
})

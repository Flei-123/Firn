'use strict'
const { startBot } = require('./rs_bot_common.cjs')

startBot((b) => {
  const g = -1
  const EAST = -90
  b.look(EAST, 90)
  b.put('lever', -4, g, -3, 1)
  b.put('comparator', -3, g, -3, 1)
  b.put('redstone_lamp', -2, g, -3, 1)
  b.put('lever', -1, g, 3, 1)
  b.put('redstone_lamp', -3, g, 3, 1)
  b.look(EAST, 0)
  b.put('observer', -2, g, 3, 1)
  b.look(EAST, 90)
  b.put('stone_button', 2, g, 3, 1)
  b.put('redstone_lamp', 3, g, 3, 1)
  b.put('oak_button', 2, g, -3, 1)
  b.put('redstone_lamp', 3, g, -3, 1)
  b.act('B0', 'stone_button', 2, 0, 3)
  b.act('B1', 'oak_button', 2, 0, -3)
  b.act('O0', 'lever', -1, 0, 3)
  b.act('O1', 'lever', -1, 0, 3)
  b.act('C0', 'lever', -4, 0, -3)
  b.act('C1', 'redstone_block', -3, g, -4)
  b.act('C2', 'comparator', -3, 0, -3)
  b.act('C3', 'comparator', -3, 0, -3)
}, { gap: 2400 })

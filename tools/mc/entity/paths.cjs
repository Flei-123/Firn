// SPDX-License-Identifier: MPL-2.0
'use strict'

const MAX_PACKED_STEPS = 8
const MAX_PATH_POINTS = 5
const MAX_TICKS = 300
const MAX_PATH_TYPE = 3

function genPackedDelta (ri, pick) {
  const steps = ri(0, MAX_PACKED_STEPS)
  const ground = ri(0, 1)
  const value = (steps << 1) | ground
  const i16 = () => pick([-32768, 32767, 0, 1, -1, ri(-32768, 32767)])
  const delta = {}
  if (steps === 0) {
    delta.dX = i16(); delta.dY = i16(); delta.dZ = i16()
  } else {
    for (let i = 0; i < steps; i++) {
      delta['ticks' + i] = ri(0, MAX_TICKS)
      delta['dX' + i] = i16(); delta['dY' + i] = i16(); delta['dZ' + i] = i16()
    }
  }
  return { packed: value, delta }
}

function genPositionPath (ri, genPos) {
  const point = () => ({ x: genPos('f64', 'x'), y: genPos('f64', 'y'), z: genPos('f64', 'z') })
  const pathType = ri(0, MAX_PATH_TYPE)
  if (pathType === 0) return { pathType, path: point() }
  const path = []
  const count = ri(1, MAX_PATH_POINTS)
  for (let i = 0; i < count; i++) path.push(Object.assign(point(), { ticks: ri(0, MAX_TICKS) }))
  return { pathType, path }
}

module.exports = { genPackedDelta, genPositionPath }

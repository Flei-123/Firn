// SPDX-License-Identifier: MPL-2.0
// ref_entities.cjs -- independent entity tracker for the reference client and the entity cross-check.
//
// Fed with PARSED packets in node-minecraft-protocol form (name + params, the parser is
// node-minecraft-protocol / protodef, never the Firn schema walker) and builds the same canonical
// text as lib/mc/mc_entity.fi `entity_dump` (docs/mc/entity.md). All maths is written here from the
// protocol description and shares no code with the Firn library:
//
//   1.8 (protocol <= 47): positions are int32 fixed point / 32, relative moves int8 / 32
//   1.9+                : positions are f64 blocks, relative moves int16 / 4096
//   angle byte (int8)   : (byte & 255) * 360 / 256 degrees;  float angles (sync_entity_position): as is
//
// Rules (same as the library): a spawn replaces an entity of the same id (on_ground false), move /
// look / teleport / sync of an unknown id are ignored, destroy removes, the player name is copied
// at spawn time from the uuid -> name table that player_info / player_remove keep.
//
//   const t = new EntityTracker(protocol); t.onPacket(name, params); t.dump() -> string
'use strict'

const f64 = new Float64Array(1)
const u64 = new BigUint64Array(f64.buffer)
function bits (v) {
  f64[0] = v
  return u64[0].toString(16).padStart(16, '0')
}

function uuidHex (u) {
  if (typeof u === 'string') return u.replace(/-/g, '').toLowerCase()
  if (Array.isArray(u) && u.length === 4) { // 4 x int32 form
    return u.map((w) => (w >>> 0).toString(16).padStart(8, '0')).join('')
  }
  return null
}

class EntityTracker {
  constructor (protocol) {
    this.fixed = protocol <= 47
    this.ents = new Map()
    this.players = new Map() // uuid hex -> name
    this.count = { spawn: 0, move: 0, teleport: 0, look: 0, destroy: 0, info: 0, miss: 0 }
  }

  pos (v) { return this.fixed ? v / 32 : v }
  delta (v) { return this.fixed ? v / 32 : v / 4096 }
  angle (v) { return ((v & 255) * 360) / 256 }

  spawn (p, kind, uuidKey) {
    const uuid = uuidKey && p[uuidKey] !== undefined ? uuidHex(p[uuidKey]) : null
    const e = {
      id: p.entityId,
      kind,
      type: kind === 2 ? -1 : p.type,
      uuid,
      name: uuid && this.players.has(uuid) ? this.players.get(uuid) : '',
      x: this.pos(p.x),
      y: this.pos(p.y),
      z: this.pos(p.z),
      yaw: p.yaw === undefined ? 0 : this.angle(p.yaw),
      pitch: p.pitch === undefined ? 0 : this.angle(p.pitch),
      ground: false
    }
    this.ents.set(e.id, e)
    this.count.spawn++
  }

  onPacket (name, p) {
    switch (name) {
      case 'spawn_entity': return this.spawn(p, 0, 'objectUUID')
      case 'spawn_entity_living': return this.spawn(p, 1, 'entityUUID')
      case 'named_entity_spawn': return this.spawn(p, 2, 'playerUUID')
      case 'entity_teleport': {
        this.count.teleport++
        const e = this.ents.get(p.entityId)
        if (!e) { this.count.miss++; return }
        e.x = this.pos(p.x); e.y = this.pos(p.y); e.z = this.pos(p.z)
        if (p.v21) { e.yaw = p.yaw; e.pitch = p.pitch } else { e.yaw = this.angle(p.yaw); e.pitch = this.angle(p.pitch) } // v21: float degrees
        e.ground = !!p.onGround
        return
      }
      case 'sync_entity_position': {
        this.count.teleport++
        const e = this.ents.get(p.entityId)
        if (!e) { this.count.miss++; return }
        e.x = p.x; e.y = p.y; e.z = p.z
        e.yaw = p.yaw; e.pitch = p.pitch
        e.ground = !!p.onGround
        return
      }
      case 'rel_entity_move':
      case 'entity_move_look': {
        this.count.move++
        const e = this.ents.get(p.entityId)
        if (!e) { this.count.miss++; return }
        e.x = e.x + this.delta(p.dX); e.y = e.y + this.delta(p.dY); e.z = e.z + this.delta(p.dZ)
        if (name === 'entity_move_look') { e.yaw = this.angle(p.yaw); e.pitch = this.angle(p.pitch) }
        e.ground = !!p.onGround
        return
      }
      case 'entity_look': {
        this.count.look++
        const e = this.ents.get(p.entityId)
        if (!e) { this.count.miss++; return }
        e.yaw = this.angle(p.yaw); e.pitch = this.angle(p.pitch)
        e.ground = !!p.onGround
        return
      }
      case 'entity_destroy':
        this.count.destroy++
        for (const id of p.entityIds) this.ents.delete(id)
        return
      case 'player_remove':
        for (const u of p.players) this.players.delete(uuidHex(u))
        return
      case 'player_info': {
        this.count.info++
        const a = p.action
        if (typeof a === 'string') { // 1.8 - 1.19.2: one action per packet
          for (const d of p.data) {
            const u = uuidHex(d.uuid)
            if (a === 'add_player') this.players.set(u, d.name)
            else if (a === 'remove_player') this.players.delete(u)
          }
        } else if (a && a.add_player) { // 1.19.3+: bit flags; a name only comes with add_player
          for (const d of p.data) {
            if (d.player && d.player.name !== undefined) this.players.set(uuidHex(d.uuid), d.player.name)
          }
        }
        return
      }
      default:
    }
  }

  dump () {
    const ids = [...this.ents.keys()].sort((a, b) => a - b)
    let s = `entities ${ids.length}\n`
    for (const id of ids) {
      const e = this.ents.get(id)
      s += `E ${id} kind=${e.kind} type=${e.type} uuid=${e.uuid || '-'} name=${e.name || '-'}` +
        ` x=${bits(e.x)} y=${bits(e.y)} z=${bits(e.z)} yaw=${bits(e.yaw)} pitch=${bits(e.pitch)} ground=${e.ground ? 1 : 0}\n`
    }
    return s
  }
}

module.exports = { EntityTracker }

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""gen_registry.py -- minecraft-data blocks / items / entities / biomes -> a Firn module.

usage: gen_registry.py <minecraft-data-data-dir> <version> <out.fi>

Emits lib/mc/vNNN/mc_reg_vNNN.fi: one string literal blob in the format documented at the top
of lib/mc/mc_registry.fi (which also holds every reader) plus `registry_make()`.
Version = data: a new game version is this script run with another version string.
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
NO_HARDNESS = 0xFFFFFFF
UNBREAKABLE = frozenset(["bedrock", "barrier", "light", "command_block", "chain_command_block", "repeating_command_block", "structure_block", "jigsaw",
                         "end_portal", "end_portal_frame", "end_gateway", "nether_portal", "moving_piston"])


def num(v):
    assert 0 <= v < (1 << 28), v
    return bytes([(v >> 21) & 0x7F, (v >> 14) & 0x7F, (v >> 7) & 0x7F, v & 0x7F])


class Pool:
    def __init__(self):
        self.buf = bytearray(b"\0")  # offset 0 = the empty string
        self.idx = {"": 0}

    def add(self, s):
        if s not in self.idx:
            self.idx[s] = len(self.buf)
            self.buf += s.encode("ascii") + b"\0"
        return self.idx[s]


def lit_string(blob):
    parts = []
    for b in blob:
        assert b < 0x80
        c = chr(b)
        if c == '"':
            parts.append('\\"')
        elif c == "\\":
            parts.append("\\\\")
        elif 0x20 <= b < 0x7F and c != "{":
            parts.append(c)
        else:
            parts.append("\\x%02x" % b)
    return "".join(parts)


def load(data_dir, version):
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    vt = json.load(open(os.path.join(HERE, "versions.json")))["versions"]
    key = vt[version]["data"] if version in vt else version
    P = paths[key]

    def get(what):
        return json.load(open(os.path.join(data_dir, P[what], what + ".json")))
    ver = json.load(open(os.path.join(data_dir, P["version"], "version.json")))
    return get("blocks"), get("items"), get("entities"), get("biomes"), ver["version"], key


def build(blocks, items, entities, biomes):
    pool = Pool()
    for b in blocks:
        if "minStateId" not in b:
            # PRE-FLATTENING versions (1.12.2 and older): there are no state ids, a block
            # state on the wire is `id << 4 | metadata`. The registry models that as 16
            # states per block id without properties (docs/MC-VERSIONS.md, "block ids").
            b["minStateId"] = b["id"] * 16
            b["maxStateId"] = b["id"] * 16 + 15
            b["defaultState"] = b["id"] * 16
            b["states"] = []
    blocks = sorted(blocks, key=lambda b: b["minStateId"])
    nstates = max(b["maxStateId"] for b in blocks) + 1
    # property lists
    props_blob = bytearray()  # offsets are relative until the final layout is known
    prop_pos = {}
    prop_chunks = []

    def prop_list(b):
        st = b.get("states") or []
        if not st:
            return None
        return st

    # first pass: strings + sizes so that offsets can be computed
    block_recs = []
    prop_recs = []  # (block index, bytes builder)
    for b in blocks:
        pool.add(b["name"])
        for s in (b.get("states") or []):
            pool.add(s["name"])
            for v in (s.get("values") or []):
                pool.add(v)
    for it in items:
        pool.add(it["name"])
    for e in entities:
        pool.add(e["name"])
        pool.add(e.get("type") or "")
    for bi in biomes:
        pool.add(bi["name"])
        pool.add(bi.get("category") or "")
    pool_bytes = bytes(pool.buf)
    # layout: "FRG1" + header(9 numbers) + pool + blocks + props + items + entities + biomes
    hdr_len = 4 + 9 * 4
    pool_off = hdr_len
    blocks_off = pool_off + len(pool_bytes)
    props_off = blocks_off + len(blocks) * 32
    # properties
    props = bytearray()
    prop_off_of = {}
    for i, b in enumerate(blocks):
        st = prop_list(b)
        if not st:
            prop_off_of[i] = 0
            continue
        prop_off_of[i] = props_off + len(props)
        # the values arrays come after the property table of this block
        table_len = 4 + len(st) * 16
        values_at = props_off + len(props) + table_len
        tab = bytearray(num(len(st)))
        vals = bytearray()
        for s in st:
            t = {"bool": 0, "enum": 1, "int": 2}[s["type"]]
            nv = s["num_values"]
            vo = 0
            if s.get("values"):  # enums AND ints that carry explicit value names (leaves: 1..7)
                assert len(s["values"]) == nv
                vo = values_at + len(vals)
                for v in s["values"]:
                    vals += num(pool_off + pool.idx[v])
            tab += num(pool_off + pool.idx[s["name"]]) + num(t) + num(nv) + num(vo)
        props += tab + vals
    items_off = props_off + len(props)
    max_item = max(it["id"] for it in items)
    ent_off = items_off + (max_item + 1) * 8
    # entities: dense records sorted by (id, kind, name) -- in the pre-flattening versions
    # objects and mobs have separate id spaces, so an id is not unique there
    entities = sorted(entities, key=lambda e: (e["id"], e.get("type") or "", e["name"]))
    biomes = sorted(biomes, key=lambda b: (b["id"], b["name"]))  # 1.8 data has id 161 twice
    bio_off = ent_off + len(entities) * 20

    out = bytearray(b"FRG1")
    for v in (len(blocks), nstates, blocks_off, max_item + 1, items_off, len(entities), ent_off, len(biomes), bio_off):
        out += num(v)
    out += pool_bytes
    assert len(out) == blocks_off
    for i, b in enumerate(blocks):
        flags = 0
        if b.get("boundingBox") == "block":
            flags |= 1
        if b.get("transparent"):
            flags |= 2
        if b.get("diggable"):
            flags |= 4
        flags |= (b.get("filterLight", 0) & 15) << 4
        flags |= (b.get("emitLight", 0) & 15) << 8
        h = b.get("hardness")
        if h == 0 and b["name"] in UNBREAKABLE:
            h = None  # minecraft-data 1.16.x lists these as hardness 0 (instant); the game says -1 (unbreakable)
        hm = NO_HARDNESS if (h is None or h < 0) else int(math.floor(h * 1000 + 0.5))
        assert hm <= NO_HARDNESS
        for v in (pool_off + pool.idx[b["name"]], b["minStateId"], b["maxStateId"], b["defaultState"],
                  prop_off_of[i], flags, hm, b.get("stackSize", 64)):
            out += num(v)
    out += props
    recs = bytearray(8 * (max_item + 1))
    for it in items:
        recs[it["id"] * 8:it["id"] * 8 + 8] = num(pool_off + pool.idx[it["name"]]) + num(it.get("stackSize", 64))
    out += recs
    recs = bytearray()
    for e in entities:
        w = int(math.floor((e.get("width") or 0) * 1000 + 0.5))
        h = int(math.floor((e.get("height") or 0) * 1000 + 0.5))
        recs += num(pool_off + pool.idx[e["name"]]) + num(w) + num(h) + num(pool_off + pool.idx[e.get("type") or ""]) + num(e["id"])
    out += recs
    recs = bytearray()
    for bi in biomes:
        recs += num(pool_off + pool.idx[bi["name"]]) + num(pool_off + pool.idx[bi.get("category") or ""]) + num(bi.get("color") or 0) + num(bi["id"])
    out += recs
    return bytes(out), len(blocks), nstates, len(items), len(entities), len(biomes)


def generate(data_dir, version, out_path):
    blocks, items, entities, biomes, pver, key = load(data_dir, version)
    blob, nb, ns, ni, ne, nbi = build(blocks, items, entities, biomes)
    L = []
    w = L.append
    w("// SPDX-License-Identifier: MPL-2.0")
    w("// GENERATED by tools/mcgen/gen_registry.py - do not edit")
    w("//")
    w("// lib/mc/v%d/mc_reg_v%d.fi -- registries of Minecraft Java %s (protocol %d):" % (pver, pver, version, pver))
    w("// %d blocks, %d block states, %d items, %d entities, %d biomes (%d octets)." % (nb, ns, ni, ne, nbi, len(blob)))
    w("// Source: minecraft-data (PrismarineJS, open source) data/%s. The reader is lib/mc/mc_registry.fi." % key)
    w("")
    w("import mc.mc_registry")
    w("")
    w('const REG_BLOB: str = "%s"' % lit_string(blob))
    w("")
    w("fn registry_make() -> mc_registry.Registry {")
    w("    return mc_registry.registry_open(REG_BLOB.p as u64)")
    w("}")
    with open(out_path, "w") as fh:
        fh.write("\n".join(L) + "\n")
    sys.stderr.write("gen_registry: %s protocol %d: %d blocks %d states %d items %d entities %d biomes, blob %d octets\n" % (
        version, pver, nb, ns, ni, ne, nbi, len(blob)))
    return pver


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    generate(*sys.argv[1:4])

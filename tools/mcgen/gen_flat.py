#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""gen_flat.py -- flattening table: 1.12.2 `id << 4 | meta` -> 1.13+ block state id.

usage: gen_flat.py <minecraft-data-data-dir> <out.fi> [--check]

Inputs
  * tools/mcgen/legacy_flat.json   hand written rules (legacy block name -> modern name + properties
                                   as a function of the metadata nibble), see its `_comment`
  * minecraft-data pc/1.12 blocks.json   legacy block ids and names
  * minecraft-data pc/1.20 blocks.json   modern blocks (1.20.1), their properties and state id ranges
Output
  lib/mc/v340/mc_flat_v340.fi: a blob of 4096 entries, entry i = the 1.20.1 global block state id
  of legacy state i. Reader: lib/mc/mc_flat.fi.

Rules for the mapping (the JS cross-check tools/mc/flat/ref_flat.cjs implements the same text
independently and must produce the same 4096 numbers):
  * legacy id with no block in minecraft-data  -> state 0 (air)
  * block without a rule                       -> the modern block of the same name, default state
  * 'to' / 'names'[(meta >> shift) & mask] / 'by_meta'[meta] pick the modern block
  * every entry of 'props' sets one property from ((meta >> shift) & mask) via 'map' (null = keep
    the default) or 'offset' (value = bits + offset); 'set' fixes values
  * UNKNOWN META COMBINATION (the index runs beyond 'names' / 'by_meta' / 'map', or the value is not
    a legal value of the modern property): the entry falls back to the result for meta 0 of the same
    block id (lenient: an unusable part of meta 0 itself keeps the default / the first name).
  * state id = min_state_id + sum(index(property_i) * stride_i), the FIRST listed property is the most
    significant, bool value order is [true, false] (as the data lists it implicitly).
The generator stops on every target block or property that does not exist in the modern data.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MAGIC = b"FLT1"


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


def value_text(v):
    if v is True:
        return "true"
    if v is False:
        return "false"
    return str(v)


ERRORS = []


class Modern:
    def __init__(self, blocks):
        self.by_name = {b["name"]: b for b in blocks}

    def block(self, name):
        if name not in self.by_name:
            ERRORS.append("modern block '%s' does not exist" % name)
            return None
        return self.by_name[name]

    @staticmethod
    def prop_values(p):
        if p["type"] == "bool":
            return ["true", "false"]
        return list(p["values"])

    def state(self, name, values):
        """values: property name -> text value (missing = default). Returns (state id, ok)."""
        b = self.block(name)
        if b is None:
            return 0
        props = b.get("states", [])
        by = {p["name"]: p for p in props}
        for k in values:
            if k not in by:
                ERRORS.append("block '%s' has no property '%s'" % (name, k))
                return 0
        # default indexes: decode the default state
        rem = b["defaultState"] - b["minStateId"]
        strides = []
        s = 1
        for p in reversed(props):
            strides.append(s)
            s *= p["num_values"]
        strides.reverse()
        idx = {}
        for p, st in zip(props, strides):
            idx[p["name"]] = (rem // st) % p["num_values"]
        for k, v in values.items():
            vals = self.prop_values(by[k])
            if v not in vals:
                return None
            idx[k] = vals.index(v)
        return b["minStateId"] + sum(idx[p["name"]] * st for p, st in zip(props, strides))


def evaluate(mod, rule, name, meta, lenient):
    """One legacy (block, meta). Returns the modern state id, or None for an unknown combination."""
    if rule is None:
        return mod.state(name, {})
    if "to" in rule:
        target = rule["to"]
    elif "by_meta" in rule:
        target = rule["by_meta"][meta]
    elif "names" in rule:
        i = (meta >> rule.get("shift", 0)) & rule["mask"]
        if i >= len(rule["names"]):
            if not lenient:
                return None
            i = 0
        target = rule["names"][i]
    else:
        target = name
    values = {}
    for k, v in rule.get("set", {}).items():
        values[k] = value_text(v)
    for k, p in rule.get("props", {}).items():
        bits = (meta >> p.get("shift", 0)) & p["mask"]
        if "map" in p:
            if bits >= len(p["map"]):
                if not lenient:
                    return None
                continue
            v = p["map"][bits]
            if v is None:
                continue
        else:
            v = bits + p.get("offset", 0)  # neither map nor offset: the bits are the value
        values[k] = value_text(v)
    if "to" not in rule:
        # a rule with several targets lists properties that only some of them have (torch: wall_torch
        # has `facing`, torch has not): those are skipped for the targets without them
        tb = mod.block(target)
        have = {p["name"] for p in tb.get("states", [])} if tb else set()
        values = {k: v for k, v in values.items() if k in have}
    st = mod.state(target, values)
    if st is None:
        if not lenient:
            return None
        # drop the illegal values one by one (meta 0 only)
        ok = {}
        for k, v in values.items():
            if mod.state(target, {k: v}) is not None:
                ok[k] = v
        st = mod.state(target, ok)
    return st


def check_rules(mod, rules):
    """Every property of a rule must exist on at least one of the rule's targets."""
    for name, rule in rules.items():
        targets = set([rule["to"]] if "to" in rule else rule.get("by_meta", []) + rule.get("names", []))
        if not targets:
            targets = {name}
        have = set()
        for t in targets:
            b = mod.block(t)
            if b:
                have |= {p["name"] for p in b.get("states", [])}
        for k in list(rule.get("props", {})) + list(rule.get("set", {})):
            if k not in have:
                ERRORS.append("rule '%s': property '%s' exists on none of its targets" % (name, k))


def build(data_dir):
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    old = json.load(open(os.path.join(data_dir, paths["1.12.2"]["blocks"], "blocks.json")))
    new = json.load(open(os.path.join(data_dir, paths["1.20.1"]["blocks"], "blocks.json")))
    rules = json.load(open(os.path.join(HERE, "legacy_flat.json")))["rules"]
    mod = Modern(new)
    check_rules(mod, rules)
    table = [0] * 4096
    fallbacks = 0
    for b in old:
        bid = b["id"]
        if not 0 <= bid < 256:
            sys.exit("gen_flat: legacy id %d out of range" % bid)
        rule = rules.get(b["name"])
        if b["name"] not in rules:
            mod.block(b["name"])  # must exist
        base = evaluate(mod, rule, b["name"], 0, True)
        for meta in range(16):
            st = evaluate(mod, rule, b["name"], meta, False)
            if st is None:
                st = base
                fallbacks += 1
            table[(bid << 4) | meta] = st
    if ERRORS:
        for e in sorted(set(ERRORS)):
            print("gen_flat: " + e, file=sys.stderr)
        sys.exit(1)
    return table, fallbacks, len(old)


def generate(data_dir, out_path):
    table, fallbacks, nold = build(data_dir)
    assert max(table) < 32768
    blob = bytearray(MAGIC)
    for v in table:  # 3 octets of 7 bits, least significant first: the blob stays ASCII
        blob += bytes([v & 0x7F, (v >> 7) & 0x7F, v >> 14])
    lines = [
        "// SPDX-License-Identifier: MPL-2.0",
        "// GENERATED by tools/mcgen/gen_flat.py - do not edit",
        "//",
        "// lib/mc/v340/mc_flat_v340.fi -- flattening table of Minecraft Java 1.12.2 (protocol 340):",
        "// 4096 legacy states (id << 4 | meta) -> 1.20.1 global block state ids (%d legacy blocks,"
        % nold,
        "// %d unknown meta combinations fell back to meta 0). Rules: tools/mcgen/legacy_flat.json." % fallbacks,
        "// The reader is lib/mc/mc_flat.fi.",
        "",
        "import mc.mc_flat",
        "",
        'const FLAT_BLOB: str = "%s"' % lit_string(blob),
        "",
        "fn flat_make() -> mc_flat.Flat {",
        "    return mc_flat.flat_open(FLAT_BLOB.p as u64)",
        "}",
        "",
    ]
    with open(out_path, "w") as f:
        f.write("\n".join(lines))
    return table, fallbacks


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    if len(args) != 2:
        sys.exit(__doc__)
    table, fallbacks = generate(args[0], args[1])
    if "--table" in argv:
        print(json.dumps(table))
    print("gen_flat: %s: 4096 states, max state id %d, %d meta fallbacks" % (args[1], max(table), fallbacks),
          file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv)

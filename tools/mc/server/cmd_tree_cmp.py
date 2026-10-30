import json
import struct
import sys

PARSER_PROPS = {
    "brigadier:bool": "", "brigadier:float": "f4", "brigadier:double": "f8", "brigadier:integer": "i4",
    "brigadier:long": "i8", "brigadier:string": "v", "minecraft:entity": "b", "minecraft:score_holder": "b",
    "minecraft:range": "b", "minecraft:time": "t4", "minecraft:resource_or_tag": "s", "minecraft:resource_or_tag_key": "s",
    "minecraft:resource": "s", "minecraft:resource_key": "s",
}


class Reader:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def byte(self):
        v = self.data[self.pos]
        self.pos += 1
        return v

    def raw(self, n):
        v = self.data[self.pos:self.pos + n]
        if len(v) != n:
            raise ValueError("short read")
        self.pos += n
        return v

    def varint(self):
        v = 0
        shift = 0
        while True:
            b = self.byte()
            v |= (b & 127) << shift
            if not b & 128:
                return v
            shift += 7

    def string(self):
        return self.raw(self.varint()).decode()


def read_props(r, kind):
    if kind == "":
        return ""
    if kind == "b" or kind == "v":
        return "%d" % (r.byte() if kind == "b" else r.varint())
    if kind == "s":
        return r.string()
    if kind == "t4":
        return "min=%d" % struct.unpack(">i", r.raw(4))[0]
    if kind == "i4" or kind == "i8":
        return "int" + str(read_bounds(r, 4 if kind == "i4" else 8, "i"))
    flags = r.byte()
    return "real" + str(read_bounds_flags(r, flags, 4 if kind == "f4" else 8, "f"))


def read_bounds(r, width, fmt):
    return read_bounds_flags(r, r.byte(), width, fmt)


def read_bounds_flags(r, flags, width, fmt):
    out = [flags]
    code = {("i", 4): ">i", ("i", 8): ">q", ("f", 4): ">f", ("f", 8): ">d"}[(fmt, width)]
    for bit in (1, 2):
        if flags & bit:
            out.append(struct.unpack(code, r.raw(width))[0])
    return out


def parse_frame(hexstr, parsers):
    r = Reader(bytes.fromhex(hexstr))
    packet_id = r.varint()
    count = r.varint()
    nodes = []
    for _ in range(count):
        flags = r.byte()
        children = [r.varint() for _ in range(r.varint())]
        redirect = r.varint() if flags & 8 else None
        node = {"flags": flags, "children": children, "redirect": redirect, "name": None, "parser": None, "props": None}
        kind = flags & 3
        if kind in (1, 2):
            node["name"] = r.string()
        if kind == 2:
            pid = r.varint()
            node["parser"] = parsers[pid]
            node["props"] = read_props(r, PARSER_PROPS.get(node["parser"], ""))
            if flags & 16:
                node["props"] += " suggest=" + r.string()
        nodes.append(node)
    root = r.varint()
    if r.pos != len(r.data):
        raise ValueError("trailing bytes: %d" % (len(r.data) - r.pos))
    return nodes, root, packet_id


def canonical_paths(nodes, root):
    paths = {root: ""}
    queue = [root]
    while queue:
        idx = queue.pop(0)
        for c in sorted(nodes[idx]["children"], key=lambda k: nodes[k]["name"]):
            if c not in paths:
                paths[c] = paths[idx] + "/" + nodes[c]["name"]
                queue.append(c)
    return paths


def dump(nodes, root, wanted):
    canon = canonical_paths(nodes, root)
    lines = []

    def walk(idx, path):
        n = nodes[idx]
        redirect = ""
        if n["redirect"] is not None:
            redirect = " -> " + canon.get(n["redirect"], "?")
        kids = ",".join(sorted(nodes[c]["name"] for c in n["children"]))
        lines.append("%s flags=%02x parser=%s props=%s kids=[%s]%s" % (path, n["flags"], n["parser"], n["props"], kids, redirect))
        for c in n["children"]:
            walk(c, path + "/" + nodes[c]["name"])

    for c in nodes[root]["children"]:
        if nodes[c]["name"] in wanted:
            walk(c, "/" + nodes[c]["name"])
    return sorted(lines)


def main():
    van_path, fir_path, registries_path, names = sys.argv[1:5]
    reg = json.load(open(registries_path))["minecraft:command_argument_type"]["entries"]
    parsers = {v["protocol_id"]: k for k, v in reg.items()}
    wanted = set(names.split(","))
    van = json.load(open(van_path))[-1]
    fir = json.load(open(fir_path))[-1]
    van_lines = dump(*parse_frame(van, parsers)[:2], wanted)
    fir_lines = dump(*parse_frame(fir, parsers)[:2], wanted)
    only_v = [l for l in van_lines if l not in fir_lines]
    only_f = [l for l in fir_lines if l not in van_lines]
    print("nodes vanilla=%d firn=%d diffs=%d" % (len(van_lines), len(fir_lines), len(only_v) + len(only_f)))
    for l in only_v:
        print("  V " + l)
    for l in only_f:
        print("  F " + l)
    sys.exit(1 if only_v or only_f or not van_lines else 0)


main()

# SPDX-License-Identifier: MPL-2.0
import copy
import json

STATES = {"configuration": "configuration", "login": "login", "play": "play", "status": "status", "handshake": "handshaking"}
DIRECTIONS = {"clientbound": "toClient", "serverbound": "toServer"}


def packet_table(proto, state, direction):
    types = proto[state][direction]["types"]
    fields = types["packet"][1]
    mapper = fields[0]["type"][1]["mappings"]
    switch = fields[1]["type"][1]["fields"]
    by_id = {int(k, 16): v for k, v in mapper.items()}
    return by_id, switch


def vanilla_to_md(base_proto, base_packets):
    result = {}
    for vstate, mstate in STATES.items():
        for vdir, mdir in DIRECTIONS.items():
            if vstate not in base_packets or vdir not in base_packets[vstate]:
                continue
            by_id, _ = packet_table(base_proto, mstate, mdir)
            names = {}
            for vname, info in base_packets[vstate][vdir].items():
                md = by_id.get(info["protocol_id"])
                if md is not None:
                    names[vname] = md
            result[(vstate, vdir)] = names
    return result


def apply_delta(base_proto, base_packets, new_packets, delta):
    proto = copy.deepcopy(base_proto)
    v2m = vanilla_to_md(base_proto, base_packets)
    report = {"added": [], "removed": [], "unnamed": []}
    for vstate, mstate in STATES.items():
        for vdir, mdir in DIRECTIONS.items():
            if vstate not in new_packets or vdir not in new_packets[vstate]:
                continue
            _, base_switch = packet_table(base_proto, mstate, mdir)
            names = v2m.get((vstate, vdir), {})
            added = delta.get("packets", {}).get(mstate + "." + mdir, {})
            mappings = {}
            switch = {}
            for vname, info in new_packets[vstate][vdir].items():
                md = names.get(vname)
                if md is None:
                    spec = added.get(vname)
                    if spec is None:
                        report["unnamed"].append(mstate + "." + mdir + " " + vname)
                        continue
                    md = spec["name"]
                    type_name = spec.get("type", "packet_" + md)
                    proto[mstate][mdir]["types"][type_name] = spec["layout"]
                    report["added"].append(mstate + "." + mdir + " " + md)
                else:
                    type_name = base_switch[md]
                mappings["0x%02x" % info["protocol_id"]] = md
                switch[md] = type_name
            for vname, md in names.items():
                if vname not in new_packets[vstate][vdir]:
                    report["removed"].append(mstate + "." + mdir + " " + md)
            fields = proto[mstate][mdir]["types"]["packet"][1]
            fields[0]["type"][1]["mappings"] = mappings
            fields[1]["type"][1]["fields"] = switch
    for type_name, layout in delta.get("layouts", {}).items():
        state, direction, name = type_name.split(".", 2)
        proto[state][direction]["types"][name] = layout
    for patch in delta.get("patches", []):
        state, direction, name = patch["type"].split(".", 2)
        fields = proto[state][direction]["types"][name][1]
        names = [f["name"] for f in fields]
        if patch["op"] == "insert_before":
            fields.insert(names.index(patch["before"]), patch["field"])
        elif patch["op"] == "insert_after":
            fields.insert(names.index(patch["after"]) + 1, patch["field"])
        elif patch["op"] == "remove":
            del fields[names.index(patch["name"])]
        elif patch["op"] == "set_type":
            fields[names.index(patch["name"])]["type"] = patch["field_type"]
        else:
            raise SystemExit("unknown patch op " + patch["op"])
    for type_name, layout in delta.get("shared_types", {}).items():
        proto["types"][type_name] = layout
    return proto, report

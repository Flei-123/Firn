import bisect
import json
import re

from elf import Elf
from lines import parse_line_section, ranges_of
from functions import read_functions


def empty():
    return {"version": 1, "files": {}, "functions": {}, "runs": []}


def peak(costs, addrs, start, end):
    index = bisect.bisect_left(addrs, start)
    best = 0
    while index < len(addrs) and addrs[index] < end:
        best = max(best, costs[addrs[index]])
        index += 1
    return best


def build(exe, costs, name, status):
    elf = Elf(exe)
    units = parse_line_section(elf.section(".debug_line"))
    if not units:
        raise ValueError(f"{exe}: no line table (was it built without debug information?)")
    ranges = []
    for unit in units:
        ranges.extend(ranges_of(unit))
    ranges.sort()
    starts = [item[0] for item in ranges]
    addrs = sorted(costs)
    profile = empty()
    for start, end, path, line in ranges:
        table = profile["files"].setdefault(path, {})
        table[line] = max(table.get(line, 0), peak(costs, addrs, start, end))
    for item in functions_of(exe, elf, ranges):
        index = bisect.bisect_right(starts, item["low"]) - 1
        if index < 0 or ranges[index][1] <= item["low"]:
            continue
        path = ranges[index][2]
        line = item["line"] or ranges[index][3]
        key = f"{path}:{line}:{item['name']}"
        entry = profile["functions"].setdefault(key, {"file": path, "line": line, "name": item["name"], "hits": 0})
        entry["hits"] += costs.get(item["low"], 0) or min(1, peak(costs, addrs, item["low"], item["high"]))
    profile["runs"].append({"name": name, "status": status})
    return profile


def merge(target, other):
    for path, table in other["files"].items():
        into = target["files"].setdefault(path, {})
        for line, hits in table.items():
            into[line] = into.get(line, 0) + hits
    for key, entry in other["functions"].items():
        into = target["functions"].setdefault(key, dict(entry, hits=0))
        into["hits"] += entry["hits"]
    target["runs"].extend(other["runs"])
    return target


def save(profile, path):
    out = dict(profile)
    out["files"] = {p: {str(l): h for l, h in sorted(t.items())} for p, t in sorted(profile["files"].items())}
    with open(path, "w") as handle:
        json.dump(out, handle, indent=1)
        handle.write("\n")


def load(path):
    with open(path) as handle:
        raw = json.load(handle)
    if raw.get("version") != 1:
        raise ValueError(f"{path}: unknown profile version")
    raw["files"] = {p: {int(l): h for l, h in t.items()} for p, t in raw["files"].items()}
    return raw


def functions_of(exe, elf, ranges):
    found = read_functions(exe)
    if found:
        return found
    symbols = [item for item in elf.function_symbols() if not item[1].startswith("_start")]
    if not ranges:
        return []
    text_end = max(item[1] for item in ranges)
    out = []
    for index, (low, label) in enumerate(symbols):
        high = symbols[index + 1][0] if index + 1 < len(symbols) else text_end
        out.append({"name": re.sub(r"^_F\d+\.", "", label), "line": 0, "low": low, "high": high})
    return out

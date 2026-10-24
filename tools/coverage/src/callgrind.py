import glob
import re

POSITION = re.compile(r"^(0x[0-9a-fA-F]+)\s+(?:\S+\s+)?(\d+)\s*$")


def read_costs(path, costs):
    skip_next = False
    with open(path, errors="replace") as handle:
        for row in handle:
            if skip_next:
                skip_next = False
                continue
            if row.startswith("calls="):
                skip_next = True
                continue
            match = POSITION.match(row)
            if match:
                addr = int(match.group(1), 16)
                costs[addr] = costs.get(addr, 0) + int(match.group(2))
    return costs


def read_all(pattern):
    costs = {}
    for path in sorted(glob.glob(pattern)):
        read_costs(path, costs)
    return costs

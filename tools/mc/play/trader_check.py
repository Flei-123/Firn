import re
import sys


def parse_log(line):
    def num(key):
        m = re.search(key + r"=(-?\d+)", line)
        return int(m.group(1)) if m else None

    return num("n"), num("base0"), num("cost0")


def parse_inventory(text):
    totals = {}
    for m in re.finditer(r"\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}", text):
        entry = m.group(0)
        ident = re.search(r'id: ?"minecraft:(\w+)"', entry)
        count = re.search(r"(?:Count|count): (\d+)", entry)
        if ident and count:
            totals[ident.group(1)] = totals.get(ident.group(1), 0) + int(count.group(1))
    return totals


def main():
    log_line, inventory = sys.argv[1], sys.argv[2]
    start = int(sys.argv[3])
    count, base, cost = parse_log(log_line)
    print("  trader: n=%s base0=%s cost0=%s" % (count, base, cost))
    if count != 1 or base is None or cost is None or not 0 < cost < base:
        sys.exit(1)
    totals = parse_inventory(inventory)
    emeralds = totals.get("emerald", 0)
    pickaxes = totals.get("iron_pickaxe", 0)
    print("  server trade: %d emeralds + %d pickaxes x cost %d (start %d)" % (emeralds, pickaxes, cost, start))
    sys.exit(0 if pickaxes >= 1 and emeralds + pickaxes * cost == start else 1)


main()

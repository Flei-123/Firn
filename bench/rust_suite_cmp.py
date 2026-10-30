#!/usr/bin/env python3
import json
import sys


def main():
    old = json.load(open(sys.argv[1]))["pairs"]
    new = json.load(open(sys.argv[2]))["pairs"]
    key = sys.argv[3] if len(sys.argv) > 3 else "fast_instr"
    print("%-14s %16s %16s %9s" % ("pair", "old", "new", "change"))
    for n in new:
        a = old.get(n, {}).get(key)
        b = new[n].get(key)
        if a and b:
            print("%-14s %16d %16d %+8.2f%%" % (n, a, b, (b / a - 1) * 100))

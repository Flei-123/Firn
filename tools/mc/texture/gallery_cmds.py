#!/usr/bin/env python3
import json
import sys

COLS = 12
ROWS = 5
SHEET_STRIDE = 64
SKIP = {"air", "cave_air", "void_air", "water", "lava", "bubble_column", "moving_piston", "piston_head", "barrier", "structure_void", "light", "nether_portal", "end_portal", "end_gateway"}
SKIP_PARTS = ("wall_banner", "wall_sign", "wall_hanging_sign", "wall_torch", "wall_head", "wall_skull", "wall_fan", "redstone_wall_torch", "soul_wall_torch")


def block_names(blocks_json):
    names = [b["name"] for b in json.load(open(blocks_json))]
    return [n for n in names if n not in SKIP and not any(p in n for p in SKIP_PARTS)]


def main():
    names = block_names(sys.argv[1])
    out_cmds = open(sys.argv[2], "w")
    out_map = open(sys.argv[3], "w")
    per_sheet = COLS * ROWS
    sheets = (len(names) + per_sheet - 1) // per_sheet
    for s in range(sheets):
        x0 = SHEET_STRIDE * s
        z0 = 0
        out_cmds.write(f"forceload add {x0 - 8} {z0 - 8} {x0 + 40} {z0 + 48}\n")
    for s in range(sheets):
        x0 = SHEET_STRIDE * s
        z0 = 0
        px = x0 + COLS // 2
        pz = z0 + ROWS * 2 + 4
        out_cmds.write(f"fill {px} -61 {pz} {px} -53 {pz} stone\n")
        for i, name in enumerate(names[s * per_sheet:(s + 1) * per_sheet]):
            col = i % COLS
            row = i // COLS
            out_cmds.write(f"setblock {x0 + col} -60 {z0 + row * 2} minecraft:{name}\n")
            out_map.write(f"{s} {row} {col} {name}\n")
        out_map.write(f"CAM {s} {px} -52 {pz} {x0 + COLS // 2} -60 {z0 + ROWS}\n")
    print(sheets)


main()

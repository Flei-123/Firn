#!/usr/bin/env python3
"""mkpack.py <out.zip> <tile> <variant> -- deterministic FIXTURE resource pack (own patterns, nothing from any game).
variant 0: flat colour per block name, variant 1: checker pattern (different look). Same inputs -> identical bytes."""
import sys, zipfile, zlib, struct

def png(w, h, px):
    raw = b"".join(b"\x00" + bytes(px[y * w * 4:(y + 1) * w * 4]) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")

NAMES = """stone dirt grass_block_side grass_block_top sand gravel cobblestone oak_planks oak_log oak_log_top bedrock sandstone sandstone_top
sandstone_bottom coal_ore iron_ore gold_ore diamond_ore water_still lava_still oak_leaves spruce_log spruce_log_top spruce_planks
birch_log birch_log_top birch_planks glass snow clay netherrack granite diorite andesite deepslate deepslate_top podzol_top podzol_side
mycelium_top mycelium_side coarse_dirt red_sand terracotta white_wool stone_bricks cracked_stone_bricks mossy_cobblestone obsidian
spruce_leaves birch_leaves jungle_leaves acacia_leaves dark_oak_leaves ice packed_ice snow_block calcite tuff dripstone_block
moss_block sculk deepslate_coal_ore copper_ore redstone_ore lapis_ore emerald_ore""".split()

def tile(name, size, variant):
    h = zlib.crc32(name.encode()) & 0xFFFFFF
    r, g, b = 40 + (h & 0xFF) % 200, 40 + ((h >> 8) & 0xFF) % 200, 40 + ((h >> 16) & 0xFF) % 200
    if variant == 1:
        r, g, b = 255 - r, 255 - g, 255 - b
    px = bytearray()
    for y in range(size):
        for x in range(size):
            k = ((x * 4 // size) + (y * 4 // size)) & 1 if variant == 1 else 0
            d = 28 if k else 0
            px += bytes((max(0, r - d), max(0, g - d), max(0, b - d), 255))
    return png(size, size, px)

def main():
    out, size, variant = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        def add(n, d):
            i = zipfile.ZipInfo(n, (1980, 1, 1, 0, 0, 0)); i.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(i, d)
        add("pack.mcmeta", b'{"pack":{"pack_format":15,"description":"mcview fixture"}}')
        for n in NAMES:
            add("assets/minecraft/textures/block/%s.png" % n, tile(n, size, variant))

main()

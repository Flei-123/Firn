import random
import sys

seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
rnd = random.Random(seed)
out = []


def add(spec, s):
    out.append(f"{spec}\t{s.replace(chr(92), chr(92) * 2).replace(chr(10), chr(92) + 'n').replace(chr(9), chr(92) + 't')}")


mods = ["", " ", "  rest", " 5", "x", " ~", "\u00e4", "\U0001F600"]
gm = ["survival", "creative", "adventure", "spectator", "s", "c", "a", "sp", "SURVIVAL", "Creative", "0", "1", "2", "3", "", "x", "surv", "survival1", "survival rest", "\u00e4", "\u0130", "-1", "creative ", "spectator\u00e4", "'survival'", "\"creative\""]
for g in gm:
    for m in ["", " x"]:
        add("gamemode", g + m)

nums = ["0", "1", "5", "10", "100", "1.5", ".5", "5.", "-1", "-0", "+3", "1e3", "1E3", "0.0001", "99999999999", "2147483647", "2147483648", "1.9999", "0.5", "1.4999", "abc", "", "-", ".", "1..2", "NaN", "Infinity", "1_000", "00012", "1.2.3", "3.4028235E38", "1e39"]
units = ["", "t", "s", "d", "T", "S", "D", "x", "ticks", "tt", " t", "d ", "s rest", "\u00e4"]
for mn in ["", "0", "1", "-5"]:
    spec = "time" + (":" + mn if mn else "")
    for n in nums:
        for u in units:
            add(spec, n + u)

ids = ["minecraft:stone", "stone", ":stone", "minecraft:", "minecraft:Stone", "a:b/c", "a:b/c.d-e_f", "A:b", "a:B", "a b", "a:b c", "minecraft:stone ", "a/b", "a:b:c", "a::b", "", ":", "//", "a:../b", "..", "a:b..c", "\u00e4:b", "a:\u00e4", "mod_id:path/to/thing", "a-b.c:d-e.f", "a:b/", "/a:b", "a:/b", "a:b//c", "9:9", "a:b$", "a:b@c", "x" * 300, "a:" + "b" * 300, "minecraft:ston\u0301e", "#tag:x", "minecraft:stone[", "minecraft:stone{", "a.b:c.d"]
for i in ids:
    add("resource_location", i)
    add("resource_location", i + " rest")

ranges = ["1..5", "1", "..5", "1..", "..", "5..1", "-3..3", "-5", "1.5", "1..5.5", "1.5..3", "abc", "", "a..b", "1...5", "0..0", "2147483647", "2147483648", "..2147483648", "-2147483649..", "1..x", "+1..+2", "1 ..2", "1.. 2", "1..2 3", ".5", "5.", "3..", "..-1", "-1..-5", "00..01", "1e3", "1..1e3", "\u00e4"]
for r in ranges:
    add("int_range", r)

coord_atoms = ["0", "1", "-1", "5", "1.5", ".5", "-.5", "5.", "~", "~1", "~-1", "~.5", "~1.5", "~ ", "^", "^1", "^-1", "^.5", "~~", "^^", "1~", "~^", "^~", "x", "", "-", "+1", "1e3", "NaN", "30000000", "30000001", "-30000000", "1.0", "~0", "^0", "~1.0e2", "0.5", "-0", "100000000000"]
for spec, k in [("block_pos", 3), ("vec3", 3), ("vec2", 2), ("column_pos", 2)]:
    for _ in range(260):
        n = rnd.choice([0, 1, 2, 3, 3, 3, 3, 4])
        parts = [rnd.choice(coord_atoms) for _ in range(n)]
        sep = rnd.choice([" ", " ", " ", "  ", ""])
        s = sep.join(parts) if sep else "".join(parts)
        if rnd.random() < 0.15:
            s += rnd.choice([" rest", " x", " "])
        add(spec, s)
    for a in coord_atoms:
        add(spec, " ".join([a] * k))

msgs = ["hello", "hello world", "", " ", "a b c", "hi there \u00e4", "tab\tx", "trailing ", "\U0001F600", "multi  space", "text with # and ~", "quote \"x\" y"]
for m in msgs:
    add("message", m)

angs = ["0", "90", "-90", "~", "~5", "180.5", "x", "", "~-1", "1e2", "360", "-0", "5.", ".5", "~.5", "NaN"]
for _ in range(60):
    add("rotation", " ".join(rnd.choice(angs) for _ in range(rnd.choice([1, 2, 2, 2, 3]))))
import os
cases = os.path.join(os.path.dirname(os.path.abspath(__file__)), "entity_cases.txt")
for line in open(cases, encoding="utf-8").read().split("\n"):
    if line:
        out.append(line)

opts = ["name=Steve", "name=!Steve", "name=\"a b\"", "distance=..5", "distance=1..", "distance=2", "distance=-1", "distance=1.5..3.5", "level=1", "level=..3", "level=-2",
    "x=1", "y=~", "z=2.5", "dx=1", "dy=-2", "dz=0.5", "x_rotation=-90..90", "y_rotation=45", "limit=1", "limit=3", "limit=0", "sort=nearest", "sort=random", "sort=bad",
    "gamemode=creative", "gamemode=!survival", "gamemode=x", "team=red", "team=!blue", "team=", "type=zombie", "type=!zombie", "type=#minecraft:skeletons", "type=player", "type=bad",
    "tag=a", "tag=!b", "tag=", "nbt={a:1b}", "nbt={a:{b:[1,2]}}", "predicate=minecraft:x", "predicate=!a:b", "scores={a=1}", "scores={a=1..2,b=..3}", "scores={a=}", "advancements={a:b=true}",
    "advancements={a={c=false}}", "unknown=1", "name", "limit=", " x = 1 ", "\"x\"=1"]
letters = ["@e", "@a", "@p", "@r", "@s", "@n", "@e", "@e", "@a"]
specs = ["entity:multiple:entities", "entity:multiple:players", "entity:single:entities", "entity:single:players", "game_profile"]
for _ in range(1400):
    k = rnd.choice([0, 1, 1, 2, 2, 3, 4])
    body = rnd.choice([",", ", ", ","]).join(rnd.choice(opts) for _ in range(k))
    tail = rnd.choice(["]", "]", "]", "]", "", " ]", "]x", "] rest"])
    add(rnd.choice(specs), rnd.choice(letters) + "[" + body + tail)
names = ["Steve", "Alex_1", "a", "abcdefghijklmnop", "abcdefghijklmnopq", "\"a b\"", "'x'", "x y", "1-1-1-1-1", "-1-1-1-1-1", "+f-+f-+f-+f-+f", "g-1-1-1-1", "1-1-1-1", "1-1-1-1-1-1", "ffffffff-ffff-ffff-ffff-ffffffffffff", "00000000-0000-0000-0000-000000000000", "ffffffffffffffffff-1-1-1-1", "-8000000000000000-1-1-1-1", "8000000000000000-1-1-1-1", "\"unterminated", "@", "@@", "@e", "@x", "@E", "~", "", " ", "name,x"]
for n in names:
    for sp in specs:
        add(sp, n)
sys.stderr.write(str(len(out)))
open(sys.argv[1], "w", encoding="utf-8").write("\n".join(out) + "\n")

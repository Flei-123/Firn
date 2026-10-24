import re
import sys

LOG, DIR = sys.argv[1], sys.argv[2]
NAMES = ("lsink", "lhold", "lexit", "flow", "fall", "vine", "vhold", "vdown", "scland", "scwalk", "scclimb", "scdown", "icehold", "icecoast", "bubble")
rows = []
for line in open(LOG, errors="replace"):
    m = re.search(r"poslog tick=(\d+) tp=(\d+) x=(-?\d+) y=(-?\d+) z=(-?\d+) vy=(-?\d+) ground=(\w+)", line)
    if m:
        rows.append(tuple(int(m.group(i)) for i in range(1, 7)) + (m.group(7) == "true",))
spans = {}
for name, arg in zip(NAMES, sys.argv[3:]):
    lo, hi = (int(v) for v in arg.split(":"))
    spans[name] = [r for r in rows if lo <= r[0] <= hi]
bad = 0


def check(ok, msg):
    global bad
    if not ok:
        print("  FAIL  " + msg)
        bad += 1


def server_pos(name):
    try:
        m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", open("%s/%s.pos" % (DIR, name)).read())
        return [int(float(m.group(i)) * 1000) for i in (1, 2, 3)]
    except Exception:
        return None


def compare(name, row, tol_xz, tol_y):
    sp = server_pos(name)
    check(sp is not None, "%s: no server position" % name)
    if sp:
        print("  %s: client %d/%d/%d server %d/%d/%d" % (name, row[2], row[3], row[4], sp[0], sp[1], sp[2]))
        check(max(abs(row[2] - sp[0]), abs(row[4] - sp[2])) <= tol_xz and abs(row[3] - sp[1]) <= tol_y, "%s: client and server positions differ" % name)


for n, s in spans.items():
    check(len(s) >= 5, "%s: only %d poslog rows" % (n, len(s)))
if bad:
    sys.exit(1)

s = spans["lsink"]
drop = s[0][3] - s[-1][3]
print("  lava sink: y %d -> %d (drop %d mm) vy_end=%d" % (s[0][3], s[-1][3], drop, s[-1][5]))
check(2400 <= drop <= 3400, "lava sink drop %d mm outside 2400..3400" % drop)
check(all(-46 <= r[5] <= -10 for r in s[3:]), "lava sink vy outside -46..-10 mm/tick: %s" % [r[5] for r in s])
check(not any(r[6] for r in s), "lava sink: on ground inside the lava")
compare("lsink", s[-1], 250, 800)

s = spans["lhold"]
s2 = s[3:]
rate = (s2[-1][3] - s2[0][3]) * 1.0 / (s2[-1][0] - s2[0][0])
print("  lava hold (space): y %d -> %d, rise %.1f mm/tick (0.04 jump impulse per tick)" % (s2[0][3], s2[-1][3], rate))
check(33 <= rate <= 42, "lava rise %.1f mm/tick outside 33..42" % rate)

s = spans["lexit"]
print("  lava exit: x %d -> %d, y %d -> %d, vy_max=%d, ground=%s" % (s[0][2], s[-1][2], s[0][3], s[-1][3], max(r[5] for r in s), s[-1][6]))
check(s[-1][2] >= 36100, "lava exit: x %d mm < 36100 (still in the basin)" % s[-1][2])
check(-51200 <= s[-1][3] <= -50900, "lava exit: y %d mm outside -51200..-50900 (shore top -51000)" % s[-1][3])
check(s[-1][6], "lava exit: not standing on the shore")
check(max(r[5] for r in s) >= 150, "lava exit: no exit boost (vy max %d < 150)" % max(r[5] for r in s))
compare("lexit", s[-1], 300, 300)

s = spans["flow"]
dx = s[-1][2] - 53500
dz = s[-1][4] - s[0][4]
speed = max(abs(b[2] - a[2]) / float(max(1, b[0] - a[0])) for a, b in zip(s, s[1:]))
print("  flow: x %d -> %d (%d mm from the tp at 53500), z drift %d mm, max speed %.0f mm/tick" % (s[0][2], s[-1][2], dx, dz, speed))
check(2000 <= dx <= 4500, "flow drift %d mm outside 2000..4500 (water ends at x=57)" % dx)
check(abs(dz) <= 250, "flow side drift %d mm > 250" % dz)
check(speed <= 90, "flow speed %.0f mm/tick above 90" % speed)
compare("flow", s[-1], 700, 400)

s = spans["fall"]
vmin = min(r[5] for r in s)
gain = s[0][3] - s[-1][3]
print("  waterfall shaft: vy_min=%d, drop %d mm" % (vmin, gain))
check(-100 <= vmin <= -62, "waterfall vy_min %d outside -100..-62 (plain water sinks at -25)" % vmin)
check(gain >= 1500, "waterfall drop %d mm < 1500" % gain)

s = spans["vine"]
cg = s[-1][3] - s[0][3]
cv = max(r[5] for r in s)
print("  vine climb: y %d -> %d (gain %d mm) vy_max=%d" % (s[0][3], s[-1][3], cg, cv))
check(4500 <= cg <= 9000, "vine climb gain %d mm outside 4500..9000" % cg)
check(90 <= cv <= 140, "vine climb vy max %d outside 90..140 mm/tick" % cv)

s = spans["vhold"]
hd = max(r[3] for r in s) - min(r[3] for r in s)
print("  vine hold: y spread %d mm" % hd)
check(hd <= 300, "vine hold spread %d mm > 300" % hd)

s = spans["vdown"]
dd = s[0][3] - s[-1][3]
slide = max((a[3] - b[3]) * 1.0 / (b[0] - a[0]) for a, b in zip(s, s[1:]))
print("  vine slide: drop %d mm, slide %.0f mm/tick" % (dd, slide))
check(dd >= 2000, "vine slide only %d mm" % dd)
check(slide <= 155, "vine slide %.0f mm/tick above the 150 clamp" % slide)

s = spans["scland"]
print("  scaffold land: y %d -> %d, ground=%s" % (s[0][3], s[-1][3], s[-1][6]))
check(-54100 <= s[-1][3] <= -53900 and s[-1][6], "scaffold land: not standing on the top (y %d, want -54000)" % s[-1][3])
compare("scland", s[-1], 300, 200)

s = spans["scwalk"]
print("  scaffold walk: x %d -> %d" % (s[0][2], s[-1][2]))
check(s[-1][2] >= 74000, "scaffold walk: blocked, x %d < 74000" % s[-1][2])
compare("scwalk", s[-1], 1200, 300)

s = spans["scclimb"]
print("  scaffold climb: y %d -> %d (max %d)" % (s[0][3], s[-1][3], max(r[3] for r in s)))
check(max(r[3] for r in s) >= -54200, "scaffold climb: never reached the top (max y %d)" % max(r[3] for r in s))
compare("scclimb", s[-1], 300, 1300)

s = spans["scdown"]
print("  scaffold sneak down: y %d -> %d, ground=%s" % (s[0][3], s[-1][3], s[-1][6]))
check(s[-1][3] <= -59800 and s[-1][6], "scaffold sneak down: y %d, want about -60000 on the ground" % s[-1][3])
compare("scdown", s[-1], 300, 300)

s = spans["icehold"]
print("  ice hold: x %d -> %d" % (s[0][2], s[-1][2]))
s = spans["icecoast"]
coast = s[-1][2] - s[0][2]
print("  ice coast after release: x %d -> %d (coast %d mm)" % (s[0][2], s[-1][2], coast))
check(coast >= 1000, "ice coast %d mm < 1000 (no sliding)" % coast)
compare("icecoast", s[-1], 600, 200)

s = spans["bubble"]
vmax = max(r[5] for r in s)
print("  bubble column: y %d -> %d, vy_max=%d" % (s[0][3], s[-1][3], vmax))
check(vmax >= 250, "bubble column vy max %d < 250 mm/tick" % vmax)
compare("bubble", s[-1], 300, 1200)

sys.exit(1 if bad else 0)

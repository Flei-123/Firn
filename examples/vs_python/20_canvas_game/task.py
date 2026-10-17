# Bouncing ball + AI paddle, 120 frames, deterministic; last frame -> out.ppm.
# (With Pillow the drawing would be 3 lines: ImageDraw.rectangle/ellipse + save.)
W, H, R, PW, PH, PY = 160, 120, 5.0, 30.0, 6.0, 108.0
bx, by, vx, vy, px = 20.0, 30.0, 3.25, 2.5, 0.0
hits = misses = bounces = 0

for _ in range(120):
    bx += vx
    by += vy
    if bx < R or bx > W - R:
        bx = max(R, min(W - R, bx))
        vx = -vx
        bounces += 1
    if by < R:
        by, vy = R, -vy
        bounces += 1
    target = max(0.0, min(W - PW, bx - PW / 2))
    px += max(-3.0, min(3.0, target - px))
    if vy > 0 and PY <= by + R and by - R <= PY + PH and px <= bx <= px + PW:
        by, vy = PY - R, -vy
        hits += 1
    elif by > H + R:
        by = 30.0
        misses += 1

frame = bytearray(bytes((20, 20, 40)) * (W * H))
for y in range(H):
    for x in range(W):
        if PY <= y < PY + PH and px <= x < px + PW:
            frame[(y * W + x) * 3:(y * W + x) * 3 + 3] = bytes((230, 230, 230))
        if (x + 0.5 - bx) ** 2 + (y + 0.5 - by) ** 2 <= R * R:
            frame[(y * W + x) * 3:(y * W + x) * 3 + 3] = bytes((255, 200, 0))
with open("out.ppm", "wb") as f:
    f.write(b"P6\n%d %d\n255\n" % (W, H) + frame)

print("frames 120")
print(f"hits {hits} misses {misses} bounces {bounces}")
print(f"ball {int(bx * 100)} {int(by * 100)} paddle {int(px * 100)}")

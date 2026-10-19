import sys

sys.setrecursionlimit(10000)
M64 = 0xFFFFFFFFFFFFFFFF


class Tree:
    __slots__ = ("left", "right")

    def __init__(self, left, right):
        self.left = left
        self.right = right


def build(d):
    if d == 0:
        return Tree(None, None)
    l = build(d - 1)
    r = build(d - 1)
    return Tree(l, r)


def check(t):
    if t.left is None:
        return 1
    return 1 + check(t.left) + check(t.right)


min_depth = 4
max_depth = 16
total = 0
stretch = build(max_depth + 1)
total += check(stretch)
long_lived = build(max_depth)
for d in range(min_depth, max_depth + 1, 4):
    iters = 1 << (max_depth - d + min_depth)
    chk = 0
    for i in range(iters):
        t = build(d)
        chk += check(t)
    total = (total * 1000003 + chk) & M64
total = (total * 1000003 + check(long_lived)) & M64
print(total)

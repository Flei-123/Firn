import sys


def main():
    got = open(sys.argv[1], "rb").read()
    ref = open(sys.argv[2], "rb").read()
    w = int(sys.argv[3])
    h = int(sys.argv[4])
    if w == 0:
        if got == ref:
            sys.exit(0)
        n = min(len(got), len(ref))
        off = next((i for i in range(n) if got[i] != ref[i]), n)
        print("raw diff at byte %d got_len=%d ref_len=%d" % (off, len(got), len(ref)))
        sys.exit(1)
    bps = int(sys.argv[5]) if len(sys.argv) > 5 else 1
    ssx = int(sys.argv[6]) if len(sys.argv) > 6 else 1
    ssy = int(sys.argv[7]) if len(sys.argv) > 7 else 1
    cw = (w + ssx) >> ssx
    ch = (h + ssy) >> ssy
    sizes = [w * h * bps, cw * ch * bps, cw * ch * bps]
    fsize = sum(sizes)
    if len(got) != len(ref):
        print("size got=%d ref=%d" % (len(got), len(ref)))
    n = min(len(got), len(ref))
    bad = 0
    for i in range(n):
        if got[i] != ref[i]:
            f, off = divmod(i, fsize)
            plane = 0
            while off >= sizes[plane]:
                off -= sizes[plane]
                plane += 1
            pw = w if plane == 0 else cw
            px = off // bps
            print("first diff frame=%d plane=%d x=%d y=%d got=%d ref=%d" % (f, plane, px % pw, px // pw, got[i], ref[i]))
            bad = 1
            break
    if len(got) != len(ref):
        bad = 1
    sys.exit(bad)


main()

import java.util.*;

public class HashSetOrder {
    static long state;

    static long next() {
        state += 0x9e3779b97f4a7c15L;
        long z = state;
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
    }

    static int rnd(int lo, int span) {
        return lo + (int) Long.remainderUnsigned(next(), (long) span);
    }

    static final class Pos {
        final int x, y, z;
        Pos(int x, int y, int z) { this.x = x; this.y = y; this.z = z; }
        public int hashCode() { return (y + z * 31) * 31 + x; }
        public boolean equals(Object o) {
            if (!(o instanceof Pos)) return false;
            Pos p = (Pos) o;
            return p.x == x && p.y == y && p.z == z;
        }
    }

    public static void main(String[] args) {
        int cases = Integer.parseInt(args[0]);
        StringBuilder sb = new StringBuilder();
        for (int c = 0; c < cases; c++) {
            state = 0x5eed0000L + c;
            int shape = c % 6;
            int count;
            if (c % 7 < 5) count = 8 + rnd(0, 600); else count = 8 + rnd(0, 2400);
            int radius = 3 + rnd(0, 6);
            int y0 = rnd(-40, 80);
            HashSet<Pos> set = new HashSet<>();
            for (int i = 0; i < count; i++) {
                int x, y, z;
                if (shape == 0) {
                    x = rnd(-radius - 1, 2 * radius + 3);
                    z = rnd(-radius - 1, 2 * radius + 3);
                    y = y0 + rnd(0, 3);
                } else if (shape == 1) {
                    x = rnd(-6, 13);
                    z = rnd(-6, 13);
                    y = y0 + rnd(0, 21);
                } else if (shape == 2) {
                    x = rnd(-30, 61);
                    z = rnd(-30, 61);
                    y = y0 + rnd(0, 3);
                } else if (shape == 3) {
                    x = rnd(-300, 601);
                    y = rnd(-300, 601);
                    z = rnd(-300, 601);
                } else if (shape == 4) {
                    x = rnd(-2, 5);
                    z = rnd(-2, 5);
                    y = rnd(-64, 129);
                } else {
                    x = 64 * rnd(-400, 800);
                    if (rnd(0, 4) == 0) x += rnd(0, 64);
                    y = 0;
                    z = 0;
                }
                set.add(new Pos(x, y, z));
            }
            sb.append("C ").append(c).append(' ').append(set.size()).append('\n');
            for (Pos p : set) {
                sb.append(p.x).append(' ').append(p.y).append(' ').append(p.z).append('\n');
            }
        }
        System.out.print(sb);
    }
}

import java.io.PrintStream;

public class WgOracle {
    static long next(long[] s) {
        s[0] += 0x9e3779b97f4a7c15L;
        long z = s[0];
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
    }

    static int rnd(long[] s, int lo, int span) {
        return lo + (int) Long.remainderUnsigned(next(s), span);
    }

    public static void main(String[] a) throws Exception {
        PrintStream out = new PrintStream(new java.io.FileOutputStream(a[0]));
        String dim = a[1];
        WApi.boot();
        String[] fields = {"temperature", "vegetation", "continents", "erosion", "depth", "ridges", "final_density"};
        long[] gen = {0x1234abcdL};
        for (int si = 0; si < 4; si++) {
            long seed = next(gen);
            Object rs = WApi.state(seed, dim);
            for (int k = 0; k < 24; k++) {
                int x = rnd(gen, -3000, 6000);
                int y = rnd(gen, -64, 384);
                int z = rnd(gen, -3000, 6000);
                for (String f : fields) {
                    double v = WApi.field(rs, f, x, y, z);
                    if (!Double.isNaN(v) || true) {
                        out.println(seed + " " + x + " " + y + " " + z + " " + f + " " + Long.toHexString(Double.doubleToRawLongBits(v)));
                    }
                }
            }
        }
        out.close();
    }
}

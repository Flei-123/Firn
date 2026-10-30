import java.io.*;

public class Oracle {
    static PrintStream O;
    static long gs;
    static StringBuilder sb = new StringBuilder();

    static long gn() {
        gs += 0x9E3779B97F4A7C15L;
        long z = gs;
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }
    static int gi(int b) { return (int) Long.remainderUnsigned(gn(), b); }
    static double gu() { return (double) (gn() >>> 11) * 1.1102230246251565e-16; }
    static double gd(double s) { return (gu() * 2.0 - 1.0) * s; }
    static boolean OLD;
    static final double[] SC = {1e-3, 0.5, 8, 256, 1e4, 1e6, 3.3e7, 1e12};
    static final int[] SPI = {0, -1, 1, 16, -16, 29999984, -29999984, Integer.MIN_VALUE, Integer.MAX_VALUE, 1 << 24, -(1 << 24)};
    static double gx() {
        int k = gi(10);
        if (k == 9) return (double) (long) gd(100);
        return gd(SC[gi(8)]);
    }
    static int gc() {
        int k = gi(8);
        if (k == 0) return SPI[gi(11)];
        if (k < 4) return (int) (gn() >>> 41) - (1 << 22);
        if (k < 7) return (int) (gn() >>> 35) - (1 << 28);
        return (int) gn();
    }
    static String gname() {
        int n = 1 + gi(12);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < n; i++) {
            int k = gi(4);
            int cp;
            if (k == 0) cp = 0x20 + gi(95);
            else if (k == 1) cp = 0x80 + gi(0x780);
            else if (k == 2) {
                cp = 0x800 + gi(0xF800);
                if (cp >= 0xD800 && cp <= 0xDFFF) cp += 0x800;
            } else cp = 0x10000 + gi(0x100000);
            b.appendCodePoint(cp);
        }
        return b.toString();
    }

    static void b(String tag) { sb.setLength(0); sb.append(tag); }
    static void l(long v) { sb.append(' ').append(v); }
    static void i(int v) { sb.append(' ').append(v); }
    static void d(double v) { sb.append(' ').append(Long.toHexString(Double.doubleToRawLongBits(v))); }
    static void z(boolean v) { sb.append(v ? " 1" : " 0"); }
    static void e() { O.println(sb); }

    static Object mk(int kind, long s) {
        switch (kind) {
            case 0: return Api.legacy(s);
            case 1: return Api.xoro(s);
            case 2: return Api.wrap(Api.legacy(s));
            default: return Api.wrap(Api.xoro(s));
        }
    }

    static final int[] BOUNDS = {1, 2, 3, 5, 7, 10, 16, 24, 100, 1000, (1 << 20) + 3, 1 << 30, (1 << 30) + 17, Integer.MAX_VALUE};

    static void stream(int kind, long s) {
        Object r = mk(kind, s);
        b("rs " + kind);
        l(Api.nextLong(r));
        i(Api.nextInt(r));
        for (int k = 0; k < BOUNDS.length; k++) i(Api.nextIntB(r, BOUNDS[k]));
        z(Api.nextBool(r));
        d(Api.nextFloat(r));
        d(Api.nextDouble(r));
        d(Api.nextGauss(r));
        d(Api.nextGauss(r));
        d(Api.nextGauss(r));
        Api.consume(r, 37);
        l(Api.nextLong(r));
        Object f = Api.fork(r);
        l(Api.nextLong(f));
        l(Api.nextLong(r));
        Api.setSeed(r, s ^ 0x2545F4914F6CDD1DL);
        l(Api.nextLong(r));
        d(Api.nextGauss(r));
        i(Api.wgCount(r));
        e();
    }

    static void positional(int kind, long s) {
        Object r = mk(kind, s);
        Object f = Api.forkPos(r);
        b("ps " + kind);
        for (int k = 0; k < 3; k++) {
            int x = gc(), y = gc(), zz = gc();
            Object q = Api.posAt(f, x, y, zz);
            l(Api.nextLong(q));
            i(Api.nextIntB(q, 100));
            d(Api.nextDouble(q));
        }
        long psd = gn();
        Object q;
        if (!OLD) {
            q = Api.posSeed(f, psd);
            l(Api.nextLong(q));
        }
        for (int k = 0; k < 3; k++) {
            q = Api.posHash(f, gname());
            l(Api.nextLong(q));
            d(Api.nextDouble(q));
        }
        q = Api.posHash(f, "minecraft:overworld");
        l(Api.nextLong(q));
        q = Api.posHash(f, "octave_-3");
        l(Api.nextLong(q));
        q = Api.posHash(f, "");
        l(Api.nextLong(q));
        e();
    }

    static void worldgen(int kind, long s) {
        Object w = mk(kind, s);
        b("wg " + kind);
        long s2 = gn();
        int x = gc(), zz = gc(), idx = gi(100), step = gi(12), salt = gi(100000);
        i(Api.nextInt(w));
        l(Api.nextLong(w));
        d(Api.nextDouble(w));
        d(Api.nextGauss(w));
        z(Api.nextBool(w));
        i(Api.nextIntB(w, 1000));
        i(Api.wgCount(w));
        l(Api.decoSeed(w, s2, x, zz));
        l(Api.nextLong(w));
        i(Api.wgCount(w));
        Api.featureSeed(w, s2, idx, step);
        l(Api.nextLong(w));
        i(Api.wgCount(w));
        Api.largeFeature(w, s2, x, zz);
        l(Api.nextLong(w));
        i(Api.wgCount(w));
        Api.largeSalt(w, s2, x, zz, salt);
        l(Api.nextLong(w));
        i(Api.wgCount(w));
        Object sl = Api.slime(x, zz, s2, gn());
        i(Api.nextIntB(sl, 10));
        l(Api.nextLong(sl));
        e();
    }

    static void improved(int kind, long s) {
        Object h = Api.improved(mk(kind, s));
        for (int k = 0; k < 6; k++) {
            b("im " + kind + " " + k);
            d(Api.imXo(h));
            d(Api.imYo(h));
            d(Api.imZo(h));
            double x = gx(), y = gx(), zz = gx();
            double ys = gd(2), ym = gd(1);
            double[] dv = new double[3];
            d(Api.imNoise(h, x, y, zz));
            d(Api.imNoiseS(h, x, y, zz, ys, ym));
            d(Api.imDeriv(h, x, y, zz, dv));
            d(dv[0]);
            d(dv[1]);
            d(dv[2]);
            e();
        }
    }

    static double[] amps(int cnt, int lim) {
        double[] a = new double[cnt];
        for (int k = 0; k < cnt; k++) {
            if (gi(5) == 0) a[k] = 0.0;
            else if (gi(4) == 0) a[k] = 1.0;
            else a[k] = gd(3);
            if (k > lim) a[k] = 0.0;
        }
        int nz = gi(Math.min(cnt, lim + 1));
        if (a[nz] == 0.0) a[nz] = 1.0 + gu();
        return a;
    }

    static void perlin(int v, long s) {
        int kind = (v == 0 || v == 3) ? 1 : 0;
        int first = -gi(13);
        int cnt = 1 + gi(Math.min(6, 1 - first));
        double[] a = amps(cnt, cnt - 1);
        Object r = mk(kind, s);
        Object p = v < 2 ? Api.perlinNew(r, first, a) : Api.perlinLegacy(r, first, a);
        for (int k = 0; k < 4; k++) {
            b("pn " + v + " " + k + " " + first + " " + cnt);
            double x = gx(), y = gx(), zz = gx();
            double ys = gd(2), ym = gd(1);
            d(Api.perlinMax(p, ys));
            d(Api.perlinGet(p, x, y, zz));
            d(Api.perlinGetS(p, x, y, zz, ys, ym));
            d(Api.perlinWrap(x));
            for (int o = 0; o < 3; o++) {
                try {
                    Object oc = Api.perlinOct(p, o);
                    if (oc == null) sb.append(" -");
                    else d(Api.imNoise(oc, x, y, zz));
                } catch (RuntimeException ex) {
                    sb.append(" -");
                }
            }
            e();
        }
    }

    static void normal(int v, long s) {
        int kind = (v == 0 || v == 3) ? 1 : 0;
        int first = -gi(13);
        int cnt = 1 + gi(Math.min(6, 1 - first));
        double[] a = amps(cnt, cnt - 1);
        Object r = mk(kind, s);
        Object n = v < 2 ? Api.normalNew(r, first, a) : Api.normalLegacy(r, first, a);
        b("nn " + v + " " + first + " " + cnt);
        d(Api.normalMax(n));
        for (int k = 0; k < 4; k++) {
            double x = gx(), y = gx(), zz = gx();
            d(Api.normalGet(n, x, y, zz));
        }
        e();
    }

    static void blended(int kind, long s) {
        double xzS = 0.1 + gu() * 3, yS = 0.1 + gu() * 3, xzF = 20 + gu() * 200, yF = 20 + gu() * 200, sm = 1 + gu() * 15;
        Object n = Api.blended(mk(kind, s), xzS, yS, xzF, yF, sm);
        b("bl " + kind);
        d(xzS);
        d(sm);
        for (int k = 0; k < 4; k++) {
            int bx = gc(), by = gi(400) - 64, bz = gc();
            d(Api.blendedGet(n, bx, by, bz));
        }
        e();
    }

    static void simplex(int kind, long s) {
        Object h = Api.simplex(mk(kind, s));
        for (int k = 0; k < 6; k++) {
            b("sx " + kind + " " + k);
            d(Api.sxXo(h));
            d(Api.sxYo(h));
            d(Api.sxZo(h));
            double x = gx(), y = gx(), zz = gx();
            d(Api.sxGet2(h, x, y));
            d(Api.sxGet3(h, x, y, zz));
            e();
        }
    }

    static void psimplex(int kind, long s) {
        int mask = 1 + gi(63);
        int cnt = Integer.bitCount(mask);
        int[] oc = new int[cnt];
        int w = 0;
        for (int k = 0; k < 6; k++) if ((mask & (1 << k)) != 0) oc[w++] = k - 3;
        Object p = Api.psimplex(mk(kind, s), oc);
        b("pm " + kind + " " + mask);
        for (int k = 0; k < 4; k++) {
            double x = gx(), y = gx();
            boolean off = gi(2) == 0;
            d(Api.psGet(p, x, y, off));
        }
        e();
    }

    static void mth() {
        b("mt");
        double t = gd(2), a = gd(100), c = gd(100);
        d(Api.lerp(t, a, c));
        double t2 = gd(2);
        double v0 = gd(10), v1 = gd(10), v2 = gd(10), v3 = gd(10);
        d(Api.lerp2(t, t2, v0, v1, v2, v3));
        double t3 = gd(2);
        d(Api.lerp3(t, t2, t3, v0, v1, v2, v3, gd(10), gd(10), gd(10), gd(10)));
        double sv = gd(2);
        d(Api.smoothstep(sv));
        d(Api.smoothstepD(sv));
        d(Api.inverseLerp(t, a, c));
        d(Api.clampedLerp(a, c, t));
        d(Api.clampedMap(t, a, c, v0, v1));
        double fx = gx();
        i(Api.floor(fx));
        l(Api.lfloor(fx));
        l(Api.getSeed(gc(), gc(), gc()));
        l(Api.mixStafford13(gn()));
        e();
    }

    static final double[] SPECIAL_D = {0.0, -0.0, 0.5, -0.5, 2147483647.5, -2147483648.5, 2147483648.0, -2147483649.0, 3e9, -3e9, 9.3e18, -9.3e18, 1e30, -1e30, 4503599627370496.5, -1.0, 1.0};

    static void mthSpecial() {
        for (int k = 0; k < SPECIAL_D.length; k++) {
            double v = SPECIAL_D[k];
            if (OLD && (k == 5 || k == 7 || k == 9 || k == 11 || k == 13)) continue;
            b("ms " + k);
            i(Api.floor(v));
            l(Api.lfloor(v));
            d(Api.perlinWrap(v));
            d(Api.smoothstep(v));
            d(Api.clampedMap(v, -1.0, 1.0, 0.0, 10.0));
            e();
        }
    }

    static final long[] SPL = {0, -1, 1, Long.MIN_VALUE, Long.MAX_VALUE, 0x5DEECE66DL, 0xFFFFFFFFL, 1L << 48, (1L << 48) - 1, -(1L << 31), 42, 12345678901234L};

    public static void main(String[] args) {
        int nr = args.length > 0 ? Integer.parseInt(args[0]) : 300;
        try {
            O = new PrintStream(new BufferedOutputStream(new FileOutputStream(args[1]), 1 << 16), false);
        } catch (IOException ex) {
            throw new RuntimeException(ex);
        }
        OLD = args.length > 2 && args[2].equals("old");
        if (OLD) SC[7] = 1e9;
        Api.boot();
        mthSpecial();
        long master = 0xC0FFEEL;
        for (int n = 0; n < SPL.length + nr; n++) {
            long s;
            if (n < SPL.length) s = SPL[n];
            else {
                gs = master;
                s = gn();
                master = gs;
            }
            gs = s ^ 0x5851F42D4C957F2DL;
            for (int k = 0; k < 4; k++) stream(k, s);
            for (int k = 0; k < 4; k++) positional(k, s);
            for (int k = 2; k < 4; k++) worldgen(k, s);
            improved(0, s);
            improved(1, s);
            for (int v = 0; v < 4; v++) perlin(v, s);
            for (int v = 0; v < 4; v++) normal(v, s);
            blended(0, s);
            blended(1, s);
            simplex(0, s);
            simplex(1, s);
            psimplex(2, s);
            psimplex(3, s);
            for (int k = 0; k < 3; k++) mth();
        }
        O.flush();
    }
}

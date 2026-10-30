import java.util.*;
import it.unimi.dsi.fastutil.doubles.DoubleArrayList;
import net.minecraft.util.Mth;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.synth.*;

final class Api {
    static Object legacy(long s) { return new LegacyRandomSource(s); }
    static Object xoro(long s) { return new XoroshiroRandomSource(s); }
    static Object wrap(Object r) { return new WorldgenRandom((RandomSource) r); }
    static int nextInt(Object r) { return ((RandomSource) r).nextInt(); }
    static int nextIntB(Object r, int b) { return ((RandomSource) r).nextInt(b); }
    static long nextLong(Object r) { return ((RandomSource) r).nextLong(); }
    static boolean nextBool(Object r) { return ((RandomSource) r).nextBoolean(); }
    static double nextFloat(Object r) { return ((RandomSource) r).nextFloat(); }
    static double nextDouble(Object r) { return ((RandomSource) r).nextDouble(); }
    static double nextGauss(Object r) { return ((RandomSource) r).nextGaussian(); }
    static void consume(Object r, int n) { ((RandomSource) r).consumeCount(n); }
    static Object fork(Object r) { return ((RandomSource) r).fork(); }
    static void setSeed(Object r, long s) { ((RandomSource) r).setSeed(s); }
    static Object forkPos(Object r) { return ((RandomSource) r).forkPositional(); }
    static Object posAt(Object f, int x, int y, int z) { return ((PositionalRandomFactory) f).at(x, y, z); }
    static Object posSeed(Object f, long s) { return ((PositionalRandomFactory) f).fromSeed(s); }
    static Object posHash(Object f, String n) { return ((PositionalRandomFactory) f).fromHashOf(n); }
    static int wgCount(Object r) { return r instanceof WorldgenRandom ? ((WorldgenRandom) r).getCount() : -1; }
    static long decoSeed(Object r, long s, int x, int z) { return ((WorldgenRandom) r).setDecorationSeed(s, x, z); }
    static void featureSeed(Object r, long s, int i, int st) { ((WorldgenRandom) r).setFeatureSeed(s, i, st); }
    static void largeFeature(Object r, long s, int x, int z) { ((WorldgenRandom) r).setLargeFeatureSeed(s, x, z); }
    static void largeSalt(Object r, long s, int x, int z, int salt) { ((WorldgenRandom) r).setLargeFeatureWithSalt(s, x, z, salt); }
    static Object slime(int x, int z, long s, long salt) { return WorldgenRandom.seedSlimeChunk(x, z, s, salt); }

    static Object improved(Object r) { return new ImprovedNoise((RandomSource) r); }
    static double imXo(Object n) { return ((ImprovedNoise) n).xo; }
    static double imYo(Object n) { return ((ImprovedNoise) n).yo; }
    static double imZo(Object n) { return ((ImprovedNoise) n).zo; }
    static double imNoise(Object n, double x, double y, double z) { return ((ImprovedNoise) n).noise(x, y, z); }
    static double imNoiseS(Object n, double x, double y, double z, double ys, double ym) { return ((ImprovedNoise) n).noise(x, y, z, ys, ym); }
    static double imDeriv(Object n, double x, double y, double z, double[] o) { return ((ImprovedNoise) n).noiseWithDerivative(x, y, z, o); }

    static Object perlinNew(Object r, int first, double[] amps) { return PerlinNoise.create((RandomSource) r, first, new DoubleArrayList(amps)); }
    static Object perlinLegacy(Object r, int first, double[] amps) { return PerlinNoise.createLegacyForLegacyNetherBiome((RandomSource) r, first, new DoubleArrayList(amps)); }
    static double perlinGet(Object p, double x, double y, double z) { return ((PerlinNoise) p).getValue(x, y, z); }
    static double perlinGetS(Object p, double x, double y, double z, double ys, double ym) { return ((PerlinNoise) p).getValue(x, y, z, ys, ym); }
    static double perlinMax(Object p, double ys) { return ((PerlinNoise) p).maxBrokenValue(ys); }
    static Object perlinOct(Object p, int i) { return ((PerlinNoise) p).getOctaveNoise(i); }
    static double perlinWrap(double v) { return PerlinNoise.wrap(v); }

    static Object normalNew(Object r, int first, double[] amps) { return NormalNoise.create((RandomSource) r, new NormalNoise.NoiseParameters(first, new DoubleArrayList(amps))); }
    static Object normalLegacy(Object r, int first, double[] amps) { return NormalNoise.createLegacyNetherBiome((RandomSource) r, new NormalNoise.NoiseParameters(first, new DoubleArrayList(amps))); }
    static double normalGet(Object n, double x, double y, double z) { return ((NormalNoise) n).getValue(x, y, z); }
    static double normalMax(Object n) { return ((NormalNoise) n).maxValue(); }

    static void boot() {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
    }
    static Object blended(Object r, double a, double b, double c, double d, double e) { return new BlendedNoise((RandomSource) r, a, b, c, d, e); }
    static double blendedGet(Object b, int x, int y, int z) { return ((BlendedNoise) b).compute(new DensityFunction.SinglePointContext(x, y, z)); }

    static Object simplex(Object r) { return new SimplexNoise((RandomSource) r); }
    static double sxXo(Object n) { return ((SimplexNoise) n).xo; }
    static double sxYo(Object n) { return ((SimplexNoise) n).yo; }
    static double sxZo(Object n) { return ((SimplexNoise) n).zo; }
    static double sxGet2(Object n, double x, double y) { return ((SimplexNoise) n).getValue(x, y); }
    static double sxGet3(Object n, double x, double y, double z) { return ((SimplexNoise) n).getValue(x, y, z); }

    static Object psimplex(Object r, int[] octaves) {
        List<Integer> l = new ArrayList<>();
        for (int o : octaves) l.add(o);
        return new PerlinSimplexNoise((RandomSource) r, l);
    }
    static double psGet(Object p, double x, double y, boolean off) { return ((PerlinSimplexNoise) p).getValue(x, y, off); }

    static long getSeed(int x, int y, int z) { return Mth.getSeed(x, y, z); }
    static double lerp(double t, double a, double b) { return Mth.lerp(t, a, b); }
    static double lerp2(double a, double b, double c, double d, double e, double f) { return Mth.lerp2(a, b, c, d, e, f); }
    static double lerp3(double a, double b, double c, double d, double e, double f, double g, double h, double i, double j, double k) { return Mth.lerp3(a, b, c, d, e, f, g, h, i, j, k); }
    static double smoothstep(double x) { return Mth.smoothstep(x); }
    static double smoothstepD(double x) { return Mth.smoothstepDerivative(x); }
    static double inverseLerp(double v, double a, double b) { return Mth.inverseLerp(v, a, b); }
    static double clampedLerp(double a, double b, double t) { return Mth.clampedLerp(t, a, b); }
    static double clampedMap(double v, double a, double b, double c, double d) { return Mth.clampedMap(v, a, b, c, d); }
    static int floor(double v) { return Mth.floor(v); }
    static long lfloor(double v) { return Mth.lfloor(v); }
    static long mixStafford13(long v) { return RandomSupport.mixStafford13(v); }
}

package net.minecraft.world.level.levelgen;

import java.io.PrintStream;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Function;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.levelgen.feature.OreFeature;
import net.minecraft.world.level.levelgen.feature.ScatteredOreFeature;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;

public class WgFeatures {
    static long key(int x, int z) {
        return ((long) x << 32) ^ (z & 0xffffffffL);
    }

    static class Region {
        Map<Long, WgSurfaceOracle.Built> grid;
        int centerX;
        int centerZ;
        long seed;
        int minY;
        int height;
        BiomeManager bm;

        ChunkAccess chunk(int x, int z) {
            WgSurfaceOracle.Built b = grid.get(key(x, z));
            if (b == null) throw new IllegalStateException("chunk outside grid " + x + "," + z);
            return b.chunk;
        }

        boolean canWrite(BlockPos p) {
            return Math.abs((p.getX() >> 4) - centerX) <= 1 && Math.abs((p.getZ() >> 4) - centerZ) <= 1;
        }
    }

    static WorldGenLevel level(Region r) {
        InvocationHandler h = (proxy, m, args) -> {
            String n = m.getName();
            int argc = args == null ? 0 : args.length;
            switch (n) {
                case "toString": return "WgRegion";
                case "hashCode": return System.identityHashCode(proxy);
                case "equals": return proxy == args[0];
                case "getMinBuildHeight": return r.minY;
                case "getHeight":
                    if (argc == 0) return r.height;
                    if (argc == 3) {
                        int x = (Integer) args[1];
                        int z = (Integer) args[2];
                        return r.chunk(x >> 4, z >> 4).getHeight((Heightmap.Types) args[0], x & 15, z & 15) + 1;
                    }
                    break;
                case "getSeed": return r.seed;
                case "getBiomeManager": return r.bm;
                case "ensureCanWrite": return r.canWrite((BlockPos) args[0]);
                case "getBlockState": {
                    BlockPos p = (BlockPos) args[0];
                    return r.chunk(p.getX() >> 4, p.getZ() >> 4).getBlockState(p);
                }
                case "setBlock": {
                    BlockPos p = (BlockPos) args[0];
                    if (!r.canWrite(p)) return false;
                    r.chunk(p.getX() >> 4, p.getZ() >> 4).setBlockState(p, (BlockState) args[1], false);
                    return true;
                }
                case "getChunk":
                    if (args[0] instanceof BlockPos p) return r.chunk(p.getX() >> 4, p.getZ() >> 4);
                    return r.chunk((Integer) args[0], (Integer) args[1]);
                default:
                    break;
            }
            if (m.isDefault()) return InvocationHandler.invokeDefault(proxy, m, args);
            throw new UnsupportedOperationException(n + "/" + argc);
        };
        return (WorldGenLevel) Proxy.newProxyInstance(WgFeatures.class.getClassLoader(), new Class<?>[]{WorldGenLevel.class}, h);
    }

    @SuppressWarnings("unchecked")
    public static void run(WgSurfaceOracle.Env e, PrintStream out, String dim, String filter, int cx, int cz) throws Exception {
        WgCarve.bindTags();
        Function<ResourceKey<Biome>, Holder<Biome>> toHolder = e::biomeOf;
        Climate.ParameterList<Holder<Biome>> hl = (Climate.ParameterList<Holder<Biome>>) e.ap.invoke(e.provider, toHolder);
        MultiNoiseBiomeSource src = MultiNoiseBiomeSource.createFromList(hl);
        NoiseBasedChunkGenerator gen = new NoiseBasedChunkGenerator(src, e.ngsHolder);
        java.lang.reflect.Field ff = net.minecraft.world.level.chunk.ChunkGenerator.class.getDeclaredField("featuresPerStep");
        ff.setAccessible(true);
        List<?> steps = (List<?>) ((java.util.function.Supplier<?>) ff.get(gen)).get();
        Class<?> sd = Class.forName("net.minecraft.world.level.biome.FeatureSorter$StepFeatureData");
        Method featuresM = sd.getDeclaredMethod("features");
        Method indexM = sd.getDeclaredMethod("indexMapping");
        featuresM.setAccessible(true);
        indexM.setAccessible(true);
        if (filter.equals("order")) {
            Map<PlacedFeature, String> names = new java.util.IdentityHashMap<>();
            e.prov.lookupOrThrow(Registries.PLACED_FEATURE).listElements().forEach(h -> names.put(h.value(), h.key().location().getPath()));
            for (int j = 0; j < steps.size(); j++) {
                List<PlacedFeature> fs = (List<PlacedFeature>) featuresM.invoke(steps.get(j));
                for (int k = 0; k < fs.size(); k++) out.println(j + " " + k + " " + names.get(fs.get(k)));
            }
            return;
        }
        int minY = e.ngs.noiseSettings().minY();
        int height = e.ngs.noiseSettings().height();
        Region r = new Region();
        r.grid = new HashMap<>();
        r.seed = e.seed;
        r.minY = minY;
        r.height = height;
        for (int dx = -2; dx <= 2; dx++)
            for (int dz = -2; dz <= 2; dz++) r.grid.put(key(cx + dx, cz + dz), e.build(cx + dx, cz + dz));
        int fillMinY = minY >> 2;
        int fillMaxY = fillMinY + (height >> 2);
        BiomeManager.NoiseBiomeSource nbs = (qx, qy, qz) -> {
            WgSurfaceOracle.Built b = r.grid.get(key(qx >> 2, qz >> 2));
            int cy = Math.max(fillMinY, Math.min(fillMaxY - 1, qy));
            if (b == null) throw new IllegalStateException("biome outside grid");
            return e.biomeOf(b.memo.get(qx + "," + cy + "," + qz));
        };
        r.bm = new BiomeManager(nbs, BiomeManager.obfuscateSeed(e.seed));
        WorldGenLevel level = level(r);
        boolean oresOnly = filter.equals("ore");
        WorldgenRandom rnd = new WorldgenRandom(new XoroshiroRandomSource(0L));
        for (int dz = -1; dz <= 1; dz++) {
            for (int dx = -1; dx <= 1; dx++) {
                int nx = cx + dx;
                int nz = cz + dz;
                r.centerX = nx;
                r.centerZ = nz;
                BlockPos origin = new BlockPos(nx * 16, minY, nz * 16);
                long l = rnd.setDecorationSeed(e.seed, origin.getX(), origin.getZ());
                Set<Holder<Biome>> set = new LinkedHashSet<>();
                for (int ax = -1; ax <= 1; ax++)
                    for (int az = -1; az <= 1; az++)
                        for (ResourceKey<Biome> k : r.grid.get(key(nx + ax, nz + az)).memo.values()) set.add(e.biomeOf(k));
                for (int j = 0; j < steps.size(); j++) {
                    java.util.TreeSet<Integer> idx = new java.util.TreeSet<>();
                    java.util.function.ToIntFunction<PlacedFeature> mapping = (java.util.function.ToIntFunction<PlacedFeature>) indexM.invoke(steps.get(j));
                    for (Holder<Biome> b : set) {
                        List<HolderSet<PlacedFeature>> bl = b.value().getGenerationSettings().features();
                        if (j >= bl.size()) continue;
                        bl.get(j).stream().map(Holder::value).forEach(pf -> idx.add(mapping.applyAsInt(pf)));
                    }
                    List<PlacedFeature> fs = (List<PlacedFeature>) featuresM.invoke(steps.get(j));
                    for (int n : idx) {
                        PlacedFeature pf = fs.get(n);
                        if (oresOnly) {
                            Object f = pf.feature().value().feature();
                            if (!(f instanceof OreFeature) && !(f instanceof ScatteredOreFeature)) continue;
                        }
                        rnd.setFeatureSeed(l, n, j);
                        pf.placeWithBiomeCheck(level, gen, rnd, origin);
                    }
                }
            }
        }
        WgSurfaceOracle.dump(out, r.grid.get(key(cx, cz)).chunk, e.seed, cx, cz, minY, height);
    }
}

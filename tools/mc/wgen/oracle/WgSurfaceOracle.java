package net.minecraft.world.level.levelgen;
import java.io.PrintStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.function.Function;
import com.mojang.serialization.Lifecycle;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.FixedBiomeSource;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkStatus;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseChunk;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldGenerationContext;
import net.minecraft.world.level.levelgen.blending.Blender;

public class WgSurfaceOracle {
    static ResourceKey<NoiseGeneratorSettings> keyOf(String dim) {
        if (dim.equals("nether")) return NoiseGeneratorSettings.NETHER;
        if (dim.equals("end")) return NoiseGeneratorSettings.END;
        if (dim.equals("amplified")) return NoiseGeneratorSettings.AMPLIFIED;
        if (dim.equals("large_biomes")) return NoiseGeneratorSettings.LARGE_BIOMES;
        return NoiseGeneratorSettings.OVERWORLD;
    }

    static class Built {
        ProtoChunk chunk;
        HashMap<String, ResourceKey<Biome>> memo;
        NoiseChunk nc;
    }

    static class Env {
        HolderLookup.Provider prov;
        NoiseGeneratorSettings ngs;
        Holder.Reference<NoiseGeneratorSettings> ngsHolder;
        Method ap;
        Object provider;
        Climate.ParameterList<ResourceKey<Biome>> list;
        MappedRegistry<Biome> biomeReg;
        Holder<Biome> plains;
        long seed;
        RandomState rs;
        Climate.Sampler sampler;
        Climate.ParameterList<ResourceKey<Biome>> chunkList;
        boolean carve;

        @SuppressWarnings("unchecked")
        void reset(long s) throws Exception {
            seed = s;
            HolderGetter<net.minecraft.world.level.levelgen.synth.NormalNoise.NoiseParameters> noises = (HolderGetter<net.minecraft.world.level.levelgen.synth.NormalNoise.NoiseParameters>) (Object) prov.lookupOrThrow(Registries.NOISE);
            rs = RandomState.create(ngs, noises, seed);
            sampler = rs.sampler();
            chunkList = (Climate.ParameterList<ResourceKey<Biome>>) ap.invoke(provider, Function.identity());
        }

        Holder<Biome> biomeOf(ResourceKey<Biome> k) {
            return prov.lookupOrThrow(Registries.BIOME).getOrThrow(k);
        }

        Built build(int cx, int cz) throws Exception {
            Built b = new Built();
            b.memo = new HashMap<>();
            HashMap<String, ResourceKey<Biome>> memo = b.memo;
            resetLast(chunkList);
            int fillMinY = ngs.noiseSettings().minY() >> 2;
            int fillMaxY = fillMinY + (ngs.noiseSettings().height() >> 2);
            for (int fy = fillMinY; fy < fillMaxY; fy++)
                for (int fz = 0; fz < 4; fz++)
                    for (int fx = 0; fx < 4; fx++) {
                        int gx = cx * 4 + fx;
                        int gz = cz * 4 + fz;
                        memo.put(gx + "," + fy + "," + gz, chunkList.findValue(sampler.sample(gx, fy, gz)));
                    }
            final int clampLo = fillMinY;
            final int clampHi = fillMaxY - 1;
            BiomeManager.NoiseBiomeSource src = (qx, qy0, qz) -> {
                int qy = Math.max(clampLo, Math.min(clampHi, qy0));
                ResourceKey<Biome> k = memo.get(qx + "," + qy + "," + qz);
                if (k == null) {
                    resetLast(list);
                    k = list.findValue(sampler.sample(qx, qy, qz));
                }
                return biomeOf(k);
            };
            BiomeManager bm = new BiomeManager(src, BiomeManager.obfuscateSeed(seed));
            int minY = ngs.noiseSettings().minY();
            int height = ngs.noiseSettings().height();
            LevelHeightAccessor lha = LevelHeightAccessor.create(minY, height);
            ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, lha, biomeReg, null);
            NoiseBasedChunkGenerator gen = new NoiseBasedChunkGenerator(new FixedBiomeSource(plains), ngsHolder);
            WorldGenerationContext wgc = new WorldGenerationContext(gen, lha);
            Method fp = NoiseBasedChunkGenerator.class.getDeclaredMethod("createFluidPicker", NoiseGeneratorSettings.class);
            fp.setAccessible(true);
            Object picker = fp.invoke(null, ngs);
            Constructor<?> ctor = null;
            for (Constructor<?> c : NoiseChunk.class.getDeclaredConstructors()) if (c.getParameterCount() == 9) ctor = c;
            ctor.setAccessible(true);
            Class<?> bm2 = Class.forName("net.minecraft.world.level.levelgen.DensityFunctions$BeardifierMarker");
            Object beard = bm2.getField("INSTANCE").get(null);
            int cw = ngs.noiseSettings().getCellWidth();
            NoiseChunk nc = (NoiseChunk) ctor.newInstance(16 / cw, rs, cx * 16, cz * 16, ngs.noiseSettings(), beard, ngs, picker, Blender.empty());
            fill(chunk, nc, ngs, cx, cz);
            chunk.setStatus(ChunkStatus.NOISE);
            rs.surfaceSystem().buildSurface(rs, bm, null, ngs.useLegacyRandomSource(), wgc, chunk, nc, ngs.surfaceRule());
            if (carve) WgCarve.run(prov, rs, seed, chunk, nc, gen, ngs, lha, bm, sampler, list, biomeReg, cx, cz);
            b.chunk = chunk;
            b.nc = nc;
            return b;
        }
    }

    public static void main(String[] a) throws Exception {
        PrintStream out = new PrintStream(new java.io.FileOutputStream(a[0]));
        String dim = a[1];
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Env e = new Env();
        e.prov = VanillaRegistries.createLookup();
        HolderLookup.Provider prov = e.prov;
        ResourceKey<NoiseGeneratorSettings> key = keyOf(dim);
        e.ngsHolder = prov.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(key);
        NoiseGeneratorSettings ngs = e.ngsHolder.value();
        e.ngs = ngs;
        Class<?> pc = Class.forName("net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList$Preset");
        Object preset = pc.getField(dim.equals("nether") ? "NETHER" : "OVERWORLD").get(null);
        Method pm = pc.getDeclaredMethod("provider");
        pm.setAccessible(true);
        e.provider = pm.invoke(preset);
        e.ap = Class.forName("net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList$Preset$SourceProvider").getDeclaredMethod("apply", Function.class);
        e.ap.setAccessible(true);
        @SuppressWarnings("unchecked")
        Climate.ParameterList<ResourceKey<Biome>> list = (Climate.ParameterList<ResourceKey<Biome>>) e.ap.invoke(e.provider, Function.identity());
        e.list = list;
        e.plains = prov.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        e.biomeReg = new MappedRegistry<>(Registries.BIOME, Lifecycle.stable());
        e.biomeReg.register(Biomes.PLAINS, e.plains.value(), Lifecycle.stable());
        e.carve = System.getProperty("wg.carve") != null || System.getProperty("wg.features") != null;
        String features = System.getProperty("wg.features");
        for (int i = 2; i < a.length; i++) {
            String[] p = a[i].split(":");
            long seed = Long.parseLong(p[0]);
            int cx = Integer.parseInt(p[1]);
            int cz = Integer.parseInt(p[2]);
            e.reset(seed);
            if (features != null) {
                WgFeatures.run(e, out, dim, features, cx, cz);
            } else {
                dump(out, e.build(cx, cz).chunk, seed, cx, cz, ngs.noiseSettings().minY(), ngs.noiseSettings().height());
            }
        }
        out.close();
    }

    static void resetLast(Object list) {
        try {
            for (java.lang.reflect.Field f : list.getClass().getDeclaredFields()) {
                if (java.lang.reflect.Modifier.isStatic(f.getModifiers())) continue;
                f.setAccessible(true);
                Object tree = f.get(list);
                if (tree == null) continue;
                for (java.lang.reflect.Field g : tree.getClass().getDeclaredFields()) {
                    if (g.getType() == ThreadLocal.class) {
                        g.setAccessible(true);
                        ((ThreadLocal<?>) g.get(tree)).set(null);
                        return;
                    }
                }
            }
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }

    static void fill(ProtoChunk chunk, NoiseChunk nc, NoiseGeneratorSettings ngs, int cx, int cz) {
        Heightmap h1 = chunk.getOrCreateHeightmapUnprimed(Heightmap.Types.OCEAN_FLOOR_WG);
        Heightmap h2 = chunk.getOrCreateHeightmapUnprimed(Heightmap.Types.WORLD_SURFACE_WG);
        int minX = cx * 16;
        int minZ = cz * 16;
        int cellW = nc.cellWidth();
        int cellH = nc.cellHeight();
        int cellsXZ = 16 / cellW;
        int minCellY = Math.floorDiv(ngs.noiseSettings().minY(), cellH);
        int cellCountY = ngs.noiseSettings().height() / cellH;
        nc.initializeForFirstCellX();
        for (int ix = 0; ix < cellsXZ; ix++) {
            nc.advanceCellX(ix);
            for (int iz = 0; iz < cellsXZ; iz++) {
                int secIdx = chunk.getSectionsCount() - 1;
                LevelChunkSection sec = chunk.getSection(secIdx);
                for (int iy = cellCountY - 1; iy >= 0; iy--) {
                    nc.selectCellYZ(iy, iz);
                    for (int ly = cellH - 1; ly >= 0; ly--) {
                        int y = (minCellY + iy) * cellH + ly;
                        int ys = y & 15;
                        int si = chunk.getSectionIndex(y);
                        if (secIdx != si) {
                            secIdx = si;
                            sec = chunk.getSection(si);
                        }
                        nc.updateForY(y, (double) ly / cellH);
                        for (int lx = 0; lx < cellW; lx++) {
                            int x = minX + ix * cellW + lx;
                            nc.updateForX(x, (double) lx / cellW);
                            for (int lz = 0; lz < cellW; lz++) {
                                int z = minZ + iz * cellW + lz;
                                nc.updateForZ(z, (double) lz / cellW);
                                BlockState st = nc.getInterpolatedState();
                                if (st == null) st = ngs.defaultBlock();
                                if (st != Blocks.AIR.defaultBlockState()) {
                                    sec.setBlockState(x & 15, ys, z & 15, st, false);
                                    h1.update(x & 15, y, z & 15, st);
                                    h2.update(x & 15, y, z & 15, st);
                                }
                            }
                        }
                    }
                }
            }
            nc.swapSlices();
        }
        nc.stopInterpolation();
    }

    static void dump(PrintStream out, ProtoChunk chunk, long seed, int cx, int cz, int minY, int height) {
        BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
        for (int lx = 0; lx < 16; lx++) {
            for (int lz = 0; lz < 16; lz++) {
                StringBuilder sb = new StringBuilder();
                sb.append(seed).append(' ').append(cx).append(' ').append(cz).append(" R ").append(lx).append(' ').append(lz);
                String last = null;
                int run = 0;
                for (int y = minY + height - 1; y >= minY; y--) {
                    BlockState st = chunk.getSection(chunk.getSectionIndex(y)).getBlockState(lx, y & 15, lz);
                    String n = BuiltInRegistries.BLOCK.getKey(st.getBlock()).getPath();
                    if (n.equals("bee_nest")) n = "air";
                    boolean waterlogged = st.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED) && st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED);
                    if (st.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.DISTANCE)) {
                        n += "[distance=" + st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.DISTANCE) + (waterlogged ? ",waterlogged=true" : "") + "]";
                    } else if (st.getBlock() instanceof net.minecraft.world.level.block.HugeMushroomBlock) {
                        n += "[" + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.DOWN) ? "1" : "0") + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.EAST) ? "1" : "0") + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.NORTH) ? "1" : "0") + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.SOUTH) ? "1" : "0") + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.UP) ? "1" : "0") + (st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WEST) ? "1" : "0") + "]";
                    } else if (st.is(net.minecraft.tags.BlockTags.LOGS) && st.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.AXIS)) {
                        n += "[axis=" + st.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.AXIS).getSerializedName() + "]";
                    } else if (waterlogged) {
                        n += "[waterlogged=true]";
                    }
                    if (n.equals(last)) {
                        run++;
                    } else {
                        if (last != null) sb.append(' ').append(last).append('*').append(run);
                        last = n;
                        run = 1;
                    }
                }
                sb.append(' ').append(last).append('*').append(run);
                out.println(sb);
            }
        }
    }
}

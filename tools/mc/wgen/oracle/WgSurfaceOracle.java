package net.minecraft.world.level.levelgen;
import java.io.PrintStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.ArrayList;
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

    public static void main(String[] a) throws Exception {
        PrintStream out = new PrintStream(new java.io.FileOutputStream(a[0]));
        String dim = a[1];
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        HolderLookup.Provider prov = VanillaRegistries.createLookup();
        ResourceKey<NoiseGeneratorSettings> key = keyOf(dim);
        Holder.Reference<NoiseGeneratorSettings> ngsHolder = prov.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(key);
        NoiseGeneratorSettings ngs = ngsHolder.value();
        Class<?> pc = Class.forName("net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList$Preset");
        Object preset = pc.getField(dim.equals("nether") ? "NETHER" : "OVERWORLD").get(null);
        Method pm = pc.getDeclaredMethod("provider");
        pm.setAccessible(true);
        Object provider = pm.invoke(preset);
        Method ap = Class.forName("net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList$Preset$SourceProvider").getDeclaredMethod("apply", Function.class);
        ap.setAccessible(true);
        @SuppressWarnings("unchecked")
        Climate.ParameterList<ResourceKey<Biome>> list = (Climate.ParameterList<ResourceKey<Biome>>) ap.invoke(provider, Function.identity());
        Holder<Biome> plains = prov.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        MappedRegistry<Biome> biomeReg = new MappedRegistry<>(Registries.BIOME, Lifecycle.stable());
        biomeReg.register(Biomes.PLAINS, plains.value(), Lifecycle.stable());
        for (int i = 2; i < a.length; i++) {
            String[] p = a[i].split(":");
            long seed = Long.parseLong(p[0]);
            int cx = Integer.parseInt(p[1]);
            int cz = Integer.parseInt(p[2]);
            @SuppressWarnings("unchecked")
            HolderGetter<net.minecraft.world.level.levelgen.synth.NormalNoise.NoiseParameters> noises = (HolderGetter<net.minecraft.world.level.levelgen.synth.NormalNoise.NoiseParameters>) (Object) prov.lookupOrThrow(Registries.NOISE);
            RandomState rs = RandomState.create(ngs, noises, seed);
            Climate.Sampler sampler = rs.sampler();
            @SuppressWarnings("unchecked")
            Climate.ParameterList<ResourceKey<Biome>> chunkList = (Climate.ParameterList<ResourceKey<Biome>>) ap.invoke(provider, Function.identity());
            java.util.HashMap<String, ResourceKey<Biome>> memo = new java.util.HashMap<>();
            int fillMinY = ngs.noiseSettings().minY() >> 2;
            int fillMaxY = fillMinY + (ngs.noiseSettings().height() >> 2);
            for (int fy = fillMinY; fy < fillMaxY; fy++)
                for (int fz = 0; fz < 4; fz++)
                    for (int fx = 0; fx < 4; fx++) {
                        int gx = cx * 4 + fx;
                        int gz = cz * 4 + fz;
                        memo.put(gx + "," + fy + "," + gz, chunkList.findValue(sampler.sample(gx, fy, gz)));
                    }
            BiomeManager.NoiseBiomeSource src = (qx, qy, qz) -> {
                ResourceKey<Biome> k = memo.get(qx + "," + qy + "," + qz);
                if (k == null) {
                    resetLast(list);
                    k = list.findValue(sampler.sample(qx, qy, qz));
                }
                return prov.lookupOrThrow(Registries.BIOME).getOrThrow(k);
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
            dump(out, chunk, seed, cx, cz, minY, height);
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
                    BlockState st = chunk.getBlockState(pos.set(cx * 16 + lx, y, cz * 16 + lz));
                    String n = BuiltInRegistries.BLOCK.getKey(st.getBlock()).getPath();
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

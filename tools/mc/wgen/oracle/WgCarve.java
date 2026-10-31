package net.minecraft.world.level.levelgen;
import java.util.List;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.chunk.ChunkStatus;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.levelgen.GenerationStep;
import net.minecraft.world.level.levelgen.carver.CarvingContext;
import net.minecraft.world.level.levelgen.carver.ConfiguredWorldCarver;
import net.minecraft.world.level.chunk.CarvingMask;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.LegacyRandomSource;

public class WgCarve {
    static boolean tagsBound = false;

    static void resolve(java.nio.file.Path dir, String name, java.util.Set<String> out, java.util.Set<String> seen) throws Exception {
        if (!seen.add(name)) return;
        com.google.gson.JsonObject o = com.google.gson.JsonParser.parseString(java.nio.file.Files.readString(dir.resolve(name + ".json"))).getAsJsonObject();
        for (com.google.gson.JsonElement e : o.getAsJsonArray("values")) {
            String v = e.getAsString();
            if (v.startsWith("#minecraft:")) resolve(dir, v.substring(11), out, seen);
            else out.add(v.substring(10));
        }
    }

    static void bindTags() throws Exception {
        if (tagsBound) return;
        tagsBound = true;
        java.nio.file.Path dir = java.nio.file.Path.of(System.getProperty("wg.tags"));
        java.util.Map<net.minecraft.tags.TagKey<net.minecraft.world.level.block.Block>, List<Holder<net.minecraft.world.level.block.Block>>> map = new java.util.HashMap<>();
        try (java.util.stream.Stream<java.nio.file.Path> st = java.nio.file.Files.list(dir)) {
            for (java.nio.file.Path f : (Iterable<java.nio.file.Path>) st::iterator) {
                String name = f.getFileName().toString().replace(".json", "");
                java.util.Set<String> vals = new java.util.LinkedHashSet<>();
                resolve(dir, name, vals, new java.util.HashSet<>());
                List<Holder<net.minecraft.world.level.block.Block>> hs = new java.util.ArrayList<>();
                for (String v : vals) {
                    hs.add(net.minecraft.core.registries.BuiltInRegistries.BLOCK.getHolderOrThrow(ResourceKey.create(Registries.BLOCK, new net.minecraft.resources.ResourceLocation("minecraft", v))));
                }
                map.put(net.minecraft.tags.TagKey.create(Registries.BLOCK, new net.minecraft.resources.ResourceLocation("minecraft", name)), hs);
            }
        }
        net.minecraft.core.registries.BuiltInRegistries.BLOCK.bindTags(map);
    }

    public static void run(HolderLookup.Provider prov, RandomState rs, long seed, ProtoChunk chunk, NoiseChunk nc, NoiseBasedChunkGenerator gen,
                           NoiseGeneratorSettings ngs, LevelHeightAccessor lha, BiomeManager bm, Climate.Sampler sampler,
                           Climate.ParameterList<ResourceKey<Biome>> list, MappedRegistry<Biome> biomeReg, int cx, int cz) {
        try { bindTags(); } catch (Exception e) { throw new RuntimeException(e); }
        chunk.setStatus(ChunkStatus.SURFACE);
        RegistryAccess ra = new RegistryAccess.ImmutableRegistryAccess(List.of(biomeReg));
        CarvingContext ctx = new CarvingContext(gen, ra, lha, nc, rs, ngs.surfaceRule());
        Aquifer aq = nc.aquifer();
        String dbg = System.getProperty("wg.aqdbg");
        if (dbg != null) {
            StringBuilder sb = new StringBuilder();
            if (dbg.startsWith("scan:")) {
                String[] r = dbg.substring(5).split(",");
                int[] v = new int[6];
                for (int i = 0; i < 6; i++) v[i] = Integer.parseInt(r[i]);
                for (int px = v[0]; px <= v[1]; px++) for (int py = v[2]; py <= v[3]; py++) for (int pz = v[4]; pz <= v[5]; pz++) {
                    net.minecraft.world.level.block.state.BlockState bs = aq.computeSubstance(new net.minecraft.world.level.levelgen.DensityFunction.SinglePointContext(px, py, pz), 0.0);
                    String n = bs == null ? "0" : bs.is(net.minecraft.world.level.block.Blocks.LAVA) ? "76" : bs.is(net.minecraft.world.level.block.Blocks.WATER) ? "87" : "65";
                    sb.append("AQ " + px + " " + py + " " + pz + " " + n + "\n");
                }
                dbg = "";
                try {
                    String[] names = {"fluidLevelFloodednessNoise", "fluidLevelSpreadNoise", "lavaNoise", "barrierNoise", "erosion", "depth"};
                    int[][] pts = {{-1581,-40,-1599},{-1581,-55,-1599},{-1581,-10,-1595}};
                    for (String nm : names) {
                        java.lang.reflect.Field ff = aq.getClass().getDeclaredField(nm);
                        ff.setAccessible(true);
                        net.minecraft.world.level.levelgen.DensityFunction df = (net.minecraft.world.level.levelgen.DensityFunction) ff.get(aq);
                        for (int[] pt : pts) sb.append("NZ " + nm + " " + pt[0] + " " + pt[1] + " " + pt[2] + " " + df.compute(new net.minecraft.world.level.levelgen.DensityFunction.SinglePointContext(pt[0], pt[1], pt[2])) + "\n");
                    }
                } catch (Exception e) { throw new RuntimeException(e); }
                try {
                    java.lang.reflect.Field fc = aq.getClass().getDeclaredField("aquiferCache");
                    java.lang.reflect.Field fl = aq.getClass().getDeclaredField("aquiferLocationCache");
                    fc.setAccessible(true); fl.setAccessible(true);
                    Object[] cache = (Object[]) fc.get(aq);
                    long[] locs = (long[]) fl.get(aq);
                    for (int i = 0; i < cache.length; i++) {
                        if (cache[i] == null) continue;
                        Object st = cache[i];
                        java.lang.reflect.Field a = st.getClass().getDeclaredField("fluidLevel");
                        a.setAccessible(true);
                        java.lang.reflect.Field b = st.getClass().getDeclaredField("fluidType");
                        b.setAccessible(true);
                        net.minecraft.core.BlockPos bp = net.minecraft.core.BlockPos.of(locs[i]);
                        sb.append("ST " + bp.getX() + " " + bp.getY() + " " + bp.getZ() + " " + a.get(st) + " " + ((net.minecraft.world.level.block.state.BlockState) b.get(st)).getBlock().getDescriptionId() + "\n");
                    }
                } catch (Exception e) { throw new RuntimeException(e); }
            }
            for (String t : dbg.isEmpty() ? new String[0] : dbg.split(";")) {
                String[] q = t.split(",");
                int px = Integer.parseInt(q[0]), py = Integer.parseInt(q[1]), pz = Integer.parseInt(q[2]);
                net.minecraft.world.level.block.state.BlockState bs = aq.computeSubstance(new net.minecraft.world.level.levelgen.DensityFunction.SinglePointContext(px, py, pz), 0.0);
                sb.append("AQ " + px + " " + py + " " + pz + " " + (bs == null ? "null" : bs.getBlock().getDescriptionId()) + "\n");
            }
            try { java.nio.file.Files.writeString(java.nio.file.Path.of(System.getProperty("wg.aqout")), sb.toString()); } catch (Exception e) { throw new RuntimeException(e); }
        }
        CarvingMask mask = chunk.getOrCreateCarvingMask(GenerationStep.Carving.AIR);
        WorldgenRandom rnd = new WorldgenRandom(new LegacyRandomSource(0L));
        for (int dx = -8; dx <= 8; dx++) {
            for (int dz = -8; dz <= 8; dz++) {
                ChunkPos sp = new ChunkPos(cx + dx, cz + dz);
                WgSurfaceOracle.resetLast(list);
                ResourceKey<Biome> key = list.findValue(sampler.sample(sp.getMinBlockX() >> 2, 0, sp.getMinBlockZ() >> 2));
                Holder<Biome> bh = prov.lookupOrThrow(Registries.BIOME).getOrThrow(key);
                int idx = 0;
                for (Holder<ConfiguredWorldCarver<?>> h : bh.value().getGenerationSettings().getCarvers(GenerationStep.Carving.AIR)) {
                    ConfiguredWorldCarver<?> c = h.value();
                    rnd.setLargeFeatureSeed(seed + (long) idx, sp.x, sp.z);
                    if (c.isStartChunk(rnd)) {
                        c.carve(ctx, chunk, bm::getBiome, rnd, aq, sp, mask);
                    }
                    idx++;
                }
            }
        }
    }
}

import net.minecraft.core.HolderLookup;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.levelgen.*;

final class WApi {
    static void boot() {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
    }

    static Object state(long seed, String dim) {
        ResourceKey<NoiseGeneratorSettings> key = NoiseGeneratorSettings.OVERWORLD;
        if (dim.equals("nether")) key = NoiseGeneratorSettings.NETHER;
        if (dim.equals("end")) key = NoiseGeneratorSettings.END;
        if (dim.equals("amplified")) key = NoiseGeneratorSettings.AMPLIFIED;
        if (dim.equals("large_biomes")) key = NoiseGeneratorSettings.LARGE_BIOMES;
        if (dim.equals("caves")) key = NoiseGeneratorSettings.CAVES;
        if (dim.equals("floating_islands")) key = NoiseGeneratorSettings.FLOATING_ISLANDS;
        HolderLookup.Provider prov = VanillaRegistries.createLookup();
        return RandomState.create(prov, key, seed);
    }

    static double field(Object rs, String name, int x, int y, int z) {
        NoiseRouter r = ((RandomState) rs).router();
        DensityFunction f = null;
        if (name.equals("temperature")) f = r.temperature();
        if (name.equals("vegetation")) f = r.vegetation();
        if (name.equals("continents")) f = r.continents();
        if (name.equals("erosion")) f = r.erosion();
        if (name.equals("depth")) f = r.depth();
        if (name.equals("ridges")) f = r.ridges();
        if (name.equals("final_density")) f = r.finalDensity();
        return f.compute(new DensityFunction.SinglePointContext(x, y, z));
    }
}

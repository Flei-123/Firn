package net.minecraft.world.level.levelgen;

import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

public class WgBlockFlags {
    private static void emit(PrintWriter out, String key, List<String> names, boolean last) {
        out.println("  \"" + key + "\": [");
        for (int i = 0; i < names.size(); i++) out.println("    \"" + names.get(i) + "\"" + (i + 1 < names.size() ? "," : ""));
        out.println(last ? "  ]" : "  ],");
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        List<String> all = new ArrayList<>();
        List<String> solid = new ArrayList<>();
        List<String> up = new ArrayList<>();
        List<String> down = new ArrayList<>();
        List<String> side = new ArrayList<>();
        for (Block b : BuiltInRegistries.BLOCK) {
            String name = BuiltInRegistries.BLOCK.getKey(b).getPath();
            BlockState s = b.defaultBlockState();
            all.add(name);
            if (s.isSolid()) solid.add(name);
            if (s.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, Direction.UP)) up.add(name);
            if (s.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, Direction.DOWN)) down.add(name);
            boolean sides = true;
            for (Direction d : Direction.Plane.HORIZONTAL) sides &= s.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, d);
            if (sides) side.add(name);
        }
        java.util.Collections.sort(all);
        java.util.Collections.sort(solid);
        java.util.Collections.sort(up);
        java.util.Collections.sort(down);
        java.util.Collections.sort(side);
        try (PrintWriter out = new PrintWriter(args[0])) {
            out.println("{");
            emit(out, "all", all, false);
            emit(out, "solid", solid, false);
            emit(out, "sturdy_up", up, false);
            emit(out, "sturdy_down", down, false);
            emit(out, "sturdy_side", side, true);
            out.println("}");
        }
    }
}

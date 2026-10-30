import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class ArgOracle {
    static ArgumentType<?> make(String spec) throws Exception {
        String[] p = spec.split(":", 2);
        String arg = p.length > 1 ? p[1] : "";
        switch (p[0]) {
            case "gamemode": return net.minecraft.commands.arguments.GameModeArgument.gameMode();
            case "time": return arg.isEmpty() ? net.minecraft.commands.arguments.TimeArgument.time() : net.minecraft.commands.arguments.TimeArgument.time(Integer.parseInt(arg));
            case "resource_location": return net.minecraft.commands.arguments.IdentifierArgument.id();
            case "int_range": return net.minecraft.commands.arguments.RangeArgument.intRange();
            case "block_pos": return net.minecraft.commands.arguments.coordinates.BlockPosArgument.blockPos();
            case "vec3": return net.minecraft.commands.arguments.coordinates.Vec3Argument.vec3();
            case "vec2": return net.minecraft.commands.arguments.coordinates.Vec2Argument.vec2();
            case "column_pos": return net.minecraft.commands.arguments.coordinates.ColumnPosArgument.columnPos();
            case "message": return net.minecraft.commands.arguments.MessageArgument.message();
            case "angle": return net.minecraft.commands.arguments.AngleArgument.angle();
            case "rotation": return net.minecraft.commands.arguments.coordinates.RotationArgument.rotation();
            default: throw new IllegalArgumentException(spec);
        }
    }

    static String describe(Object o) throws Exception {
        if (o instanceof net.minecraft.advancements.criterion.MinMaxBounds.Ints) {
            var b = ((net.minecraft.advancements.criterion.MinMaxBounds.Ints) o).bounds();
            return "min=" + b.min() + " max=" + b.max();
        }
        if (o instanceof net.minecraft.world.level.GameType) return ((net.minecraft.world.level.GameType) o).getName();
        if (o instanceof net.minecraft.commands.arguments.MessageArgument.Message) return "text=" + ((net.minecraft.commands.arguments.MessageArgument.Message) o).text();
        return String.valueOf(o);
    }

    static String esc(String s) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\') b.append("\\\\");
            else if (c == '\n') b.append("\\n");
            else if (c == '\t') b.append("\\t");
            else if (c == '\r') b.append("\\r");
            else b.append(c);
        }
        return b.toString();
    }

    public static void main(String[] args) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        System.setOut(real);
        List<String> lines = Files.readAllLines(Paths.get(args[0]), StandardCharsets.UTF_8);
        try (PrintStream out = new PrintStream(new BufferedOutputStream(new FileOutputStream(args[1])), false, "UTF-8")) {
            Map<String, ArgumentType<?>> cache = new HashMap<>();
            for (String line : lines) {
                int t = line.indexOf('\t');
                String spec = line.substring(0, t);
                String in = line.substring(t + 1).replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\");
                ArgumentType<?> at = cache.get(spec);
                if (at == null) { at = make(spec); cache.put(spec, at); }
                StringReader r = new StringReader(in);
                String res;
                try {
                    Object v = at.parse(r);
                    res = "OK " + esc(describe(v)) + " @" + r.getCursor();
                } catch (CommandSyntaxException e) {
                    res = "ERR " + esc(e.getMessage()) + " @" + e.getCursor();
                } catch (Throwable e) {
                    res = "EXC " + e.getClass().getSimpleName();
                }
                out.println(spec + "\t" + esc(in) + "\t" + res);
            }
        }
    }
}

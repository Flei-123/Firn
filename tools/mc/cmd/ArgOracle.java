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
            case "entity": {
                String[] f = arg.split(":");
                boolean single = f[0].equals("single");
                boolean players = f[1].equals("players");
                if (single) return players ? net.minecraft.commands.arguments.EntityArgument.player() : net.minecraft.commands.arguments.EntityArgument.entity();
                return players ? net.minecraft.commands.arguments.EntityArgument.players() : net.minecraft.commands.arguments.EntityArgument.entities();
            }
            case "game_profile": return net.minecraft.commands.arguments.GameProfileArgument.gameProfile();
            default: throw new IllegalArgumentException(spec);
        }
    }

    static String optText(Object o) throws Exception {
        Object v = o.getClass().getMethod("min").invoke(o);
        Object w = o.getClass().getMethod("max").invoke(o);
        return "[" + v + ".." + w + "]";
    }

    static String describeSelector(String in) throws Exception {
        var rd = new StringReader(in);
        var parser = new net.minecraft.commands.arguments.selector.EntitySelectorParser(rd, true);
        parser.parse();
        var cls = parser.getClass();
        TreeMap<String, String> m = new TreeMap<>();
        for (var f : cls.getDeclaredFields()) {
            String n = f.getName();
            if (n.equals("reader") || n.equals("allowSelectors") || n.equals("startPosition") || n.equals("suggestions") || java.lang.reflect.Modifier.isStatic(f.getModifiers())) continue;
            f.setAccessible(true);
            Object v = f.get(parser);
            String t;
            if (n.equals("predicates")) t = String.valueOf(((List<?>) v).size());
            else if (n.equals("order")) {
                t = "?";
                for (Class<?> oc : new Class<?>[] { net.minecraft.commands.arguments.selector.EntitySelector.class, cls }) {
                    for (var sf : oc.getDeclaredFields()) {
                        if (java.lang.reflect.Modifier.isStatic(sf.getModifiers()) && sf.getName().startsWith("ORDER_")) {
                            sf.setAccessible(true);
                            if (sf.get(null) == v) t = sf.getName().substring(6);
                        }
                    }
                }
            } else if (n.equals("type")) t = v == null ? "null" : net.minecraft.world.entity.EntityType.getKey((net.minecraft.world.entity.EntityType<?>) v).toString();
            else if (v != null && v.getClass().getName().contains("MinMaxBounds")) t = optText(v);
            else t = String.valueOf(v);
            m.put(n, t);
        }
        StringBuilder b = new StringBuilder();
        for (var e : m.entrySet()) b.append(e.getKey()).append('=').append(e.getValue()).append(' ');
        return b.toString().trim();
    }

    static String describe(Object o) throws Exception {
        if (o instanceof net.minecraft.commands.arguments.selector.EntitySelector) return "selector";
        if (o instanceof net.minecraft.commands.arguments.GameProfileArgument.Result) return "profile";
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
                    String d = describe(v);
                    if (d.equals("selector")) d = describeSelector(in);
                    if (d.equals("profile")) d = in.startsWith("@") ? "selector " + describeSelector(in) : "name " + in.substring(0, r.getCursor());
                    res = "OK " + esc(d) + " @" + r.getCursor();
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

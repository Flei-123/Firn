import com.mojang.brigadier.*;
import com.mojang.brigadier.arguments.*;
import com.mojang.brigadier.builder.*;
import com.mojang.brigadier.context.*;
import com.mojang.brigadier.exceptions.*;
import com.mojang.brigadier.suggestion.*;
import com.mojang.brigadier.tree.*;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class Oracle {
    static final StringBuilder out = new StringBuilder();
    static final Map<Integer, String[]> suggStrings = new HashMap<>();
    static final Map<Integer, int[]> suggInts = new HashMap<>();
    static final Map<String, CommandNode<Long>> labels = new HashMap<>();
    static CommandDispatcher<Long> dispatcher = new CommandDispatcher<>();

    static class N {
        int depth;
        boolean lit;
        String name, type;
        Map<String, String> kv = new LinkedHashMap<>();
        List<N> kids = new ArrayList<>();
    }

    static ArgumentType<?> type(String spec) {
        String[] p = spec.split(":");
        switch (p[0]) {
            case "bool": return BoolArgumentType.bool();
            case "int": return p.length == 3 ? IntegerArgumentType.integer(Integer.parseInt(p[1]), Integer.parseInt(p[2])) : IntegerArgumentType.integer();
            case "long": return p.length == 3 ? LongArgumentType.longArg(Long.parseLong(p[1]), Long.parseLong(p[2])) : LongArgumentType.longArg();
            case "float": return p.length == 3 ? FloatArgumentType.floatArg(Float.parseFloat(p[1]), Float.parseFloat(p[2])) : FloatArgumentType.floatArg();
            case "double": return p.length == 3 ? DoubleArgumentType.doubleArg(Double.parseDouble(p[1]), Double.parseDouble(p[2])) : DoubleArgumentType.doubleArg();
            case "word": return StringArgumentType.word();
            case "string": return StringArgumentType.string();
            case "greedy": return StringArgumentType.greedyString();
        }
        throw new IllegalArgumentException(spec);
    }

    static Command<Long> command(String spec) {
        if (spec.equals("err")) {
            return c -> { throw new SimpleCommandExceptionType(new LiteralMessage("boom")).create(); };
        }
        if (spec.equals("src")) {
            return c -> (int) (long) c.getSource();
        }
        int n = Integer.parseInt(spec);
        return c -> n;
    }

    static RedirectModifier<Long> modifier(String spec) {
        String[] p = spec.split(":");
        return c -> {
            long s = c.getSource();
            List<Long> r = new ArrayList<>();
            try {
                if (p[0].equals("add")) {
                    r.add(s + Long.parseLong(p[1]));
                } else if (p[0].equals("multi")) {
                    int k = Integer.parseInt(p[1]);
                    for (int i = 0; i < k; i++) r.add(s * 10 + i);
                } else if (p[0].equals("err")) {
                    throw new SimpleCommandExceptionType(new LiteralMessage("modboom")).create();
                } else if (p[0].equals("odd")) {
                    if (s % 2 != 0) throw new SimpleCommandExceptionType(new LiteralMessage("odd")).create();
                    r.add(s + 1);
                }
            } catch (CommandSyntaxException e) {
                out.append("M src=").append(s).append(" !").append(e.getMessage()).append('\n');
                throw e;
            }
            out.append("M src=").append(s).append(" ->");
            for (long x : r) out.append(' ').append(x);
            out.append('\n');
            return r;
        };
    }

    static SuggestionProvider<Long> provider(int id) {
        return (ctx, b) -> {
            String[] ss = suggStrings.get(id);
            if (ss != null) {
                for (String s : ss) if (s.toLowerCase(Locale.ROOT).startsWith(b.getRemainingLowerCase())) b.suggest(s);
            }
            int[] is = suggInts.get(id);
            if (is != null) {
                for (int v : is) if (Integer.toString(v).startsWith(b.getRemainingLowerCase())) b.suggest(v);
            }
            return b.buildFuture();
        };
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    static ArgumentBuilder<Long, ?> build(N n) {
        ArgumentBuilder b;
        if (n.lit) {
            b = LiteralArgumentBuilder.<Long>literal(n.name);
        } else {
            RequiredArgumentBuilder<Long, Object> r = RequiredArgumentBuilder.argument(n.name, (ArgumentType) type(n.type));
            if (n.kv.containsKey("sugg")) r.suggests((SuggestionProvider) provider(Integer.parseInt(n.kv.get("sugg"))));
            b = r;
        }
        if (n.kv.containsKey("req")) {
            long k = Long.parseLong(n.kv.get("req"));
            b.requires(s -> (Long) s >= k);
        }
        if (n.kv.containsKey("cmd")) b.executes(command(n.kv.get("cmd")));
        String target = n.kv.containsKey("redir") ? n.kv.get("redir") : n.kv.get("fork");
        if (target != null) {
            CommandNode<Long> t = target.equals("root") ? dispatcher.getRoot() : labels.get(target);
            RedirectModifier<Long> mod = n.kv.containsKey("mod") ? modifier(n.kv.get("mod")) : null;
            b.forward(t, mod, n.kv.containsKey("fork"));
        }
        for (N k : n.kids) b.then(build(k));
        return b;
    }

    static String range(StringRange r) {
        return r.getStart() + "-" + r.getEnd();
    }

    static String nodesText(List<ParsedCommandNode<Long>> nodes) {
        StringBuilder b = new StringBuilder();
        for (ParsedCommandNode<Long> p : nodes) {
            if (b.length() > 0) b.append(',');
            b.append(p.getNode().getName()).append('@').append(range(p.getRange()));
        }
        return b.toString();
    }

    static String argsOfContext(CommandContext<Long> c) {
        StringBuilder b = new StringBuilder();
        for (ParsedCommandNode<Long> p : c.getNodes()) {
            if (p.getNode() instanceof ArgumentCommandNode) {
                if (b.length() > 0) b.append(',');
                b.append(p.getNode().getName()).append('=').append(String.valueOf(c.getArgument(p.getNode().getName(), Object.class)));
            }
        }
        return b.toString();
    }

    static void printParse(ParseResults<Long> p) {
        List<String> msgs = new ArrayList<>();
        for (CommandSyntaxException e : p.getExceptions().values()) msgs.add(e.getMessage());
        Collections.sort(msgs);
        out.append("P read=").append(p.getReader().getCursor()).append('/').append(p.getReader().getTotalLength());
        out.append(" errs=").append(msgs.size());
        for (String m : msgs) out.append(" ; ").append(m);
        out.append('\n');
        CommandContextBuilder<Long> c = p.getContext();
        int level = 0;
        while (c != null) {
            out.append("L").append(level++).append(" src=").append(c.getSource()).append(" range=").append(range(c.getRange()));
            out.append(" nodes=").append(nodesText(c.getNodes()));
            StringBuilder a = new StringBuilder();
            for (Map.Entry<String, ParsedArgument<Long, ?>> e : c.getArguments().entrySet()) {
                if (a.length() > 0) a.append(',');
                a.append(e.getKey()).append('=').append(String.valueOf(e.getValue().getResult())).append('@').append(range(e.getValue().getRange()));
            }
            out.append(" args=").append(a);
            out.append(" cmd=").append(c.getCommand() != null).append('\n');
            c = c.getChild();
        }
    }

    static void printSuggestions(ParseResults<Long> p, int cursor, String input) {
        out.append("S@").append(cursor).append(' ');
        try {
            Suggestions s = dispatcher.getCompletionSuggestions(p, cursor).join();
            out.append(range(s.getRange()));
            for (Suggestion g : s.getList()) {
                out.append(" | ").append(g.getText()).append(" => ").append(g.apply(input.substring(0, cursor)));
            }
        } catch (Exception e) {
            out.append("ISE");
        }
        out.append('\n');
    }

    static List<Integer> cursors(int len) {
        List<Integer> r = new ArrayList<>();
        if (len <= 16) {
            for (int i = 0; i <= len; i++) r.add(i);
        } else {
            int[] c = {0, len / 4, len / 2, 3 * len / 4, len - 1, len};
            for (int i : c) r.add(i);
        }
        return r;
    }

    static void usage(long src, String pathText) {
        List<String> path = pathText.isEmpty() ? new ArrayList<>() : Arrays.asList(pathText.split(" "));
        out.append("U ").append(src).append(" path=").append(pathText).append('\n');
        CommandNode<Long> node = dispatcher.findNode(path);
        if (node == null) {
            out.append("none\n");
            return;
        }
        for (boolean restricted : new boolean[] {false, true}) {
            for (String u : dispatcher.getAllUsage(node, src, restricted)) out.append("A").append(restricted ? 1 : 0).append(": ").append(u).append('\n');
        }
        for (Map.Entry<CommandNode<Long>, String> e : dispatcher.getSmartUsage(node, src).entrySet()) {
            out.append("S: ").append(e.getKey().getUsageText()).append(" => ").append(e.getValue()).append('\n');
        }
    }

    static void run(String line) {
        if (line.startsWith("@usage ")) {
            String rest = line.substring(7);
            int sp = rest.indexOf(' ');
            long src = Long.parseLong(sp < 0 ? rest : rest.substring(0, sp));
            usage(src, sp < 0 ? "" : rest.substring(sp + 1));
            return;
        }
        int bar = line.indexOf('|');
        long src = Long.parseLong(line.substring(0, bar));
        String input = line.substring(bar + 1);
        out.append("## ").append(src).append('|').append(input).append('\n');
        ParseResults<Long> p = dispatcher.parse(input, src);
        printParse(p);
        try {
            int r = dispatcher.execute(p);
            out.append("R ").append(r).append('\n');
        } catch (CommandSyntaxException e) {
            out.append("E ").append(e.getMessage()).append(" | raw=").append(e.getRawMessage().getString()).append(" | cursor=").append(e.getCursor()).append('\n');
        }
        for (int c : cursors(input.length())) printSuggestions(p, c, input);
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        List<String> tree = Files.readAllLines(Path.of(args[0]), StandardCharsets.UTF_8);
        List<N> top = new ArrayList<>();
        Deque<N> stack = new ArrayDeque<>();
        for (String raw : tree) {
            if (raw.trim().isEmpty() || raw.trim().startsWith("#")) continue;
            int depth = 0;
            while (raw.startsWith("  ", depth * 2)) depth++;
            String[] t = raw.trim().split("\\s+");
            if (t[0].equals("sugglist") || t[0].equals("suggint")) {
                int id = Integer.parseInt(t[1]);
                String[] rest = Arrays.copyOfRange(t, 2, t.length);
                if (t[0].equals("sugglist")) {
                    suggStrings.put(id, rest);
                } else {
                    int[] v = new int[rest.length];
                    for (int i = 0; i < v.length; i++) v[i] = Integer.parseInt(rest[i]);
                    suggInts.put(id, v);
                }
                continue;
            }
            N n = new N();
            n.depth = depth;
            n.lit = t[0].equals("lit");
            n.name = t[1];
            int i = 2;
            if (!n.lit) n.type = t[i++];
            for (; i < t.length; i++) {
                int eq = t[i].indexOf('=');
                n.kv.put(t[i].substring(0, eq), t[i].substring(eq + 1));
            }
            while (stack.size() > depth) stack.pop();
            if (stack.isEmpty()) top.add(n);
            else stack.peek().kids.add(n);
            stack.push(n);
        }
        for (N n : top) {
            LiteralCommandNode<Long> node = dispatcher.register((LiteralArgumentBuilder<Long>) build(n));
            if (n.kv.containsKey("label")) labels.put(n.kv.get("label"), node);
        }
        dispatcher.setConsumer((c, ok, res) -> {
            out.append("C ok=").append(ok).append(" res=").append(res).append(" src=").append(c.getSource()).append(" range=").append(range(c.getRange()))
                .append(" forked=").append(c.isForked()).append(" nodes=").append(nodesText(c.getNodes())).append(" args=").append(argsOfContext(c)).append('\n');
        });
        for (String line : Files.readAllLines(Path.of(args[1]), StandardCharsets.UTF_8)) {
            if (line.isEmpty()) continue;
            run(line);
        }
        PrintStream ps = new PrintStream(Files.newOutputStream(Path.of(args[2])), false, "UTF-8");
        ps.print(out);
        ps.close();
    }
}

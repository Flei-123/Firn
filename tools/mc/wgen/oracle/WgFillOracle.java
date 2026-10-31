import java.io.PrintStream;

public class WgFillOracle {
    public static void main(String[] a) throws Exception {
        PrintStream out = new PrintStream(new java.io.FileOutputStream(a[0]));
        String dim = a[1];
        WApi.boot();
        for (int i = 2; i < a.length; i++) {
            String[] p = a[i].split(":");
            long seed = Long.parseLong(p[0]);
            int cx = Integer.parseInt(p[1]);
            int cz = Integer.parseInt(p[2]);
            for (String row : WApi.fill(seed, dim, cx, cz)) {
                int sp = row.indexOf(' ');
                out.println(seed + " " + cx + " " + cz + " " + row.substring(0, sp) + " " + row.substring(sp + 1));
            }
        }
        out.close();
    }
}

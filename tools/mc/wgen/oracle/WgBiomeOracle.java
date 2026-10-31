import java.io.PrintStream;

public class WgBiomeOracle {
    public static void main(String[] a) throws Exception {
        PrintStream out = new PrintStream(new java.io.FileOutputStream(a[0]));
        String dim = a[1];
        int qy0 = dim.equals("nether") ? 0 : -16;
        int qy1 = dim.equals("nether") ? 32 : 80;
        WApi.boot();
        for (int i = 2; i < a.length; i++) {
            String[] p = a[i].split(":");
            long seed = Long.parseLong(p[0]);
            int cx = Integer.parseInt(p[1]);
            int cz = Integer.parseInt(p[2]);
            for (String row : WApi.biomes(seed, dim, cx, cz, qy0, qy1)) {
                int sp = row.indexOf(' ');
                out.println(seed + " " + cx + " " + cz + " B " + row);
            }
        }
        out.close();
    }
}

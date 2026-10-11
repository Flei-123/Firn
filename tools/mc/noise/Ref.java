// Prints the java.util.Random lines of tools/mc/noise/dump.fi using the REAL java.util.Random.
import java.util.Random;
public class Ref {
    static String hex(double d) { return String.format("%016x", Double.doubleToRawLongBits(d)); }
    public static void main(String[] a) {
        long[] seeds = {0L, 42L, -1L, 123456789012345L, 9223372036854775807L};
        int[] bounds = {1, 2, 10, 100, 256, 1000, 1073741824, 2147483647};
        for (long s : seeds) {
            Random r = new Random(s);
            StringBuilder sb = new StringBuilder("jr " + s);
            for (int i = 0; i < 6; i++) sb.append(" ").append(r.nextInt());
            for (int b : bounds) sb.append(" b").append(b).append(":").append(r.nextInt(b));
            sb.append(" L").append(r.nextLong());
            sb.append(" D").append(hex(r.nextDouble()));
            sb.append(" B").append(r.nextBoolean());
            System.out.println(sb);
        }
    }
}

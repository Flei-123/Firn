import java.lang.reflect.*;
import java.util.*;
public class Ref {
  public static void main(String[] a) throws Exception {
    Class<?> lcg = Class.forName("aow");
    Method nx = lcg.getMethod("a", long.class, long.class);
    long[] seeds = {0L, 1L, -7046029254386353131L, 123456789012345L};
    for (long s : seeds) {
      long r = (Long)nx.invoke(null, s, 5L);
      long r2 = (Long)nx.invoke(null, r, -3L);
      System.out.println("LCG " + s + " 5 " + r + " -3 " + r2);
    }
    Class<?> bm = Class.forName("cnm");
    Method hash = bm.getMethod("a", long.class);
    for (long s : new long[]{0L, 1L, 12345L, -4172144997902289642L}) System.out.println("HASH " + s + " " + hash.invoke(null, s));
    Method fd = bm.getDeclaredMethod("a", long.class,int.class,int.class,int.class,double.class,double.class,double.class);
    fd.setAccessible(true);
    long hs = (Long)hash.invoke(null, 12345L);
    int[][] cs = {{0,0,0},{1,-2,3},{-5,7,-9},{100,-4,2000},{-1,-1,-1}};
    for (int[] c : cs) System.out.println("FID " + hs + " " + c[0]+" "+c[1]+" "+c[2]+" 0.25 0.5 0.75 " + fd.invoke(null, hs, c[0],c[1],c[2],0.25,0.5,0.75));
    // biome pick with proxy
    Class<?> iface = Class.forName("cnm$a");
    final int[] last = new int[3];
    Object px = Proxy.newProxyInstance(iface.getClassLoader(), new Class[]{iface}, (p,m,ar)->{ if (m.getName().equals("getNoiseBiome")) { last[0]=(Integer)ar[0]; last[1]=(Integer)ar[1]; last[2]=(Integer)ar[2]; } return null; });
    Constructor<?> ct = bm.getConstructor(iface, long.class);
    Method byXYZ = bm.getMethod("a", int.class,int.class,int.class);
    // BlockPos-based method a(gu): use b? use double variant is quart; find a(gu)
    Class<?> bp = Class.forName("gu");
    Method byPos = bm.getMethod("a", bp);
    Constructor<?> bpc = bp.getConstructor(int.class,int.class,int.class);
    long[] ws = {12345L, -99L, 4172144997902289642L};
    Random rnd = new Random(77);
    for (long w : ws) {
      Object mgr = ct.newInstance(px, (Long)hash.invoke(null, w));
      long hh = (Long)hash.invoke(null, w);
      for (int i=0;i<12;i++) {
        int x = rnd.nextInt(200)-100, y = rnd.nextInt(200)-60, z = rnd.nextInt(200)-100;
        byPos.invoke(mgr, bpc.newInstance(x,y,z));
        System.out.println("BIOME " + hh + " " + x+" "+y+" "+z+" -> "+last[0]+" "+last[1]+" "+last[2]);
      }
    }
    for (long w : new long[]{12345L, -99L}) {
      Object mgr = ct.newInstance(px, (Long)hash.invoke(null, w));
      long sum = 0; Random r3 = new Random(5);
      for (int i=0;i<4000;i++) {
        int x = r3.nextInt(4000)-2000, y = r3.nextInt(400)-80, z = r3.nextInt(4000)-2000;
        byPos.invoke(mgr, bpc.newInstance(x,y,z));
        sum = sum*31 + last[0]*1000003L + last[1]*1009L + last[2];
      }
      System.out.println("BSUM " + w + " " + sum);
    }
    Class<?> rs = Class.forName("dia");
    Object rnd2 = rs.getConstructor(long.class).newInstance(2345L);
    Class<?> nz = Class.forName("dwj");
    Constructor<?> nc = nz.getConstructor(Class.forName("apf"), List.class);
    Object noise = nc.newInstance(rnd2, Arrays.asList(0));
    Method gv = nz.getMethod("a", double.class,double.class,boolean.class);
    { double tot=0; int cnt=0; for (int gx=-200; gx<200; gx+=10) for (int gz=-200; gz<200; gz+=10) { double v=(Double)gv.invoke(noise, gx*0.0225, gz*0.0225, false); tot+=v; if (v < -0.1) cnt++; } System.out.println("SGRID " + tot + " " + cnt); }
    int[][] pts = {{0,0},{1,0},{10,10},{-37,52},{100,-200},{1234,-5678},{-300,-300},{7,3},{64,64},{-1,-1},{500,500},{-1000,777}};
    for (int[] p : pts) {
      double v = (Double)gv.invoke(noise, p[0]*0.0225, p[1]*0.0225, false);
      System.out.println("SWAMP " + p[0]+" "+p[1]+" "+v+" "+(v < -0.1 ? "4c763c" : "6a7039"));
    }
  }
}

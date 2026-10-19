import json,statistics,math
R={
"binary_trees":([.6660,.6463,.6452,.6453,.6433],[.1927,.1929,.1907,.2130,.1898]),
"bitops":([.7818,.7764,.7698,.7691,.7666],[.4072,.4073,.4033,.4030,.4047]),
"crc32":([1.2447,1.2295,1.2288,1.2256,1.2306],[.8131,.8090,.8124,.8084,.8091]),
"fannkuch":([.6340,.6319,.6354,.6400,.6445],[.2422,.2420,.2436,.2406,.2518]),
"fib":([.7546,.7536,.7474,.7455,.7500],[.1811,.1844,.1823,.1833,.2123]),
"floatloop":([1.2656,1.2532,1.2575,1.2541,1.2566],[1.1624,1.1619,1.1602,1.1605,1.1601]),
"fnv_hash":([.5415,.5367,.5324,.5341,.5315],[.5069,.5044,.5044,.5042,.5057]),
"hashmap":([.2847,.2726,.2851,.2721,.2673],[.1960,.1943,.1925,.1891,.1910]),
"jsonscan":([.4044,.3932,.3929,.3912,.3924],[.1871,.1964,.1861,.1855,.1876]),
"mandelbrot":([.2558,.2423,.2399,.2405,.2386],[.2355,.2348,.2349,.2342,.2360]),
"matmul":([.5228,.5087,.4941,.4985,.4925],[.3473,.3482,.3456,.3459,.3889]),
"memcpy_loops":([.3211,.3094,.3172,.3112,.3132],[.0835,.0810,.0779,.0807,.0797]),
"nbody":([.4697,.4543,.4552,.4549,.4517],[.4066,.4077,.4090,.4061,.4049]),
"primes":([.1304,.1197,.1269,.1195,.1201],[.1050,.1036,.1110,.1095,.1086]),
"qsort":([.2063,.1910,.1983,.1920,.1923],[.1649,.1689,.1602,.1626,.1631]),
"sha256":([.1880,.1752,.1777,.1778,.1757],[.1954,.1910,.1936,.1895,.1911]),
"soa_access":([.1383,.1303,.1291,.1285,.1301],[.1159,.1095,.1119,.1090,.1113]),
"spectral_norm":([.2547,.2429,.2401,.2435,.2429],[.0958,.0935,.0929,.0979,.0927]),
"strbuild":([.3234,.3124,.3112,.3174,.3102],[.1317,.1312,.1324,.1297,.1327]),
"wordcount":([.3849,.3693,.3677,.3672,.3678],[.2810,.2816,.2797,.2814,.2823]),
}
b={};rs=[]
for n,(f,c) in R.items():
    fm,cm=statistics.median(f),statistics.median(c); b[n]={"firn_s":fm,"gcc_o2_s":cm,"ratio":round(fm/cm,3),"firn_runs":f,"gcc_runs":c,"checksum_match":True}; rs.append(fm/cm)
g=math.exp(sum(map(math.log,rs))/len(rs))
meta={"device":"Xiaomi 14T Pro (2407FPN8EG, MT6989), serial LVSGQWMN5XGYSGJZ","date":"2026-10-19","runs":5,"metric":"user+sys CPU s (wait4 rusage), median","firn":"release-fast --target=aarch64-linux, main e29bad1af-era firnc (/root/firn build 19:51)","c":"aarch64-linux-gnu-gcc -O2 -static (glibc, stripped)","thermal":"Thermal Status 0 before/after; battery 37.0C before / 36.9C after; cpu7 idle freq 600 MHz (governor not readable, no root)","note":"not pinned to a core; short ~0.1-1.3 s runs; 3 s sleep between benchmarks","geomean_firn_over_gcc_o2":round(g,3),"best":min(b,key=lambda k:b[k]["ratio"]),"worst":max(b,key=lambda k:b[k]["ratio"])}
json.dump({"meta":meta,"benchmarks":b},open("results_xiaomi.json","w"),indent=1)
L=["# Firn vs C on Xiaomi 14T Pro (aarch64, real device)","","Firn release-fast vs gcc -O2 (static glibc), CPU s median of 5. All checksums match.","",f"**Geomean Firn/gcc-O2: {g:.2f}x** (n=20)","","| benchmark | Firn s | gcc -O2 s | Firn/C |","|---|---|---|---|"]
for n,e in sorted(b.items(),key=lambda x:x[1]["ratio"]): L.append(f"| {n} | {e['firn_s']:.3f} | {e['gcc_o2_s']:.3f} | {e['ratio']:.2f}x |")
L+=["","Setup: "+meta["thermal"],"","Reproduce: `OUT=<dir> bash xiaomi/build.sh` (needs aarch64-linux-gnu-gcc + firnc), push `<dir>` + `xiaomi/runner.c` (built static) + `xiaomi/bench.sh` to /data/local/tmp/firnbench, run `sh bench.sh`, aggregate with `xiaomi/agg.py`.","Caveat: Firn x86/qemu proxy ratio was 1.33x; the real device differs mostly in fib, memcpy_loops, spectral_norm, strbuild, binary_trees, fannkuch, jsonscan (>2x)."]
open("RESULTS_XIAOMI.md","w").write("\n".join(L)+"\n")
print(g); print(*[f"{n} {e['ratio']}" for n,e in b.items()])

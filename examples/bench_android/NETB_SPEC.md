# NETB spec (network benchmark of the bench apps)

Server files (generated, not in git): `nb_small.bin` 2048 octets, `nb_big.bin` 5242880 octets. Content: LCG `x = (x*1103515245 + 12345) & 0x7fffffff; byte = (x>>16) & 255`, seed 1 (small) / 2 (big).
FNV-1a 32 (same as the hash kernel) of the body: small = 125781171, big = 1517788501.

Run: ten GETs of nb_small.bin, each on a fresh connection (`Connection: close`, no pooling), then one GET of nb_big.bin.
ttfb = ms from request start (DNS, TCP, TLS 1.3, request) to the complete response head; total = ms to the complete body. The checksum is not part of the time.
Start: intent extras `--es tab lab --es lab net --ez autorun true --es netbase https://host/dir/` (Firn: netb.fi, Kotlin: LabScreen.kt NetBench).
Log (tag BENCH): `netb small i=N ...`, `netb big ...`, `netb_sum ...` (Kotlin: two passes, variant=sys default stack, variant=cold new SSLContext per request).
Host test: `NETB_BASE=https://host/dir/ ./netb_main` (examples/bench_android/netb_main.fi, no window).
Harness: bench/android_measure/netenergy.py (phase netb / energy).

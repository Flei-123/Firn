# tools/stack_e/host_attach.py -- G1b from ctypes: 4 Python threads call into the Firn .so (ctypes drops the GIL).
import ctypes as C, sys, threading
l = C.CDLL(sys.argv[1])
for n in ("se_attach", "se_detach", "se_threads", "se_gc_probe_h", "se_gc_rounds_h", "se_jobs_probe_h"):
    getattr(l, n).restype = C.c_uint64
l.se_gc_probe_h.argtypes = [C.c_uint64]
l.se_gc_rounds_h.argtypes = [C.c_uint64, C.c_uint64]
l.se_jobs_probe_h.argtypes = [C.c_void_p, C.c_uint64, C.c_uint64]
bad = []
def work(i):
    if l.se_attach() != 1: bad.append(("attach", i)); return
    buf = (C.c_uint32 * 8192)()
    for rep in range(3):
        for w in (0, 3):
            r = l.se_jobs_probe_h(buf, 4096, w)
            if r != 4096 or buf[100] != 300 or buf[4095] != 12285: bad.append(("jobs", i, r))
        r = l.se_gc_probe_h(i % 3)
        if r != 400 * (i % 3 + 1): bad.append(("gc", i, r))
        r = l.se_gc_rounds_h(i * 1000 + 7, 300)
        if r != 300: bad.append(("rounds", i, r))
    if l.se_detach() != 1: bad.append(("detach", i))
assert l.se_attach() == 1               # the main thread becomes the Firn main thread
for wave in range(2):
    ts = [threading.Thread(target=work, args=(i,)) for i in range(4)]
    for t in ts: t.start()
    for t in ts: t.join()
    n = l.se_threads()
    if n != 1: bad.append(("count", wave, n))
print("ok  host_attach.py" if not bad else "FAIL host_attach.py %s" % bad[:5])
sys.exit(1 if bad else 0)

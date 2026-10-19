import random, sys
seeds=[0,1,42,2**32,2**64-1,123456789012,31337]
KS=[1,7,31,32,33,53,64]
def arr(xs): return "["+", ".join(str(x) for x in xs)+"]"
def farr(xs): return "["+", ".join(repr(x) for x in xs)+"]"
out=[]
for si,s in enumerate(seeds):
    r=random.Random(s)
    f=[r.random() for _ in range(3)]
    bits=[r.getrandbits(k) for k in KS]
    ints=[r.randint(-5,5) for _ in range(3)]+[r.randrange(0,1000000007) for _ in range(2)]+[r.randint(0,2**40)]
    shuf=list(range(20)); r.shuffle(shuf)
    ch=[r.choice(range(100)) for _ in range(2)]
    sa=r.sample(range(1000),30)
    sb=r.sample(range(30),7)
    sc=r.sample(range(10),3)
    by=list(r.randbytes(7))
    g=[r.gauss(0,1) for _ in range(3)]
    ex=r.expovariate(2.5)
    un=r.uniform(2.0,5.0)
    nxt=r.getrandbits(32)
    out.append(f"""    {{
        var r: rng.Rng = rng.new({s})
        var i: usize = 0
        let f: [f64; 3] = {farr(f)}
        var ok: bool = true
        while i < 3 {{ if rng.random(&r) != f[i] {{ ok = false }}; i = i + 1 }}
        testkit.check(ok, "seed {s}: random()")
        let ks: [u32; 7] = {arr(KS)}
        let bits: [u64; 7] = {arr(bits)}
        ok = true
        i = 0
        while i < 7 {{ if rng.getrandbits(&r, ks[i]) != bits[i] {{ ok = false }}; i = i + 1 }}
        testkit.check(ok, "seed {s}: getrandbits")
        let ints: [i64; 6] = {arr(ints)}
        ok = rng.randint(&r, -5, 5) == ints[0] && rng.randint(&r, -5, 5) == ints[1] && rng.randint(&r, -5, 5) == ints[2]
        ok = ok && rng.randrange(&r, 0, 1000000007) == ints[3] && rng.randrange(&r, 0, 1000000007) == ints[4]
        ok = ok && rng.randint(&r, 0, 1099511627776) == ints[5]
        testkit.check(ok, "seed {s}: randint / randrange")
        var v: Vec[i64] = vec_new[i64]()
        i = 0
        while i < 20 {{ vec_push[i64](&v, i as i64); i = i + 1 }}
        rng_shuffle[i64](&r, &v)
        let shw: [i64; 20] = {arr(shuf)}
        testkit.check(same(&v, (&shw[0]) as u64, 20), "seed {s}: shuffle")
        var src: Vec[i64] = vec_new[i64]()
        i = 0
        while i < 100 {{ vec_push[i64](&src, i as i64); i = i + 1 }}
        let c0: i64 = *rng_choice[i64](&r, &src)
        let c1: i64 = *rng_choice[i64](&r, &src)
        testkit.check(c0 == {ch[0]} && c1 == {ch[1]}, "seed {s}: choice")
        var big: Vec[i64] = vec_new[i64]()
        i = 0
        while i < 1000 {{ vec_push[i64](&big, i as i64); i = i + 1 }}
        var o: Vec[i64] = vec_new[i64]()
        rng_sample[i64](&r, &big, 30, &o)
        let saw: [i64; 30] = {arr(sa)}
        testkit.check(same(&o, (&saw[0]) as u64, 30), "seed {s}: sample (set branch)")
        var mid: Vec[i64] = vec_new[i64]()
        i = 0
        while i < 30 {{ vec_push[i64](&mid, i as i64); i = i + 1 }}
        var o2: Vec[i64] = vec_new[i64]()
        rng_sample[i64](&r, &mid, 7, &o2)
        let sbw: [i64; 7] = {arr(sb)}
        testkit.check(same(&o2, (&sbw[0]) as u64, 7), "seed {s}: sample (pool branch)")
        var sm: Vec[i64] = vec_new[i64]()
        i = 0
        while i < 10 {{ vec_push[i64](&sm, i as i64); i = i + 1 }}
        var o3: Vec[i64] = vec_new[i64]()
        rng_sample[i64](&r, &sm, 3, &o3)
        let scw: [i64; 3] = {arr(sc)}
        testkit.check(same(&o3, (&scw[0]) as u64, 3), "seed {s}: sample small")
        var bb: [u8; 7] = [0; 7]
        rng.randbytes(&r, (&bb[0]) as u64, 7)
        let wb: [u8; 7] = {arr(by)}
        ok = true
        i = 0
        while i < 7 {{ if bb[i] != wb[i] {{ ok = false }}; i = i + 1 }}
        testkit.check(ok, "seed {s}: randbytes(7)")
        let g: [f64; 3] = {farr(g)}
        ok = near(rng.gauss(&r, 0.0, 1.0), g[0]) && near(rng.gauss(&r, 0.0, 1.0), g[1]) && near(rng.gauss(&r, 0.0, 1.0), g[2])
        testkit.check(ok, "seed {s}: gauss x3")
        testkit.check(near(rng.expovariate(&r, 2.5), {ex!r}), "seed {s}: expovariate")
        testkit.check(rng.uniform(&r, 2.0, 5.0) == {un!r}, "seed {s}: uniform")
        testkit.check(rng.next_u32(&r) as u64 == {nxt}, "seed {s}: stream still in sync")
        vec_free[i64](&v); vec_free[i64](&src); vec_free[i64](&big); vec_free[i64](&o); vec_free[i64](&mid); vec_free[i64](&o2); vec_free[i64](&sm); vec_free[i64](&o3)
    }}""")
print("\n".join(out))

#!/usr/bin/env bash
set -euo pipefail
WT=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$1; CX=$2; CZ=$3; NX=$4; NZ=$5; ST=$6; IX=$7
L=$(mktemp -d)
trap 'rm -rf "$L"' EXIT
cp -r "$WT/lib/." "$L/"
python3 -I - "$L" "$CX" "$CZ" "$NX" "$NZ" "$ST" "$IX" <<'EOF'
import sys
L,cx,cz,nx,nz,st,ix=sys.argv[1:8]
p=L+'/mc/gen/mg_random.fi'
s=open(p).read()
s=s.replace("import std.math\n","import std.io\nimport std.math\n",1)
s=s.replace("    gauss: f64,\n}","    gauss: f64,\n    dbg: bool,\n    dbg_idx: i32,\n}",1)
s=s.replace("has_gauss: false, gauss: 0.0,\n    }","has_gauss: false, gauss: 0.0, dbg: false, dbg_idx: 0,\n    }",1)
s=s.replace("    return (v >> ((64 - bits) as u32)) as% i32\n","""    let out: i32 = (v >> ((64 - bits) as u32)) as% i32
    if (*r).dbg {
        let di: i32 = (*r).dbg_idx
        io.fmt_eprint_line(f"D {di} {bits} {out}")
        (*r).dbg_idx = di + 1
    }
    return out
""",1)
open(p,'w').write(s)
p=L+'/mc/gen/wgft_region.fi'
s=open(p).read()
s=s.replace("    veg: u64,\n}","    veg: u64,\n    dbg: bool,\n}",1)
s=s.replace("""    if a == 0 { return }
    *(a as *mut u16) = code
""","""    if a == 0 { return }
    if (*r).dbg {
        let nm: str = wgtag.code_name((*(*r).ft).sd as *wgsr_def.SrDef, code as i32)
        io.fmt_eprint_line(f"SET {x} {y} {z} {nm}")
    }
    *(a as *mut u16) = code
""",1)
s=s.replace("\nimport ","\nimport std.io\nimport mc.gen.wgtag\nimport mc.gen.wgsr_def\nimport ",1)
open(p,'w').write(s)
p=L+'/mc/gen/wgft_run.fi'
s=open(p).read()
s=s.replace("import mc.gen.mg_random\n","import std.io\nimport mc.gen.mg_random\n",1)
s=s.replace("                    wgft_place.run_feature(&c, nx * 16, (*r).min_y, nz * 16)\n","""                    rng.dbg = cx == %s && cz == %s && nx == %s && nz == %s && (*f).step == %s && (*f).index == %s
                    (*r).dbg = rng.dbg
                    rng.dbg_idx = 0
                    wgft_place.run_feature(&c, nx * 16, (*r).min_y, nz * 16)
                    rng.dbg = false
                    (*r).dbg = false
""" % (cx,cz,nx,nz,st,ix),1)
open(p,'w').write(s)
EOF
cd "$WT"
FIRNLIB="$L" /root/firn/compiler/target/release/firnc -o "$OUT" tools/mc/wgen/surface_probe.fi

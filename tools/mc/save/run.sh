#!/usr/bin/env bash
# tools/mc/save/run.sh -- world save formats (NBT, Anvil, chunk codec) against REAL vanilla worlds.
#   worlds   vanilla 1.20.1 and 26.1 (zlib) and 26.1 with region-file-compression=lz4, generated headless
#            (seed fixed, online-mode=false, ~400 forceloaded chunks each) and cached in $SAVE_CACHE
#   A  Firn reads EVERY nbt file and chunk (level.dat, data/*.dat, playerdata, region/entities/poi), writes each tree back
#      byte-identically, and its canonical digest per tree equals an independent python reader's
#   B  chunk codec: per section block/biome palette+BitStorage digests, light sums, heightmaps equal python;
#      re-packing the decoded indices gives the original long array
#   C  vanilla oracle: random block positions answered by the real server (`execute if block`) equal Firn and python
#      (plus negative controls that must fail)
#   D  Firn rewrites the whole world (other compression, densely packed): digests unchanged, python parses the files,
#      vanilla starts on it without chunk warnings, sample blocks and the player position survive
#   E  fuzz (damaged chunk data and LZ4 streams: no crash, no hang) and counter-check (one flipped payload octet is seen)
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$REPO/lib"
CACHE=${SAVE_CACHE:-/root/.w5/savetest}
JARS=${MC_JARS:-/root/.w5/jars}
JAVA17=java
JAVA25=${JAVA25:-/root/tools/jdk25/bin/java}
HEAVY=/root/jarvis/bin/heavy
TOOLS="$REPO/tools/mc/save"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/res"
PICKS=${SAVE_PICKS:-120}

worlds="v1201 v261 v261lz4"
jar_of()    { case $1 in v1201) echo "$JARS/server-1.20.1.jar";; *) echo "$JARS/server-26.1.jar";; esac; }
java_of()   { case $1 in v1201) echo $JAVA17;; *) echo $JAVA25;; esac; }
region_of() { case $1 in v1201) echo region;; *) echo dimensions/minecraft/overworld/region;; esac; }
types_of()  { case $1 in v1201) echo "2 1";; v261) echo "4 3";; *) echo "2";; esac; }
port_of()   { case $1 in v1201) echo 25810;; v261) echo 25820;; *) echo 25830;; esac; }

$FIRNC -o "$W/prog" tools/mc/save/prog.fi 2>"$W/err" || { echo "  FAIL  compile"; head -5 "$W/err"; exit 1; }

gen() {
    name=$1
    [ -d "$CACHE/$name/world" ] && return 0
    mkdir -p "$CACHE"
    extra=""
    [ "$name" = v261lz4 ] && extra="--compression lz4"
    [ "$name" = v1201 ] && extra="--bot $TOOLS/join_bot.cjs"
    $HEAVY python3 -I "$TOOLS/gen_world.py" --jar "$(jar_of $name)" --java "$(java_of $name)" --out "$CACHE/$name" --port "$(port_of $name)" $extra >/dev/null 2>&1
    if [ "$name" = v1201 ] && [ -z "$(ls "$CACHE/v1201/world/playerdata" 2>/dev/null)" ]; then
        $HEAVY python3 -I "$TOOLS/gen_world.py" --jar "$(jar_of $name)" --java "$(java_of $name)" --out "$CACHE/$name" --port "$(( $(port_of $name) + 1 ))" --bot "$TOOLS/join_bot.cjs" --bot-only >/dev/null 2>&1
    fi
}

say() { echo "$1" >> "$W/res/$2.txt"; }
okl() { say "  ok    $1" "$2"; }
fail() { say "  FAIL  $1" "$2"; touch "$W/failed"; }

cmp_lines() {
    # $1 firn file  $2 python file  $3 label  $4 world
    grep -v '^#' "$1" | sort > "$1.s"
    n=$(diff "$1.s" "$2" | wc -l)
    if [ "$n" = 0 ] && [ -s "$2" ]; then okl "$3: $(wc -l < "$2") lines identical to the python reader" "$4"; else fail "$3: $n differing lines" "$4"; diff "$1.s" "$2" | head -3 >> "$W/res/$4.txt"; fi
}

oracle() {
    # $1 world dir (contains world/)  $2 name  $3 label  $4 region dir (relative to world/)  $5 want player (1/0)
    d="$W/or_$2_$3"
    rm -rf "$d"; mkdir -p "$d"
    cp -r "$1/world" "$d/world"
    botarg=""
    [ "$5" = 1 ] && botarg="--bot $TOOLS/join_bot.cjs"
    p=$(( $(port_of $2) + RANDOM % 8 + 2 ))
    python3 -I "$TOOLS/oracle.py" --jar "$(jar_of $2)" --java "$(java_of $2)" --dir "$d" --port $p --picks "$W/picks_$2.txt" --out "$d.res" --wrap "$HEAVY" $botarg > "$d.log" 2>&1
    rc=$?
    if [ $rc = 0 ]; then
        okl "$3: vanilla answers $(grep -o 'picks=[0-9]*' "$d.log") block checks as expected; $(grep -o 'log_chunk_problems=[0-9]*' "$d.log")" "$2"
        if [ "$5" = 1 ]; then
            if grep -q 'player_pos=100.5,[0-9.-]*,-50.5' "$d.log"; then okl "$3: player position x/z from playerdata survived ($(grep -o 'player_pos=.*' "$d.log"))" "$2"; else fail "$3: player position lost ($(grep -o 'player_pos=.*' "$d.log"))" "$2"; fi
        fi
    else
        fail "$3: vanilla oracle rc=$rc" "$2"; head -6 "$d.log" >> "$W/res/$2.txt"
    fi
    rm -rf "$d"
}

run_world() {
    name=$1
    base="$CACHE/$name"
    rdir=$(region_of $name)
    gen $name
    [ -d "$base/world" ] || { fail "world $name missing" $name; return; }
    "$W/prog" tree "$base/world" > "$W/$name.t_fi" 2>"$W/$name.err"; rc=$?
    tail -1 "$W/$name.t_fi" | grep -q '^#' && okl "A: Firn read+rewrote every tree byte-identically ($(tail -1 "$W/$name.t_fi" | cut -c3-))" $name
    [ $rc = 0 ] || fail "A: Firn tree rc=$rc" $name
    "$W/prog" blocks "$base/world" > "$W/$name.b_fi" 2>>"$W/$name.err"; rc=$?
    [ $rc = 0 ] && okl "B: every section re-packs to the original long array ($(grep -c '^S' "$W/$name.b_fi") sections)" $name || fail "B: Firn blocks rc=$rc" $name
    python3 -I "$TOOLS/ref.py" tree "$base/world" > "$W/$name.t_py" 2>>"$W/$name.err" &
    python3 -I "$TOOLS/ref.py" blocks "$base/world" > "$W/$name.b_py" 2>>"$W/$name.err" &
    python3 -I "$TOOLS/ref.py" picks "$base/world/$rdir" $PICKS 7 -120 120 > "$W/picks_$name.txt" 2>>"$W/$name.err"
    wait
    cmp_lines "$W/$name.t_fi" "$W/$name.t_py" "A: tree digests" $name
    cmp_lines "$W/$name.b_fi" "$W/$name.b_py" "B: chunk codec digests" $name
    "$W/prog" pick "$base/world/$rdir" "$W/picks_$name.txt" > "$W/$name.p_fi" 2>/dev/null
    if cmp -s "$W/$name.p_fi" "$W/picks_$name.txt"; then okl "C: Firn block names at $(wc -l < "$W/picks_$name.txt") positions equal python" $name; else fail "C: Firn picks differ" $name; fi
    wantp=0; [ $name = v1201 ] && wantp=1
    oracle "$base" $name original "$rdir" $wantp &
    for ct in $(types_of $name); do
        rd="$W/rw_${name}_$ct"
        mkdir -p "$rd"
        "$W/prog" rewrite "$base/world" "$rd/world" $ct; rc=$?
        [ $rc = 0 ] || { fail "D: rewrite ctype $ct rc=$rc" $name; continue; }
        "$W/prog" tree "$rd/world" > "$W/$name.t_rw$ct" 2>>"$W/$name.err"
        grep -v '^#' "$W/$name.t_rw$ct" | sort > "$W/$name.t_rw$ct.s"
        n=$(diff "$W/$name.t_fi.s" "$W/$name.t_rw$ct.s" | wc -l)
        [ "$n" = 0 ] && okl "D: world rewritten with compression $ct: all $(wc -l < "$W/$name.t_rw$ct.s") tree digests unchanged" $name || fail "D: rewrite ctype $ct changed $n digests" $name
        if [ "$ct" = "$(types_of $name | cut -d' ' -f1)" ]; then
            python3 -I "$TOOLS/ref.py" tree "$rd/world" > "$W/$name.t_rwpy$ct" 2>>"$W/$name.err"
            cmp_lines "$W/$name.t_rw$ct" "$W/$name.t_rwpy$ct" "D: python parses the Firn-written world (compression $ct)" $name
        fi
        oracle "$rd" $name "rewritten-$ct" "$rdir" $wantp &
    done
    wait
}

for w in $worlds; do gen $w; done
for w in $worlds; do run_world $w & done
wait

for w in $worlds; do
    echo "== $w"
    cat "$W/res/$w.txt" 2>/dev/null
done

echo "== E fuzz + counter-check"
b="$CACHE/v1201/world"
timeout 300 "$W/prog" fuzz "$b" 300 5 > "$W/fuzz.txt" 2>&1; rc=$?
if [ $rc = 0 ]; then echo "  ok    E: $(cat "$W/fuzz.txt")"; else echo "  FAIL  E: fuzz rc=$rc"; touch "$W/failed"; fi
mkdir -p "$W/cc"
"$W/prog" rewrite "$b" "$W/cc/world" 3 >/dev/null
python3 -I - "$W/cc/world/region" <<'PY'
import glob, struct, sys
f = sorted(glob.glob(sys.argv[1] + "/r.*.mca"))[0]
raw = bytearray(open(f, "rb").read())
for s in range(1024):
    e = struct.unpack(">I", raw[s * 4:s * 4 + 4])[0]
    if e:
        off = e >> 8
        ln = struct.unpack(">I", raw[off * 4096:off * 4096 + 4])[0]
        raw[off * 4096 + 5 + ln // 2] ^= 0x10
        break
open(f, "wb").write(raw)
PY
"$W/prog" tree "$W/cc/world" 2>/dev/null | grep -v '^#' | sort > "$W/cc.s"
n=$(diff "$W/v1201.t_fi.s" "$W/cc.s" | wc -l)
if [ "$n" -gt 0 ]; then echo "  ok    E: counter-check: one flipped payload octet changes $n digest lines"; else echo "  FAIL  E: counter-check did not notice the damage"; touch "$W/failed"; fi

echo "== G external .mcc chunks"
"$W/prog" mkmcc "$W/mcc/src" && "$W/prog" rewrite "$W/mcc/src" "$W/mcc/dst" 4 && "$W/prog" rewrite "$W/mcc/dst" "$W/mcc/dst2" 2
for d in src dst dst2; do "$W/prog" tree "$W/mcc/$d" | grep -v '^#' | sort > "$W/mcc/$d.f"; done
python3 -I "$TOOLS/ref.py" tree "$W/mcc/dst" > "$W/mcc/dst.p" 2>/dev/null
if [ -s "$W/mcc/src.f" ] && cmp -s "$W/mcc/src.f" "$W/mcc/dst.f" && cmp -s "$W/mcc/src.f" "$W/mcc/dst2.f" && cmp -s "$W/mcc/dst.f" "$W/mcc/dst.p" && [ -f "$W/mcc/dst/region/c.0.0.mcc" ]; then echo "  ok    G: 1.3 MB chunk in .mcc: zlib -> lz4 -> zlib, python reads the lz4 .mcc, digests unchanged"; else echo "  FAIL  G: .mcc round trip"; touch "$W/failed"; fi

echo "== F speed (v1201 world, parse of every region chunk)"
s=$(date +%s%N)
"$W/prog" bench "$CACHE/v1201/world" > "$W/bench.txt" 2>&1
e=$(date +%s%N)
python3 -I - "$W/bench.txt" $(( (e - s) / 1000000 )) <<'PY'
import re, sys
t = open(sys.argv[1]).read()
b = int(re.search(r"nbt_bytes=(\d+)", t).group(1)); c = int(re.search(r"chunks=(\d+)", t).group(1)); ms = int(sys.argv[2])
print("  info  read+inflate+parse: %.1f MB/s of NBT, %.0f chunks/s (%d chunks, %d ms wall)" % (b / 1e6 / (ms / 1000), c / (ms / 1000), c, ms))
PY

if [ -e "$W/failed" ]; then echo "SAVE: FAIL"; exit 1; fi
echo "SAVE: ok"

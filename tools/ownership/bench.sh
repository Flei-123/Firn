#!/bin/bash
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
source "$HERE/targets.sh"
OUT=${OWN_BENCH_OUT:-$OWN_ROOT/.own-bench}
RUNS=${RUNS:-3}
MODES=${MODES:-"off on"}
TARGETS=${TARGETS:-$(target_list)}
CG_TARGETS=${CG_TARGETS:-"binary_trees openplan_derive db_bench"}
GC_DUMP=${GC_DUMP:-1}
while [ $# -gt 0 ]; do
  case $1 in
    --out) OUT=$2; shift 2 ;;
    --runs) RUNS=$2; shift 2 ;;
    --targets) TARGETS=$2; shift 2 ;;
    --cg) CG_TARGETS=$2; shift 2 ;;
    --modes) MODES=$2; shift 2 ;;
    --table) python3 -I "$HERE/bench_table.py" "$OUT/bench.tsv"; exit $? ;;
    *) echo "usage: bench.sh [--out DIR] [--runs N] [--targets 'a b'] [--cg 'a b'] [--modes 'off on'] [--table]" >&2; exit 2 ;;
  esac
done
mkdir -p "$OUT"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
TSV=$OUT/bench.tsv
printf 'target\tmode\tbuild_rc\trun_rc\tir\tuser_s\trss_kb\tgc_runs\tgc_pause_max_ns\tgc_pause_total_ns\tgc_alloc_bytes\tout_md5\tdeterministic\n' > "$TSV"

median() { sort -n | awk '{a[NR]=$1} END {if (NR==0) print 0; else print a[int((NR+1)/2)]}'; }

for t in $TARGETS; do
  det=0
  target_deterministic "$t" && det=1
  for m in $MODES; do
    mkdir -p "$WORK/$t/$m"
    if target_build "$t" "$m" "$WORK/$t/$m" > "$WORK/$t/$m/build.log" 2>&1; then echo 0 > "$WORK/$t/$m/build.rc"; else echo 1 > "$WORK/$t/$m/build.rc"; sed -n 1,5p "$WORK/$t/$m/build.log" >&2; fi
  done
  for i in $(seq 1 "$RUNS"); do
    for m in $MODES; do
      [ "$(cat "$WORK/$t/$m/build.rc")" = 0 ] || continue
      /usr/bin/time -o "$WORK/$t/$m/time.$i" -f '%U %M' bash -c "source '$HERE/targets.sh'; target_run '$t' '$WORK/$t/$m' '$WORK/$t/$m/out.$i'" > /dev/null 2> "$WORK/$t/$m/err.$i"
      echo $? > "$WORK/$t/$m/rc.$i"
    done
  done
  for m in $MODES; do
    d=$WORK/$t/$m
    brc=$(cat "$d/build.rc")
    if [ "$brc" != 0 ]; then
      printf '%s\t%s\t1\t-\t0\t0\t0\t0\t0\t0\t0\t-\t%s\n' "$t" "$m" "$det" >> "$TSV"
      continue
    fi
    rc=$(cat "$d/rc.1")
    user=$(for i in $(seq 1 "$RUNS"); do tail -n 1 "$d/time.$i" | awk '{print $1}'; done | median)
    rss=$(for i in $(seq 1 "$RUNS"); do tail -n 1 "$d/time.$i" | awk '{print $2}'; done | median)
    md5=-
    [ "$det" = 1 ] && md5=$(md5sum < "$d/out.1" | cut -c1-12)
    ir=0
    case " $CG_TARGETS " in
      *" $t "*)
        cgout=$(bash -c "source '$HERE/targets.sh'; target_run '$t' '$d' '$d/out.cg' valgrind --tool=callgrind --callgrind-out-file='$d/cg.out'" 2>&1 | grep -E 'Collected' | tail -n 1)
        ir=$(echo "$cgout" | sed -E 's/.*Collected *: *([0-9]+).*/\1/')
        [ -n "$ir" ] || ir=0 ;;
    esac
    gc="gc_runs=0 gc_pause_max_ns=0 gc_pause_total_ns=0 gc_alloc_bytes=0"
    [ "$GC_DUMP" = 1 ] && [ "$t" != openplan_derive ] && gc=$(bash "$HERE/gcdump.sh" "$d/bin" 2>/dev/null)
    if [ "$GC_DUMP" = 1 ] && [ "$t" = openplan_derive ]; then
      rm -rf "$d/proj"; cp -r "$OP_PROJ" "$d/proj"
      gc=$(bash "$HERE/gcdump.sh" "$d/bin" derive "$d/proj" 2>/dev/null)
    fi
    g_runs=$(echo "$gc" | sed -E 's/.*gc_runs=([0-9]+).*/\1/')
    g_max=$(echo "$gc" | sed -E 's/.*gc_pause_max_ns=([0-9]+).*/\1/')
    g_tot=$(echo "$gc" | sed -E 's/.*gc_pause_total_ns=([0-9]+).*/\1/')
    g_al=$(echo "$gc" | sed -E 's/.*gc_alloc_bytes=([0-9]+).*/\1/')
    printf '%s\t%s\t0\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$m" "$rc" "$ir" "$user" "$rss" "$g_runs" "$g_max" "$g_tot" "$g_al" "$md5" "$det" >> "$TSV"
  done
  echo "done $t $(date +%T)" >&2
done
python3 -I "$HERE/bench_table.py" "$TSV" | tee "$OUT/bench.md"

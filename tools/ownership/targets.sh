#!/bin/bash
OWN_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
FIRNC=${FIRNC:-$OWN_ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$OWN_ROOT/lib}
OP_DIR=${OP_DIR:-/root/projects/openplan}
OP_PROJ=${OP_PROJ:-/root/jarvis/data/staff_scratch/b3own/proj300}
FVC_NAMES="binary_trees bitops crc32 fannkuch fib floatloop fnv_hash hashmap jsonscan mandelbrot matmul memcpy_loops nbody primes qsort sha256 soa_access spectral_norm strbuild wordcount"

target_list() {
  echo "binary_trees openplan_derive db_bench"
  for n in $FVC_NAMES; do echo -n "fvc_$n "; done
  echo
}

target_deterministic() {
  case $1 in
    db_bench) return 1 ;;
    *) return 0 ;;
  esac
}

target_build() {
  local n=$1 mode=$2 d=$3
  mkdir -p "$d"
  case $n in
    binary_trees) "$FIRNC" --no-cache --ownership="$mode" --opt-level=release-fast "$OWN_ROOT/bench/firn_vs_c/firn/binary_trees.fi" -o "$d/bin" ;;
    fvc_*) "$FIRNC" --no-cache --ownership="$mode" --opt-level=release-fast "$OWN_ROOT/bench/firn_vs_c/firn/${n#fvc_}.fi" -o "$d/bin" ;;
    db_bench) "$FIRNC" --no-cache --ownership="$mode" --opt-level=release-fast "$OWN_ROOT/tools/db/bench.fi" -o "$d/bin" ;;
    openplan_derive) (cd "$OP_DIR" && FIRN_USE_AS=1 "$FIRNC" --no-cache --ownership="$mode" --opt-level=release-safe --package src/cli -o "$d/bin") ;;
    *) echo "unknown target $n" >&2; return 2 ;;
  esac
}

target_run() {
  local n=$1 d=$2 out=$3
  shift 3
  case $n in
    openplan_derive)
      rm -rf "$d/proj"
      cp -r "$OP_PROJ" "$d/proj"
      "$@" "$d/bin" derive "$d/proj" > "$out"
      local rc=$?
      [ $rc -eq 0 ] && (cd "$d/proj" && find . -type f | sort | xargs md5sum | md5sum) >> "$out"
      return $rc ;;
    db_bench)
      rm -rf "$d/dbdir"
      mkdir -p "$d/dbdir"
      "$@" "$d/bin" "$d/dbdir" 20000 > "$out" ;;
    *) "$@" "$d/bin" > "$out" ;;
  esac
}

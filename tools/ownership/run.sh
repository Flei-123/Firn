#!/bin/bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
C=$ROOT/tools/ownership/cases
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
pass=0
build() {
  "$FIRNC" ${OWN_NO_CACHE---no-cache} --ownership="$2" "$C/$1.fi" -o "$D/$1_$2" > "$D/$1_$2.log" 2>&1 || { echo "FAIL build $1 $2"; sed -n 1,5p "$D/$1_$2.log"; fail=$((fail + 1)); return 1; }
}
run() {
  (cd "$D" && timeout 60 "./$1_$2" > "$D/$1_$2.out" 2> "$D/$1_$2.err"); echo $?
}
expect_rc() {
  case " ${OWN_SKIP:-} " in *" $1 "*) return ;; esac
  build "$1" "$2" || return
  local rc
  rc=$(run "$1" "$2")
  if [ "$rc" = "$3" ]; then pass=$((pass + 1)); else echo "FAIL $1 $2: rc $rc, want $3"; fail=$((fail + 1)); fi
}
same_out() {
  case " ${OWN_SKIP:-} " in *" $1 "*) return ;; esac
  local ref=""
  for m in off on check strict; do
    build "$1" "$m" || return
    run "$1" "$m" > /dev/null
  done
  for m in on check strict; do
    if cmp -s "$D/$1_off.out" "$D/$1_$m.out"; then pass=$((pass + 1)); else echo "FAIL $1: output $m differs from off"; fail=$((fail + 1)); fi
  done
}
same_out concat_chain
expect_rc free_reuse on 0
expect_rc free_reuse strict 0
expect_rc free_reuse check 0
expect_rc poison_hit check 86
same_out tree_drop
expect_rc tree_drop_count on 0
expect_rc tree_drop_count strict 0
expect_rc tree_drop_count check 0
expect_rc tree_drop_bad check 86
same_out fir_str_loop
same_out fir_str_branch
same_out fir_str_param
same_out fir_str_ret
same_out fir_str_escape
same_out fir_str_escape_novec
expect_rc fir_str_freed on 0
expect_rc fir_str_freed strict 0
expect_rc fir_str_freed check 0
same_out fir_obj_loop
same_out fir_obj_branch
same_out fir_obj_field
same_out fir_obj_factory
same_out fir_obj_escape
FIRN_OWN_NO_REUSE=1 expect_rc fir_obj_freed on 0
FIRN_OWN_NO_REUSE=1 expect_rc fir_obj_freed strict 0
expect_rc fir_obj_freed check 0
same_out fir_vec_loop
same_out fir_vec_ret
same_out fir_vec_field
same_out fir_vec_iter
same_out fir_map_loop
expect_rc fir_vec_freed on 0
expect_rc fir_vec_freed strict 0
expect_rc fir_vec_freed check 0
expect_rc fir_child_vecret on 0
expect_rc fir_child_vecret strict 0
expect_rc fir_child_vecret check 0
same_out fir_child_alias
FIRN_OWN_NO_AST=1 same_out fir_child_alias
FIRN_OWN_NO_AST=1 expect_rc fir_child_tree on 0
FIRN_OWN_NO_AST=1 expect_rc fir_child_tree strict 0
FIRN_OWN_NO_AST=1 expect_rc fir_child_tree check 0
expect_rc fir_child_tree on 0
expect_rc fir_map_freed on 0
expect_rc fir_map_freed strict 0
expect_rc fir_map_freed check 0
expect_rc fir_vec_poison check 86
same_out fir_clo_local
same_out fir_clo_stored
FIRN_OWN_NO_REUSE=1 expect_rc fir_clo_freed on 0
FIRN_OWN_NO_REUSE=1 expect_rc fir_clo_freed strict 0
expect_rc fir_clo_freed check 0
has_reuse() {
  case " ${OWN_SKIP:-} " in *" $1 "*) return ;; esac
  local n
  n=$(FIRN_OWN_STATS=1 "$FIRNC" ${OWN_NO_CACHE---no-cache} --ownership=on "$C/$1.fi" -o "$D/$1_stats" 2>&1 | sed -n 's/.* \([0-9]*\) reuses.*/\1/p' | tail -1)
  if [ "${n:-0}" -gt 0 ]; then pass=$((pass + 1)); else echo "FAIL $1: no reuse"; fail=$((fail + 1)); fi
}
for c in fir_reuse_obj_loop fir_reuse_pairs fir_reuse_closure fir_reuse_mixed; do
  same_out "$c"
  has_reuse "$c"
done
same_out fir_tree_shared
FIRN_OWN_NO_AST=1 same_out fir_tree_shared
same_out fir_tree_after
FIRN_OWN_NO_AST=1 same_out fir_tree_after
same_out fir_tree_structcopy
FIRN_OWN_NO_AST=1 same_out fir_tree_structcopy
same_out fir_tree_rawcopy
FIRN_OWN_NO_AST=1 same_out fir_tree_rawcopy
same_out fir_tree_cycle
FIRN_OWN_NO_AST=1 same_out fir_tree_cycle
same_out fir_tree_calls
FIRN_OWN_NO_AST=1 same_out fir_tree_calls
same_out fir_tree_swap
FIRN_OWN_NO_AST=1 same_out fir_tree_swap
same_out fir_tree_threads
FIRN_OWN_NO_AST=1 same_out fir_tree_threads
same_out fir_mt_loop
same_out fir_vec_cycle
expect_rc strict_trip on 0
expect_rc strict_trip strict 86
[ -n "${OWN_FORK:-}" ] && same_out fork_mt_par
expect_rc fir_mt_freed on 0
expect_rc fir_mt_freed strict 0
expect_rc fir_mt_freed check 0
grep -q "reached the collector" "$D/strict_trip_strict.err" && pass=$((pass + 1)) || { echo "FAIL strict_trip: no message"; fail=$((fail + 1)); }
grep -q "use after free" "$D/poison_hit_check.err" && pass=$((pass + 1)) || { echo "FAIL poison_hit: no message"; fail=$((fail + 1)); }
echo "ownership: $pass passed, $fail failed"
[ "$fail" = 0 ]

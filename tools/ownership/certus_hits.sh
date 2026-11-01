#!/bin/bash
set -u
FIRNC="${FIRNC:-/root/firnc-gc/compiler/target/release/firnc}"
CERTUS="${CERTUS:-/root/certus-ui}"
OUT="${OUT:?set OUT=<result dir>}"
TARGETS="${TARGETS:-jsrun web1 browser}"
MODES="${MODES:-off on check}"
mkdir -p "$OUT/bin"
src_of() {
  case "$1" in
    jsrun) echo lib/js/run_main.fi ;;
    web1) echo lib/browser/web1_main.fi ;;
    browser) echo lib/browser/window_main.fi ;;
    *) return 1 ;;
  esac
}
sites() {
  objdump -d --no-show-raw-insn "$1" | grep -E 'call' | grep -oE '__str_concat3|__str_concat4_tail_check|__str_concat4_tail|__str_concat4|__own_drop_tree_check|__own_drop_tree|__gc_poison_block|__gc_free_block' | sort | uniq -c | awk '{printf "%s=%s ", $2, $1}'
}
printf 'target\tmode\tbytes\tsites\n' > "$OUT/hits.tsv"
for t in $TARGETS; do
  s=$(src_of "$t") || exit 2
  for m in $MODES; do
    b="$OUT/bin/${t}_$m"
    [ -x "$b" ] || (cd "$CERTUS" && FIRNC="$FIRNC" bash tools/work/mk.sh release-safe "$b" "$s" --ownership="$m" > "$OUT/bin/${t}_$m.log" 2>&1) || { echo "build failed: $t $m" >&2; continue; }
    printf '%s\t%s\t%s\t%s\n' "$t" "$m" "$(stat -c %s "$b")" "$(sites "$b")" | tee -a "$OUT/hits.tsv"
  done
done

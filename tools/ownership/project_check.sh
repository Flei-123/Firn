#!/bin/bash
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
source "$HERE/targets.sh"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
MODE=${FIRNC_OWNERSHIP:-off}
bad=0

emit() { echo "RESULT $1 $2 $3"; }

check_targets() {
  for t in "$@"; do
    if ! target_build "$t" "$MODE" "$D/$t" > "$D/$t.build.log" 2>&1; then
      emit "$t" build_fail 1
      sed -n 1,6p "$D/$t.build.log"
      bad=1
      continue
    fi
    target_run "$t" "$D/$t" "$D/$t.out" env
    rc=$?
    [ -f "$D/$t.out" ] || : > "$D/$t.out"
    if target_deterministic "$t"; then md5=$(md5sum < "$D/$t.out" | cut -c1-16); else md5=nondeterministic; fi
    emit "$t" "run_rc=$rc" "md5=$md5"
    [ "$rc" = 86 ] && echo "POISON $t exit 86"
    [ "$rc" = 0 ] || bad=1
  done
}

check_osum() {
  local osum=${OSUM_DIR:-/root/jarvis/projects/u_DiS4in7esMF1/osum}
  local lib=$osum/vendor/firn/lib
  for f in kernel/kmain.fi kernel/uprog.fi kernel/user/settings.fi; do
    if (cd "$osum" && FIRNLIB=$lib "$FIRNC" --no-cache --ownership="$MODE" "$f" -o "$D/osum.o") > "$D/osum.log" 2>&1; then
      emit "osum:$f" build_ok 0
    else
      emit "osum:$f" build_fail 1
      sed -n 1,6p "$D/osum.log"
      bad=1
    fi
  done
}

case ${1:-} in
  openplan) check_targets openplan_derive ;;
  fvc) for n in $FVC_NAMES; do check_targets "fvc_$n"; done ;;
  osum) check_osum ;;
  *) echo "usage: project_check.sh openplan|fvc|osum" >&2; exit 2 ;;
esac
exit $bad

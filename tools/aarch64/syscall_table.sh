#!/usr/bin/env bash
# tools/aarch64/syscall_table.sh -- THE TWO SYSTEM CALL TABLES HAVE TO AGREE.
#
# ROUND ARM-FREESTANDING. There are now two of them:
#
#   compiler/src/syscalls.rs   the Rust bootstrap's, in use since round 80
#   lib/firnc1/syscalls.fi     the self-hosted compiler's, new in this round
#
# The second one has no caller yet -- `firnc1` cannot generate A64, and says
# so (`--target=aarch64-linux` refuses with a message). A table without a
# caller is exactly the kind of thing that rots quietly, so it is compared
# here, every run, against the one that IS in use.
#
# How: the Firn table is not read as text, it is READ OUT OF A RUNNING
# PROGRAM (`bin/sysdump.fi`), built by BOTH compilers -- so the comparison
# also proves that firnc0 and firnc1 agree about the module. The Rust table
# is extracted from its source by the small awk below; if anybody ever
# changes the SHAPE of that table, this script stops finding it and fails
# loudly rather than comparing nothing against nothing.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="$ROOT/compiler/target/release/firnc"
FC1=${FIRNC1:-$ROOT/.firnc1}
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc is missing: $FIRNC"; exit 1; }

# --- the Rust table, as <canonical> <shape> <aarch64> -----------------------
# The shape numbers are the constants of lib/firnc1/syscalls.fi:
# 1 Direct, 2 AtFdcwd, 3 ForkClone, 4 Dup3, 5 SetThreadPointer, 6 Missing.
awk '
  /^const TABLE/      { inside = 1; next }
  inside && /^\];/    { inside = 0 }
  !inside             { next }
  {
    line = $0
    if (line !~ /A64::/) next
    if (match(line, /\(-?[0-9]+,/) == 0) next
    canon = substr(line, RSTART + 1, RLENGTH - 2)
    num = -1
    kind = 0
    if (line ~ /A64::Direct/)                 kind = 1
    else if (line ~ /A64::AtFdcwd/)           kind = 2
    else if (line ~ /A64::ForkClone/)         kind = 3
    else if (line ~ /A64::Dup3/)              kind = 4
    else if (line ~ /A64::SetThreadPointer/)  kind = 5
    else if (line ~ /A64::Missing/)           kind = 6
    if (kind == 0) next
    if (kind >= 1 && kind <= 4) {
      rest = substr(line, RSTART + RLENGTH)
      if (match(rest, /\(-?[0-9]+\)/) > 0) num = substr(rest, RSTART + 1, RLENGTH - 2)
    }
    print canon, kind, num
  }
' compiler/src/syscalls.rs | sort -n > "$TMPD/rust.txt"

n=$(wc -l < "$TMPD/rust.txt")
if [ "$n" -lt 40 ]; then
    bad "the Rust table could not be read ($n entries found) -- has its shape changed?"
    echo "FAIL: $fail"; exit 1
fi
ok "the Rust table was read: $n entries"

# --- the Firn table, out of a running program ------------------------------
"$FIRNC" bin/sysdump.fi -o "$TMPD/dump0" 2>"$TMPD/e0" \
    && "$TMPD/dump0" | sort -n > "$TMPD/firn0.txt" \
    || { bad "firnc0 cannot build bin/sysdump.fi"; sed 's/^/        /' "$TMPD/e0" | head -5; }

if [ -x "$FC1" ]; then
    "$FC1" bin/sysdump.fi -o "$TMPD/dump1" 2>"$TMPD/e1" \
        && "$TMPD/dump1" | sort -n > "$TMPD/firn1.txt" \
        || { bad "firnc1 cannot build bin/sysdump.fi"; sed 's/^/        /' "$TMPD/e1" | head -5; }
else
    echo "  (firnc1 is not built: $FC1 -- the stage 1 comparison is skipped)"
fi

if [ -f "$TMPD/firn0.txt" ]; then
    if diff -u "$TMPD/rust.txt" "$TMPD/firn0.txt" > "$TMPD/d0"; then
        ok "compiler/src/syscalls.rs == lib/firnc1/syscalls.fi (firnc0, $n entries)"
    else
        bad "the two tables differ"
        head -20 "$TMPD/d0" | sed 's/^/        /'
    fi
fi
if [ -f "$TMPD/firn1.txt" ] && [ -f "$TMPD/firn0.txt" ]; then
    if cmp -s "$TMPD/firn0.txt" "$TMPD/firn1.txt"; then
        ok "firnc0 and firnc1 read the same table out of the same module"
    else
        bad "firnc0 and firnc1 disagree about lib/firnc1/syscalls.fi"
        diff "$TMPD/firn0.txt" "$TMPD/firn1.txt" | head -10 | sed 's/^/        /'
    fi
fi

# --- and the refusal that goes with it -------------------------------------
if [ -x "$FC1" ]; then
    if "$FC1" --target=aarch64-linux tests/850_asm_basic.fi -o "$TMPD/x" >"$TMPD/t.err" 2>&1; then
        bad "firnc1 --target=aarch64-linux did NOT refuse (it cannot generate A64)"
    else
        grep -q "firnc1 cannot generate aarch64 yet" "$TMPD/t.err" \
            && ok "firnc1 refuses --target=aarch64-linux and says what is missing" \
            || { bad "firnc1 refuses with the wrong message"; head -2 "$TMPD/t.err" | sed 's/^/        /'; }
    fi
    # ... and accepts the two it really can do.
    "$FC1" --target=x86_64-linux tests/850_asm_basic.fi -o "$TMPD/y" >/dev/null 2>&1 \
        && "$TMPD/y"; rc=$?
    [ "$rc" -eq 42 ] && ok "firnc1 --target=x86_64-linux builds and runs (exit 42)" \
                     || bad "firnc1 --target=x86_64-linux: exit $rc, expected 42"
    "$FC1" --target=x86_64-none demos/freestanding/core.fi -o "$TMPD/k.o" >/dev/null 2>&1
    kind=$(readelf -h "$TMPD/k.o" 2>/dev/null | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "firnc1 --target=x86_64-none yields an ELF object file" \
                        || bad "firnc1 --target=x86_64-none: ELF kind '$kind', expected REL"
fi

echo "--------------------------------------------------------------------"
echo "SYSCALL TABLES: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1

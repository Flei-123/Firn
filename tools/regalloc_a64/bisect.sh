#!/bin/bash
# Bisect: which function, when register-allocated, breaks the program?
# Usage: bisect.sh <test.fi> <expected-exit>
cd /root/firn-regalloc
export FIRNLIB=$PWD/lib
F=$PWD/compiler/target/release/firnc
T="$1"; EXP="$2"
B=$(basename "$T" .fi)

# all function names of the program
"$F" --target=aarch64-linux --emit=asm -o /tmp/bs_$B.s "$T" >/dev/null 2>&1
mapfile -t FNS < <(grep '^_F[0-9]*\.' /tmp/bs_$B.s | sed 's/:$//' | sed 's/^_F[0-9]*\.//' | sort -u)
echo "functions: ${#FNS[@]}"

try() {   # $1 = comma list of names to allocate
  FIRN_RA_A64_ONLY="$1" "$F" --target=aarch64-linux -o /tmp/bs_$B "$T" >/dev/null 2>&1 || return 2
  timeout 30 qemu-aarch64 /tmp/bs_$B >/dev/null 2>&1
  local rc=$?
  rm -f /tmp/bs_$B
  [ "$rc" = "$EXP" ] && return 0 || return 1
}

# sanity: all off must be good, all on must be bad
if try "__NOTHING__"; then echo "baseline (none allocated): OK"; else echo "baseline BROKEN -- not an RA bug"; exit 1; fi

lo=0; hi=${#FNS[@]}
# shrink: keep halving the candidate set that still reproduces
cand=("${FNS[@]}")
while [ ${#cand[@]} -gt 1 ]; do
  half=$(( ${#cand[@]} / 2 ))
  first=("${cand[@]:0:$half}")
  second=("${cand[@]:$half}")
  j1=$(IFS=,; echo "${first[*]}")
  j2=$(IFS=,; echo "${second[*]}")
  if ! try "$j1"; then cand=("${first[@]}"); echo "  -> first half (${#cand[@]})"; continue; fi
  if ! try "$j2"; then cand=("${second[@]}"); echo "  -> second half (${#cand[@]})"; continue; fi
  echo "  neither half alone reproduces (interaction of ${#cand[@]}); stopping"
  break
done
echo "CULPRIT CANDIDATES: ${cand[*]}"

#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
printf '%-24s %8s %8s %8s\n' dir files casts derefs
total_files=0
total_casts=0
total_derefs=0
for d in lib/*/; do
    name=${d%/}
    files=$(grep -rlE '\*(mut|const) ' "$d" --include='*.fi' 2>/dev/null | wc -l)
    [ "$files" -eq 0 ] && continue
    casts=$(grep -rhoE 'as \*(mut|const) ' "$d" --include='*.fi' | wc -l)
    derefs=$(grep -rhoE '\(\*[a-z_][a-z_0-9.]*\)|^\s*\*\(' "$d" --include='*.fi' | wc -l)
    printf '%-24s %8d %8d %8d\n' "$name" "$files" "$casts" "$derefs"
    total_files=$((total_files + files))
    total_casts=$((total_casts + casts))
    total_derefs=$((total_derefs + derefs))
done
printf '%-24s %8d %8d %8d\n' total "$total_files" "$total_casts" "$total_derefs"

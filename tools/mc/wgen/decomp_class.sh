#!/usr/bin/env bash
set -euo pipefail
MOJ=${MC_MOJ:-/root/.w5/jars/server-1.20.1-mojang.jar}
JDK25=${JDK25:-/root/tools/jdk25}
VF=${VINEFLOWER:-/root/.w5/sr/vineflower-1.10.1.jar}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/in" "$W/out"
for c in "$@"; do
    p=$(echo "$c" | tr . /)
    unzip -qo "$MOJ" "$p.class" "$p\$*.class" -d "$W/in" 2>/dev/null || true
done
"$JDK25/bin/java" -jar "$VF" -dgs=1 -log=ERROR "$W/in" "$W/out" >/dev/null 2>&1
for c in "$@"; do
    cat "$W/out/$(echo "$c" | tr . /).java"
done

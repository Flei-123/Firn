#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
JDK25=${JDK25:-/root/tools/jdk25}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$JDK25/bin/javac" -d "$W" tools/mc/brigadier/CaseDump.java
"$JDK25/bin/java" -cp "$W" CaseDump | python3 -I tools/mc/brigadier/gen_casemap.py > lib/brigadier/casemap.fi

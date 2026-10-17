#!/usr/bin/env bash
# Keeps tests/data/fleitec-id-vectors_v2.json equal to the shared Fleitec-ID vectors of the JARVIS repo
# (the Node service fleitec-id passes the same file). Usage:
#   tools/fleitec_login/sync_vectors_v2.sh          copy the JARVIS file over the snapshot
#   tools/fleitec_login/sync_vectors_v2.sh --check  exit 1 when the snapshot differs (the JARVIS file may be absent: exit 0, "skipped")
set -eu
SRC="${FLEITEC_ID_VECTORS:-/root/jarvis/docs/fleitec-id/vectors_v2.json}"
DST="$(cd "$(dirname "$0")/../.." && pwd)/tests/data/fleitec-id-vectors_v2.json"
if [ ! -f "$SRC" ]; then echo "skipped: $SRC not found"; exit 0; fi
if [ "${1:-}" = "--check" ]; then
  cmp -s "$SRC" "$DST" && echo "vectors_v2 in sync" || { echo "vectors_v2 DIFFERS: run tools/fleitec_login/sync_vectors_v2.sh"; exit 1; }
else
  cp "$SRC" "$DST" && echo "copied $SRC"
fi

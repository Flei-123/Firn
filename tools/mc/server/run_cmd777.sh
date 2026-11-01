#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/server/run_cmd777.sh -- 26.3 (777): Firn server vs the vanilla 26.3 jar: declare_commands tree and tab complete of the supported commands.
set -uo pipefail
cd "$(dirname "$0")/../../.."
TOOLS=tools/mc/server
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
JAR=${JAR:-/root/.w5/jars/server-26.3.jar}
JAVA=${JAVA:-/root/tools/jdk25/bin/java}
REPORTS=${MC_REPORTS:-/root/.w5/rep-26.3/out/reports}
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); PIDS=""
cleanup() { exec 3>&- 4>&- 2>/dev/null; for p in $PIDS; do kill "$p" 2>/dev/null; done; sleep 1; for p in $PIDS; do kill -9 "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT
free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
OPS=$(python3 -I -c 'import hashlib,uuid,json
b=bytearray(hashlib.md5(b"OfflinePlayer:Bot").digest());b[6]=(b[6]&15)|0x30;b[8]=(b[8]&0x3f)|0x80
print(json.dumps([{"uuid":str(uuid.UUID(bytes=bytes(b))),"name":"Bot","level":4,"bypassesPlayerLimit":False}]))')
$FIRNC --opt-level=dev-fast -o "$W/fmc" demos/firnmc-server/main.fi 2>"$W/err" || { head -10 "$W/err"; echo "RESULT cmd777: FAIL"; exit 1; }
mkdir -p "$W/van" "$W/fir"
cp "$JAR" "$W/van/server.jar"; echo "eula=true" > "$W/van/eula.txt"; echo "$OPS" > "$W/van/ops.json"; echo "$OPS" > "$W/fir/ops.json"
VPORT=$(free_port); FPORT=$(free_port)
printf 'server-ip=127.0.0.1\nserver-port=%s\nonline-mode=false\ngamemode=creative\nlevel-type=minecraft\\:flat\nview-distance=4\nwhite-list=false\nenforce-whitelist=false\nspawn-protection=0\n' "$VPORT" > "$W/van/server.properties"
mkfifo "$W/vin" "$W/fin"; exec 3<>"$W/vin"; exec 4<>"$W/fin"
( cd "$W/van" && exec "$JAVA" -Xmx1G -jar server.jar nogui < "$W/vin" > "$W/van.log" 2>&1 ) & PIDS="$PIDS $!"
( cd "$W/fir" && exec "$W/fmc" nogui --protocol 777 --port "$FPORT" < "$W/fin" > "$W/fir.log" 2>&1 ) & PIDS="$PIDS $!"
for i in $(seq 1 90); do grep -aq 'Done (' "$W/van.log" 2>/dev/null && grep -aq 'Done (' "$W/fir.log" 2>/dev/null && break; sleep 1; done
grep -aq 'Done (' "$W/van.log" || { echo "  FAIL  vanilla did not start"; echo "RESULT cmd777: FAIL"; exit 1; }
printf 'tp 10 70 10\ntp 10 abc 10\ntp\ngamemode creative\ngamemode foo\ngamemode\ntime set 1000\ntime set noon\ntime set abc\ntime query gametime\nsay\nfoo\n?gamemode \n?time \n?tp \n' > "$W/cmds.txt"
node $TOOLS/bot777.cjs 127.0.0.1 $VPORT Bot "$W/cmds.txt" "$W/van.json" > "$W/bv.err" 2>&1 &
B1=$!
node $TOOLS/bot777.cjs 127.0.0.1 $FPORT Bot "$W/cmds.txt" "$W/fir.json" > "$W/bf.err" 2>&1 &
B2=$!
wait $B1 $B2
ERR=0
python3 -I - "$W" "$REPORTS/registries.json" "$TOOLS/cmd_tree_cmp.py" <<'PY' || ERR=1
import json, subprocess, sys
w, reg, cmp = sys.argv[1:4]
for n in ("van", "fir"):
    r = json.load(open("%s/%s.json" % (w, n)))
    if not r["declare"]:
        print("  FAIL  %s: no declare_commands" % n); sys.exit(1)
    json.dump(["10" + r["declare"]], open("%s/%s_decl.json" % (w, n), "w"))
p = subprocess.run([sys.executable, "-I", cmp, w + "/van_decl.json", w + "/fir_decl.json", reg, "gamemode,say,teleport,time,tp"], capture_output=True, text=True)
print("  %s  declare_commands: %s" % ("ok  " if p.returncode == 0 else "FAIL", (p.stdout.strip().splitlines() or ["?"])[0]))
if p.returncode:
    print(p.stdout[:800]); sys.exit(1)
v = json.load(open(w + "/van.json")); f = json.load(open(w + "/fir.json"))
bad = 0
for a, b in zip(v["tabs"], f["tabs"]):
    if a["start"] != b["start"] or a["length"] != b["length"] or a["matches"] != b["matches"]:
        bad += 1; print("  FAIL  tab txn %d: vanilla %s firn %s" % (a["txn"], a["matches"], b["matches"]))
print("  %s  tab complete: %d/%d equal " % ("ok  " if not bad and len(v["tabs"]) == len(f["tabs"]) else "FAIL", len(v["tabs"]) - bad, len(v["tabs"])))
sys.exit(1 if bad or len(v["tabs"]) != len(f["tabs"]) else 0)
PY
[ $ERR -eq 0 ] && echo "RESULT cmd777: ok" || echo "RESULT cmd777: FAIL"
echo stop >&3
exit $ERR

#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/window/cfhtml.sh -- round clip: the CF_HTML builder (lib/window/cfhtml.fi). Builds the block of several fragments,
# checks every offset against an independent Python count, and that extract gives the fragment back (also for foreign blocks).
Q="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT; cd "$Q" || exit 2
FIRNLIB="$Q/lib" "${FIRNC:-$Q/compiler/target/release/firnc}" --no-cache --opt-level=dev-fast -o "$T/cfhtml" lib/window/cfhtml_main.fi >/dev/null 2>"$T/b.err" || { cat "$T/b.err" | tail -5; echo "FAIL cfhtml (build)"; exit 1; }
python3 - "$T/cfhtml" <<'PY'
import subprocess, sys, re
exe = sys.argv[1]
bad = 0
for frag in ["<b>hi</b>", "", "<p>héllo € \U0001F600</p>", "<table><tr><td>x</td></tr></table>" * 200]:
    r = subprocess.run([exe], input=frag.encode(), capture_output=True)
    blk, back = r.stdout.split(b"\x01", 1)
    h = dict((k, int(v)) for k, v in re.findall(rb"(StartHTML|EndHTML|StartFragment|EndFragment):(\d{10})\r\n", blk[:200]) for k, v in [(k.decode(), v)])
    fb = frag.encode()
    ok = (h["StartHTML"] == blk.index(b"<html>") and h["EndHTML"] == len(blk)
          and blk[h["StartFragment"]:h["EndFragment"]] == fb
          and blk[h["StartFragment"] - 20:h["StartFragment"]] == b"<!--StartFragment-->"
          and blk[h["EndFragment"]:h["EndFragment"] + 18] == b"<!--EndFragment-->"
          and blk.startswith(b"Version:0.9\r\nStartHTML:0000000105\r\n") and back == fb and r.returncode == 0)
    print("ok  " if ok else "FAIL", "fragment of", len(fb), "octets; header", h)
    bad += 0 if ok else 1
sys.exit(1 if bad else 0)
PY
rc=$?; [ $rc = 0 ] && echo "PASS cfhtml" || echo "FAIL cfhtml"; exit $rc

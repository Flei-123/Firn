#!/usr/bin/env bash
# quick loop helper: cc.sh <seed> <count> [opt flag]
cd "$(dirname "$0")/../.."
export FIRNLIB=$PWD/lib
D=$(cat /tmp/mail_D)
/root/firn/compiler/target/release/firnc ${3:---opt-level=dev-fast} -o $D/check tools/mail/check.fi 2>&1 | head -20
python3 tools/mail/gen.py gen $1 $2 $D/cases $D/exp && $D/check $D/cases $D/res && python3 tools/mail/gen.py cmp $D/cases $D/exp $D/res 2>&1 | cut -c1-900 | head -${4:-50}

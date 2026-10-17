#!/bin/sh
# Harness: start server.py, run task.py, compare with expect.txt. Usage: check.sh [py|fi] [exe]
cd "$(dirname "$0")" || exit 2
export VSPY_PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
python3 server.py & SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do python3 -c "import socket;socket.create_connection(('127.0.0.1',$VSPY_PORT),0.2)" 2>/dev/null && break; sleep 0.1; done
if [ "$1" = fi ]; then "$2"; else python3 task.py; fi | diff - expect.txt && echo "OK $1"

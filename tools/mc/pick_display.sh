#!/usr/bin/env bash
lo=${1:-90}
span=${2:-100}
for _ in $(seq 1 80); do
    d=$((lo + RANDOM % span))
    if [ -e "/tmp/.X$d-lock" ]; then
        pid=$(tr -d ' \n' < "/tmp/.X$d-lock" 2>/dev/null)
        [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && continue
    fi
    if mkdir "/tmp/.mcdisp-$d" 2>/dev/null; then
        ( sleep 30; rmdir "/tmp/.mcdisp-$d" ) >/dev/null 2>&1 &
        echo "$d"
        exit 0
    fi
done
echo "$lo"

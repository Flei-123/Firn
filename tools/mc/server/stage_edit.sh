#!/usr/bin/env bash
# sourced by run.sh: stage I, block edits survive a save: Firn applies dig/place from a bot, stops (save), vanilla 1.20.1 loads the saved copy
EDIT_WORLD="$W/fedit/world"
if [ ! -f "$WORLD/level.dat" ]; then
    echo "  FAIL  saved world missing: $WORLD"
    ERR=1
else
    EPORT=$(free_port); VEPORT=$(free_port)
    mkdir -p "$W/fedit" "$W/vedit"
    cp -r "$WORLD" "$EDIT_WORLD"
    rm -f "$EDIT_WORLD/session.lock"
    "$W/mcsrv" --port "$EPORT" --world "$EDIT_WORLD" --ticks 700 > "$W/fe.out" 2>&1 &
    FEPID=$!
    sleep 2
    node "$TOOLS/bot_edit.cjs" 127.0.0.1 "$EPORT" build EditBot > "$W/edit_build.json"
    node "$TOOLS/check_edit.cjs" firn "$W/edit_build.json" | report
    wait $FEPID
    if grep -q 'stopped' "$W/fe.out"; then echo "  ok    firn server stopped and saved: $(grep stopped "$W/fe.out")"; else echo "  FAIL  firn server did not stop"; ERR=1; fi
    cp -r "$EDIT_WORLD" "$W/vedit/world"
    rm -f "$W/vedit/world/session.lock"
    node "$TOOLS/patch_level.cjs" "$W/vedit/world/level.dat" randomTickSpeed=0 doFireTick=false doMobSpawning=false doWeatherCycle=false doDaylightCycle=false
    printf 'online-mode=false\nserver-port=%s\nlevel-name=world\nview-distance=8\nsimulation-distance=4\nenforce-secure-profile=false\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\nmax-tick-time=-1\n' "$VEPORT" > "$W/vedit/server.properties"
    echo "eula=true" > "$W/vedit/eula.txt"
    mkfifo "$W/vedit/in"; exec 5<>"$W/vedit/in"
    ( cd "$W/vedit" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vedit/in" > server.out 2>&1 ) &
    VEPID=$!
    for i in $(seq 1 120); do grep -q 'Done (' "$W/vedit/server.out" 2>/dev/null && break; sleep 1; done
    grep -q 'Done (' "$W/vedit/server.out" || { echo "  FAIL  vanilla edit server did not start"; ERR=1; }
    node "$TOOLS/bot_edit.cjs" 127.0.0.1 "$VEPORT" rejoin EditBot > "$W/edit_rejoin.json"
    node "$TOOLS/check_edit.cjs" player "$W/edit_rejoin.json" "$W/edit_build.json" | report
    node -e '
const fs = require("fs")
const e = JSON.parse(fs.readFileSync(process.argv[1], "utf8")).expect
const lines = e.map((p) => "execute if block " + p.x + " " + p.y + " " + p.z + " " + p.block)
const q = e[0]
lines.push("execute if block " + q.x + " " + q.y + " " + q.z + " minecraft:diamond_block")
fs.writeFileSync(process.argv[2], lines.join("\n") + "\n")
' "$W/edit_build.json" "$W/vedit/cmds.txt"
    NCHK=$(wc -l < "$W/vedit/cmds.txt")
    idx=0
    while IFS= read -r cmd; do
        idx=$((idx+1))
        before=$(wc -l < "$W/vedit/server.out")
        echo "$cmd" >&5
        for i in $(seq 1 30); do [ "$(wc -l < "$W/vedit/server.out")" -gt "$before" ] && break; sleep 0.3; done
        sleep 0.3
        res=$(tail -n +$((before+1)) "$W/vedit/server.out" | grep -E 'Test (passed|failed)' | head -1)
        if [ "$idx" -eq "$NCHK" ]; then
            case "$res" in *failed*) echo "  ok    counter-check: a wrong block expectation is detected";; *) echo "  FAIL  counter-check did not detect a wrong expectation: '$res'"; ERR=1;; esac
        else
            case "$res" in *passed*) echo "  ok    vanilla: $cmd";; *) echo "  FAIL  vanilla: $cmd -> '$res'"; ERR=1;; esac
        fi
    done < "$W/vedit/cmds.txt"
    echo stop >&5
    wait $VEPID
    exec 5>&-
    bad=$(grep -E 'WARN|ERROR|Exception' "$W/vedit/server.out" | grep -viE "Can't keep up|Ambiguity|has been deprecated" | head -5)
    if [ -z "$bad" ]; then echo "  ok    vanilla loaded the saved copy without chunk warnings"; else echo "  FAIL  vanilla warnings:"; echo "$bad"; ERR=1; fi
fi

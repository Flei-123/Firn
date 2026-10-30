#!/usr/bin/env bash
# sourced by run.sh: stage H, saved world differential against the vanilla server on a copy of the same world
if [ ! -f "$WORLD/level.dat" ]; then
    echo "  FAIL  saved world missing: $WORLD (run tools/mc/save/run.sh once to create the cache)"
    ERR=1
else
    SVPORT=$(free_port); SFPORT=$(free_port)
    mkdir -p "$W/vsave"
    cp -r "$WORLD" "$W/vsave/world"
    cp -r "$WORLD" "$W/vsave/fworld"
    rm -f "$W/vsave/fworld/session.lock"
    rm -f "$W/vsave/world/session.lock"
    node "$TOOLS/patch_level.cjs" "$W/vsave/world/level.dat" randomTickSpeed=0 doFireTick=false doMobSpawning=false doWeatherCycle=false doDaylightCycle=false
    printf 'online-mode=false\nserver-port=%s\nlevel-name=world\nview-distance=8\nsimulation-distance=4\nenforce-secure-profile=false\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\nmax-tick-time=-1\n' "$SVPORT" > "$W/vsave/server.properties"
    echo "eula=true" > "$W/vsave/eula.txt"
    mkfifo "$W/vsave/in"; exec 4<>"$W/vsave/in"
    ( cd "$W/vsave" && exec java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui < "$W/vsave/in" > server.out 2>&1 ) &
    SVPID=$!
    "$W/mcsrv" --port "$SFPORT" --world "$W/vsave/fworld" --ticks 900 > "$W/fs.out" 2>&1 &
    for i in $(seq 1 120); do grep -q 'Done (' "$W/vsave/server.out" 2>/dev/null && break; sleep 1; done
    grep -q 'Done (' "$W/vsave/server.out" || { echo "  FAIL  vanilla save server did not start"; ERR=1; }
    BOT_HISTS=1 BOT_FULL=1 BOT_SECONDS=10 $BOT 127.0.0.1 $SVPORT join VSave > "$W/sv.json" &
    B1=$!
    BOT_HISTS=1 BOT_FULL=1 BOT_SECONDS=10 $BOT 127.0.0.1 $SFPORT join FSave > "$W/sf.json" &
    B2=$!
    wait $B1 $B2
    node "$TOOLS/check.cjs" world "$W/sf.json" "$W/sv.json" firn-save | report
    if node "$TOOLS/check.cjs" world-mutate "$W/sf.json" "$W/sv.json"; then echo "  ok    counter-check: a mutated saved-world column is detected"; else echo "  FAIL  counter-check did not detect the mutated column"; ERR=1; fi
    node "$TOOLS/check.cjs" extras "$W/sf.json" "$W/sv.json" firn-save | report
    if node "$TOOLS/check.cjs" extras-mutate "$W/sf.json" "$W/sv.json"; then echo "  ok    counter-check: a mutated biome / block entity column is detected"; else echo "  FAIL  counter-check did not detect the mutated biome / block entity column"; ERR=1; fi
    echo stop >&4
    wait
fi

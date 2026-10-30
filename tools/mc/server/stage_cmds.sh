#!/usr/bin/env bash
# sourced by run.sh: stage I, chat commands (tp, gamemode, time, say, error cases) and declare_commands against the real vanilla server
REPORTS=${MC_REPORTS:-/root/.w5/rep-1.20.1/out/reports}
CMD_PORT=$(free_port)
"$W/mcsrv" --port "$CMD_PORT" --ticks ${MCS_CMD_TICKS:-1300} > "$W/cmd_f.out" 2>&1 &
CF=$!
sleep 1
node "$TOOLS/cmd_capture.cjs" 127.0.0.1 $VPORT CmdVan "$W/decl_v.json" 5 &
C1=$!
node "$TOOLS/cmd_capture.cjs" 127.0.0.1 $CMD_PORT CmdFirn "$W/decl_f.json" 5 &
C2=$!
wait $C1 $C2
if python3 -I "$TOOLS/cmd_tree_cmp.py" "$W/decl_v.json" "$W/decl_f.json" "$REPORTS/registries.json" gamemode,say,teleport,time,tp > "$W/decl_cmp.out" 2>&1; then
    echo "  ok    declare_commands: $(head -1 "$W/decl_cmp.out")"
else
    echo "  FAIL  declare_commands: $(head -8 "$W/decl_cmp.out")"; touch "$W/failed"
fi
cp "$W/decl_f.json" "$W/decl_mut.json"
python3 -I - "$W/decl_mut.json" <<'PY'
import json, sys
p = sys.argv[1]
f = json.load(open(p))
h = f[-1]
f[-1] = h[:-4] + "0100" if h.endswith("00") else h
json.dump(f, open(p, "w"))
PY
if python3 -I "$TOOLS/cmd_tree_cmp.py" "$W/decl_v.json" "$W/decl_mut.json" "$REPORTS/registries.json" gamemode,say,teleport,time,tp > /dev/null 2>&1; then
    echo "  FAIL  counter-check: a mutated declare_commands frame was not detected"; touch "$W/failed"
else
    echo "  ok    counter-check: a mutated declare_commands frame is detected"
fi
unzip -p "${MC_VANILLA_JAR:-$JARS/client-1.20.1.jar}" assets/minecraft/lang/en_us.json > "$W/en_us.json"
export CMD_LANG="$W/en_us.json"
node "$TOOLS/cmd_bot.cjs" 127.0.0.1 $VPORT CmdVan "$TOOLS/cmd_list.txt" "$W/cmd_v.json" 2> "$W/cmd_bot_v.err" &
B1=$!
node "$TOOLS/cmd_bot.cjs" 127.0.0.1 $CMD_PORT CmdFirn "$TOOLS/cmd_list.txt" "$W/cmd_f.json" 2> "$W/cmd_bot_f.err" &
B2=$!
wait $B1 $B2
if [ -s "$W/cmd_v.json" ] && [ -s "$W/cmd_f.json" ]; then
    node "$TOOLS/cmd_check.cjs" run "$W/cmd_f.json" "$W/cmd_v.json" | report
    if node "$TOOLS/cmd_check.cjs" mutate "$W/cmd_f.json" "$W/cmd_v.json"; then echo "  ok    counter-check: a mutated chat line is detected"; else echo "  FAIL  counter-check: a mutated chat line was not detected"; touch "$W/failed"; fi
else
    echo "  FAIL  commands: a bot produced no output"; head -3 "$W/cmd_bot_v.err" "$W/cmd_bot_f.err"; touch "$W/failed"
fi
wait $CF
if [ -z "${MCS_NO_SEL:-}" ]; then
    source "$TOOLS/stage_sel.sh"
fi

# sourced by run.sh: stage J2, selector commands and tab_complete with two bots (op + non-op) against the real vanilla server
SEL_PORT=$(free_port)
"$W/mcsrv" --port "$SEL_PORT" --ops OpBot:4,Op2:2 --ticks ${MCS_SEL_TICKS:-2600} > "$W/sel_f.out" 2>&1 &
SF=$!
sleep 1
mkdir -p "$W/sel_f" "$W/sel_v" "$W/sel_bar_f" "$W/sel_bar_v"
[ -f "$W/en_us.json" ] || unzip -p "${MC_VANILLA_JAR:-$JARS/client-1.20.1.jar}" assets/minecraft/lang/en_us.json > "$W/en_us.json"
export CMD_LANG="$W/en_us.json"
REPORTS=${MC_REPORTS:-/root/.w5/rep-1.20.1/out/reports}
DB=""
for lvl in DeclNo Op2; do
    node "$TOOLS/cmd_capture.cjs" 127.0.0.1 $VPORT $lvl "$W/sel_decl_v_$lvl.json" 5 &
    DB="$DB $!"
    node "$TOOLS/cmd_capture.cjs" 127.0.0.1 $SEL_PORT $lvl "$W/sel_decl_f_$lvl.json" 5 &
    DB="$DB $!"
done
wait $DB
for lvl in DeclNo Op2; do
    EXPECT=full; [ "$lvl" = DeclNo ] && EXPECT=empty
    if python3 -I "$TOOLS/cmd_tree_cmp.py" "$W/sel_decl_v_$lvl.json" "$W/sel_decl_f_$lvl.json" "$REPORTS/registries.json" gamemode,say,teleport,time,tp $EXPECT > "$W/sel_decl_$lvl.out" 2>&1; then
        echo "  ok    declare_commands $lvl: $(head -1 "$W/sel_decl_$lvl.out")"
    else
        echo "  FAIL  declare_commands $lvl: $(head -8 "$W/sel_decl_$lvl.out")"; touch "$W/failed"
    fi
done
SB=""
for who in OpBot NoBot; do
    [ "$who" = NoBot ] && sleep 3
    node "$TOOLS/sel_bot.cjs" 127.0.0.1 $VPORT $who "$TOOLS/sel_list.txt" "$W/sel_v/sel_$who.json" "$W/sel_bar_v" 2> "$W/sel_bot_v_$who.err" &
    SB="$SB $!"
    node "$TOOLS/sel_bot.cjs" 127.0.0.1 $SEL_PORT $who "$TOOLS/sel_list.txt" "$W/sel_f/sel_$who.json" "$W/sel_bar_f" 2> "$W/sel_bot_f_$who.err" &
    SB="$SB $!"
done
wait $SB
if [ -s "$W/sel_v/sel_OpBot.json" ] && [ -s "$W/sel_f/sel_OpBot.json" ] && [ -s "$W/sel_v/sel_NoBot.json" ] && [ -s "$W/sel_f/sel_NoBot.json" ]; then
    node "$TOOLS/sel_check.cjs" run "$W/sel_f" "$W/sel_v" | report
    if node "$TOOLS/sel_check.cjs" mutate "$W/sel_f" "$W/sel_v"; then echo "  ok    counter-check: a mutated tab_complete answer is detected"; else echo "  FAIL  counter-check: a mutated tab_complete answer was not detected"; touch "$W/failed"; fi
else
    echo "  FAIL  selectors+tab: a bot produced no output"; head -3 "$W"/sel_bot_*.err; touch "$W/failed"
fi
kill $SF 2>/dev/null

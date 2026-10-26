#!/usr/bin/env bash
panel_case() {
  local nm=$1 mode=$2 kind=$3 rows=$4 want_n=$5 setup=$6 post=${7:-} wait_s=${8:-0} check=${9:-cont} line i wid cmd
  env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CONT_UI=$mode MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=40 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
      timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
  PP=$!
  for i in $(seq 1 90); do grep -q "$nm joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
  sleep 2
  echo "kill @e[type=minecraft:item]" >&3
  echo "execute at $nm run setblock ~1 ~ ~ minecraft:air" >&3
  echo "gamemode survival $nm" >&3
  echo "execute at $nm run $setup" >&3
  if [ -n "$post" ]; then
    local cmds
    IFS='|' read -ra cmds <<< "$post"
    for cmd in "${cmds[@]}"; do echo "execute at $nm run $cmd" >&3; done
  fi
  echo "give $nm stone 1" >&3; echo "give $nm dirt 1" >&3
  for i in $(seq 1 40); do grep -q 'contui open' "$W/$nm.out" 2>/dev/null && break; sleep 1; done
  line=$(grep 'contui open' "$W/$nm.out" | head -1)
  case "$line" in
    *" n=$want_n "*)
      sleep "$(python3 -c "print(1.5 + $wait_s)")"
      wid=$(xdotool search --name mcplay 2>/dev/null | head -1)
      xdotool mousemove --window "$wid" 5 5; sleep 0.8
      if xwd -id "$wid" -silent -out "$W/$nm.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$nm.xwd" "$W/$nm.ppm"; then
        python3 -I tools/mc/play/hud_check.py "$check" "$W/$nm.ppm" "$CJAR" "$kind" "$rows" || ERR=1
      else
        echo "  FAIL  panel $nm: screenshot"; ERR=1
      fi;;
    *) echo "  FAIL  panel $nm: container did not open (want n=$want_n): ${line:-no contui line}"; ERR=1;;
  esac
  xdotool key Escape; sleep 1
  wait $PP
}

run_panel_stage() {
  panel_case Player18 3 generic 3 27 "setblock ~1 ~ ~ minecraft:chest"
  panel_case Player19 4 furnace 3 3 "setblock ~1 ~ ~ minecraft:furnace"
  panel_case Player20 5 craft 3 10 "setblock ~1 ~ ~ minecraft:crafting_table"
  if [ "${RUN_INV_PANELS:-1}" = 1 ]; then
    panel_case Player21 7 dispenser 3 9 "setblock ~1 ~ ~ minecraft:dispenser"
    panel_case Player22 8 hopper 1 5 "setblock ~1 ~ ~ minecraft:hopper"
    panel_case Player23 9 blast_furnace 3 3 "setblock ~1 ~ ~ minecraft:blast_furnace"
    panel_case Player24 10 smoker 3 3 "setblock ~1 ~ ~ minecraft:smoker"
    panel_case Player25 11 brewing 3 5 "setblock ~1 ~ ~ minecraft:brewing_stand"
    panel_case Player26 12 grindstone 3 3 "setblock ~1 ~ ~ minecraft:grindstone"
    panel_case Player27 13 stonecutter 3 2 "setblock ~1 ~ ~ minecraft:stonecutter"
    panel_case Player28 14 loom 3 4 "setblock ~1 ~ ~ minecraft:loom"
    panel_case Player29 15 smithing 3 4 "setblock ~1 ~ ~ minecraft:smithing_table"
    panel_case Player30 16 cartography 3 3 "setblock ~1 ~ ~ minecraft:cartography_table"
    panel_case Player31 4 furnace 3 3 "setblock ~1 ~ ~ minecraft:furnace" "item replace block ~1 ~ ~ container.0 with minecraft:raw_iron 8|item replace block ~1 ~ ~ container.1 with minecraft:coal 8" 4 furnace_lit
  fi
}

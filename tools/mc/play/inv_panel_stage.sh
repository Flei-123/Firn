#!/usr/bin/env bash
panel_case() {
  local nm=$1 mode=$2 kind=$3 rows=$4 want_n=$5 setup=$6 line i wid
  env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CONT_UI=$mode MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=40 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
      timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
  PP=$!
  sleep 5
  echo "kill @e[type=minecraft:item]" >&3
  echo "execute at $nm run setblock ~1 ~ ~ minecraft:air" >&3
  echo "gamemode survival $nm" >&3
  echo "execute at $nm run $setup" >&3
  echo "give $nm stone 1" >&3; echo "give $nm dirt 1" >&3
  for i in $(seq 1 40); do grep -q 'contui open' "$W/$nm.out" 2>/dev/null && break; sleep 1; done
  line=$(grep 'contui open' "$W/$nm.out" | head -1)
  case "$line" in
    *" n=$want_n "*)
      sleep 1.5
      wid=$(xdotool search --name mcplay 2>/dev/null | head -1)
      xdotool mousemove --window "$wid" 5 5; sleep 0.8
      if xwd -id "$wid" -silent -out "$W/$nm.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$nm.xwd" "$W/$nm.ppm"; then
        python3 -I tools/mc/play/hud_check.py cont "$W/$nm.ppm" "$CJAR" "$kind" "$rows" || ERR=1
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
}
